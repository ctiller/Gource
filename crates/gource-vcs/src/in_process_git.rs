//! In-process git repository reading using gitoxide (`gix`).
//!
//! Traverses repository commits, extracts author/committer timestamps and names,
//! computes file tree changes against parent commits, and streams commits in
//! standard git log text format.

use crate::options::VcsOptions;
use crate::worktree::is_binary_buffer;
use gix::bstr::ByteSlice;
use std::io::Write;
use std::path::Path;
use tempfile::NamedTempFile;

/// Generate git log directly from the repository using `gix` (in-process).
/// Returns a NamedTempFile containing the generated log in git log format.
pub fn generate_in_process_git_log(
    repo_dir: &Path,
    options: &VcsOptions,
) -> Result<NamedTempFile, String> {
    let repo = gix::discover(repo_dir)
        .map_err(|e| format!("failed to open git repository in-process: {e}"))?;

    let head_commit = if !options.git_branch.is_empty() {
        match repo.rev_parse_single(options.git_branch.as_str()) {
            Ok(id) => id.detach(),
            Err(e) => {
                return Err(format!(
                    "failed to resolve branch {}: {e}",
                    options.git_branch
                ));
            }
        }
    } else {
        match repo.head() {
            Ok(head) => match head.try_into_peeled_id() {
                Ok(Some(id)) => id.detach(),
                Ok(None) => {
                    // Empty repository / unborn branch
                    let temp = NamedTempFile::new().map_err(|e| e.to_string())?;
                    return Ok(temp);
                }
                Err(e) => return Err(format!("failed to peel HEAD: {e}")),
            },
            Err(e) => match repo.rev_parse_single("HEAD") {
                Ok(id) => id.detach(),
                Err(_) => return Err(format!("failed to resolve HEAD: {e}")),
            },
        }
    };

    let mut revwalk = repo
        .rev_walk([head_commit])
        .sorting(gix::revision::walk::Sorting::BreadthFirst)
        .all()
        .map_err(|e| format!("failed to initialize revwalk: {e}"))?;

    let mut commit_ids = Vec::new();
    for info in revwalk.by_ref() {
        match info {
            Ok(info) => commit_ids.push(info.id),
            Err(e) => return Err(format!("error during revision walk: {e}")),
        }
    }

    // Chronological order: oldest to newest
    commit_ids.reverse();

    let mut temp = NamedTempFile::new().map_err(|e| e.to_string())?;
    let mut live_files: std::collections::HashMap<String, gix::ObjectId> =
        std::collections::HashMap::new();

    for id in commit_ids {
        let git_commit = match repo.find_object(id) {
            Ok(obj) => match obj.try_into_commit() {
                Ok(c) => c,
                Err(_) => continue,
            },
            Err(_) => continue,
        };

        let commit_time = match git_commit.time() {
            Ok(t) => t,
            Err(_) => continue,
        };

        let timestamp = if options.author_time {
            git_commit
                .author()
                .ok()
                .and_then(|a| a.time().ok())
                .map(|t| t.seconds)
                .unwrap_or(commit_time.seconds)
        } else {
            commit_time.seconds
        };

        if options.start_timestamp != 0 && timestamp < options.start_timestamp {
            continue;
        }
        if options.stop_timestamp != 0 && timestamp > options.stop_timestamp {
            continue;
        }

        let author_name = git_commit
            .author()
            .map(|a| a.name.to_str_lossy().into_owned())
            .unwrap_or_else(|_| "Unknown".to_string());

        if !options.filters.allows_user(&author_name) {
            continue;
        }

        let current_tree = match git_commit.tree() {
            Ok(t) => t,
            Err(_) => continue,
        };

        let parent_ids: Vec<_> = git_commit.parent_ids().collect();
        let changes = if parent_ids.is_empty() {
            // Initial commit
            collect_all_tree_files(
                &repo,
                &current_tree,
                options.include_numstat,
                &mut live_files,
            )
        } else if parent_ids.len() == 1 {
            if let Ok(parent_obj) = repo.find_object(parent_ids[0])
                && let Ok(parent_commit) = parent_obj.try_into_commit()
                && let Ok(parent_tree) = parent_commit.tree()
            {
                diff_trees(
                    &repo,
                    &parent_tree,
                    &current_tree,
                    options.include_numstat,
                    &mut live_files,
                )
            } else {
                Vec::new()
            }
        } else {
            diff_merge_commit(
                &repo,
                &parent_ids,
                &current_tree,
                options.include_numstat,
                &mut live_files,
            )
        };

        if changes.is_empty() {
            continue;
        }

        // Write commit header
        writeln!(temp, "user:{author_name}").map_err(|e| e.to_string())?;
        writeln!(temp, "{timestamp}").map_err(|e| e.to_string())?;

        // Write changes: raw line first, then optional numstat line so
        // parse_commit attaches stats to the raw entry with the true action code.
        for (path, action_code, is_bin, added, removed) in &changes {
            // Raw line: :100644 100644 0000000 0000000 <action>\t<path>
            writeln!(temp, ":100644 100644 0000000 0000000 {action_code}\t{path}")
                .map_err(|e| e.to_string())?;
            if options.include_numstat {
                if *is_bin {
                    writeln!(temp, "-\t-\t{path}").map_err(|e| e.to_string())?;
                } else {
                    writeln!(temp, "{added}\t{removed}\t{path}").map_err(|e| e.to_string())?;
                }
            }
        }

        // Empty line separates commits in git log output
        writeln!(temp).map_err(|e| e.to_string())?;
    }

    temp.flush().map_err(|e| e.to_string())?;
    Ok(temp)
}

fn collect_all_tree_files(
    repo: &gix::Repository,
    tree: &gix::Tree,
    include_numstat: bool,
    live_files: &mut std::collections::HashMap<String, gix::ObjectId>,
) -> Vec<(String, String, bool, u32, u32)> {
    let mut files = Vec::new();
    let mut stack = vec![(String::new(), tree.clone())];

    while let Some((prefix, cur_tree)) = stack.pop() {
        for entry in cur_tree.iter().flatten() {
            let name = entry.filename().to_str_lossy();
            let full_path = if prefix.is_empty() {
                name.into_owned()
            } else {
                format!("{prefix}/{name}")
            };

            let mode = entry.mode();
            let oid = entry.oid().to_owned();
            if mode.is_tree() {
                if let Ok(sub_obj) = repo.find_object(oid)
                    && let Ok(sub_tree) = sub_obj.try_into_tree()
                {
                    stack.push((full_path, sub_tree));
                }
            } else if mode.is_blob() || mode.is_blob_or_symlink() {
                let (lines, is_bin) = if include_numstat {
                    count_blob_lines(repo, oid)
                } else {
                    (0, false)
                };
                live_files.insert(full_path.clone(), oid);
                files.push((full_path, "A".to_string(), is_bin, lines, 0));
            }
        }
    }
    files.sort_by(|a, b| a.0.cmp(&b.0));
    files
}

fn diff_trees(
    repo: &gix::Repository,
    old_tree: &gix::Tree,
    new_tree: &gix::Tree,
    include_numstat: bool,
    live_files: &mut std::collections::HashMap<String, gix::ObjectId>,
) -> Vec<(String, String, bool, u32, u32)> {
    use std::collections::HashMap;

    let mut old_files = HashMap::new();
    let mut new_files = HashMap::new();

    diff_tree_entries(
        repo,
        old_tree,
        new_tree,
        String::new(),
        &mut old_files,
        &mut new_files,
    );

    let mut results = Vec::new();

    for (path, (new_id, _)) in &new_files {
        live_files.insert(path.clone(), *new_id);
        match old_files.get(path) {
            None => {
                let (lines, is_bin) = if include_numstat {
                    count_blob_lines(repo, *new_id)
                } else {
                    (0, false)
                };
                results.push((path.clone(), "A".to_string(), is_bin, lines, 0));
            }
            Some((old_id, _)) => {
                if old_id != new_id {
                    let (added, removed, is_bin) = if include_numstat {
                        compute_line_diff(repo, *old_id, *new_id)
                    } else {
                        (0, 0, false)
                    };
                    results.push((path.clone(), "M".to_string(), is_bin, added, removed));
                }
            }
        }
    }

    for (path, (old_id, _)) in &old_files {
        if !new_files.contains_key(path) {
            live_files.remove(path);
            let (lines, is_bin) = if include_numstat {
                count_blob_lines(repo, *old_id)
            } else {
                (0, false)
            };
            results.push((path.clone(), "D".to_string(), is_bin, 0, lines));
        }
    }

    results.sort_by(|a, b| a.0.cmp(&b.0));
    results
}

fn diff_tree_entries(
    repo: &gix::Repository,
    old_tree: &gix::Tree,
    new_tree: &gix::Tree,
    prefix: String,
    old_out: &mut std::collections::HashMap<String, (gix::ObjectId, gix::objs::tree::EntryMode)>,
    new_out: &mut std::collections::HashMap<String, (gix::ObjectId, gix::objs::tree::EntryMode)>,
) {
    use std::collections::HashMap;

    let mut old_map = HashMap::new();
    for entry in old_tree.iter().flatten() {
        old_map.insert(
            entry.filename().to_str_lossy().into_owned(),
            (entry.oid().to_owned(), entry.mode()),
        );
    }

    for entry in new_tree.iter().flatten() {
        let name = entry.filename().to_str_lossy().into_owned();
        let new_oid = entry.oid().to_owned();
        let new_mode = entry.mode();
        let full_path = if prefix.is_empty() {
            name.clone()
        } else {
            format!("{prefix}/{name}")
        };

        if let Some((old_oid, old_mode)) = old_map.remove(&name) {
            if old_oid == new_oid && old_mode == new_mode {
                // Unchanged subtree or blob: skip entirely!
                continue;
            }
            if old_mode.is_tree()
                && new_mode.is_tree()
                && let Ok(old_obj) = repo.find_object(old_oid)
                && let Ok(old_sub) = old_obj.try_into_tree()
                && let Ok(new_obj) = repo.find_object(new_oid)
                && let Ok(new_sub) = new_obj.try_into_tree()
            {
                diff_tree_entries(repo, &old_sub, &new_sub, full_path, old_out, new_out);
                continue;
            }
            // Mode changed between tree and blob or modified blob
            if old_mode.is_tree() {
                if let Ok(old_obj) = repo.find_object(old_oid)
                    && let Ok(old_sub) = old_obj.try_into_tree()
                {
                    collect_tree_entries(repo, &old_sub, full_path.clone(), old_out);
                }
            } else if old_mode.is_blob() || old_mode.is_blob_or_symlink() {
                old_out.insert(full_path.clone(), (old_oid, old_mode));
            }
        }

        if new_mode.is_tree() {
            if let Ok(new_obj) = repo.find_object(new_oid)
                && let Ok(new_sub) = new_obj.try_into_tree()
            {
                collect_tree_entries(repo, &new_sub, full_path, new_out);
            }
        } else if new_mode.is_blob() || new_mode.is_blob_or_symlink() {
            new_out.insert(full_path, (new_oid, new_mode));
        }
    }

    for (name, (old_oid, old_mode)) in old_map {
        let full_path = if prefix.is_empty() {
            name
        } else {
            format!("{prefix}/{name}")
        };
        if old_mode.is_tree() {
            if let Ok(old_obj) = repo.find_object(old_oid)
                && let Ok(old_sub) = old_obj.try_into_tree()
            {
                collect_tree_entries(repo, &old_sub, full_path, old_out);
            }
        } else if old_mode.is_blob() || old_mode.is_blob_or_symlink() {
            old_out.insert(full_path, (old_oid, old_mode));
        }
    }
}

fn diff_merge_commit(
    repo: &gix::Repository,
    parent_ids: &[gix::Id<'_>],
    current_tree: &gix::Tree,
    include_numstat: bool,
    live_files: &mut std::collections::HashMap<String, gix::ObjectId>,
) -> Vec<(String, String, bool, u32, u32)> {
    use std::collections::HashMap;

    let mut parent_maps = Vec::with_capacity(parent_ids.len());
    for &pid in parent_ids {
        if let Ok(p_obj) = repo.find_object(pid)
            && let Ok(p_commit) = p_obj.try_into_commit()
            && let Ok(p_tree) = p_commit.tree()
        {
            let mut map = HashMap::new();
            collect_tree_entries(repo, &p_tree, String::new(), &mut map);
            parent_maps.push(map);
        }
    }
    if parent_maps.is_empty() {
        return Vec::new();
    }

    let mut new_files = HashMap::new();
    collect_tree_entries(repo, current_tree, String::new(), &mut new_files);

    let mut results = Vec::new();

    // 1. Deletions: any file currently alive in `live_files` (from any ancestor branch)
    //    or present in all parents that is absent from `current_tree` must be deleted.
    let mut deleted: HashMap<String, gix::ObjectId> = HashMap::new();
    for (path, &old_id) in live_files.iter() {
        if !new_files.contains_key(path) {
            deleted.insert(path.clone(), old_id);
        }
    }
    for (path, (old_id, _)) in &parent_maps[0] {
        if !new_files.contains_key(path) && parent_maps[1..].iter().all(|pm| pm.contains_key(path))
        {
            deleted.entry(path.clone()).or_insert(*old_id);
        }
    }
    for (path, old_id) in deleted {
        live_files.remove(&path);
        let (lines, is_bin) = if include_numstat {
            count_blob_lines(repo, old_id)
        } else {
            (0, false)
        };
        results.push((path, "D".to_string(), is_bin, 0, lines));
    }

    // 2. Additions and modifications in the merge commit itself:
    //    If a file in `current_tree` matches at least one parent's OID and is already in
    //    `live_files`, it was already emitted by that branch's commits.
    for (path, (new_id, _)) in &new_files {
        let matches_any_parent = parent_maps
            .iter()
            .any(|pm| pm.get(path).is_some_and(|(pid, _)| pid == new_id));
        let in_any_parent = parent_maps.iter().any(|pm| pm.contains_key(path));
        let was_live = live_files.insert(path.clone(), *new_id);

        if matches_any_parent && (was_live.is_some() || in_any_parent) {
            continue;
        }

        if !in_any_parent && was_live.is_none() {
            let (lines, is_bin) = if include_numstat {
                count_blob_lines(repo, *new_id)
            } else {
                (0, false)
            };
            results.push((path.clone(), "A".to_string(), is_bin, lines, 0));
        }
    }

    results.sort_by(|a, b| a.0.cmp(&b.0));
    results
}

fn collect_tree_entries(
    repo: &gix::Repository,
    tree: &gix::Tree,
    prefix: String,
    out: &mut std::collections::HashMap<String, (gix::ObjectId, gix::objs::tree::EntryMode)>,
) {
    for entry in tree.iter().flatten() {
        let name = entry.filename().to_str_lossy();
        let full_path = if prefix.is_empty() {
            name.into_owned()
        } else {
            format!("{prefix}/{name}")
        };

        let mode = entry.mode();
        if mode.is_tree() {
            if let Ok(obj) = repo.find_object(entry.oid())
                && let Ok(sub) = obj.try_into_tree()
            {
                collect_tree_entries(repo, &sub, full_path, out);
            }
        } else if mode.is_blob() || mode.is_blob_or_symlink() {
            out.insert(full_path, (entry.oid().to_owned(), mode));
        }
    }
}

fn count_blob_lines(repo: &gix::Repository, id: gix::ObjectId) -> (u32, bool) {
    if let Ok(obj) = repo.find_object(id) {
        let bytes = obj.data.as_bytes();
        if is_binary_buffer(bytes) {
            (0, true)
        } else {
            (bytes.iter().filter(|&&b| b == b'\n').count() as u32, false)
        }
    } else {
        (0, false)
    }
}

fn compute_line_diff(
    repo: &gix::Repository,
    old_id: gix::ObjectId,
    new_id: gix::ObjectId,
) -> (u32, u32, bool) {
    let old_bytes = repo
        .find_object(old_id)
        .map(|o| o.data.to_vec())
        .unwrap_or_default();
    let new_bytes = repo
        .find_object(new_id)
        .map(|o| o.data.to_vec())
        .unwrap_or_default();

    if is_binary_buffer(&old_bytes) || is_binary_buffer(&new_bytes) {
        return (0, 0, true);
    }

    let old_str = String::from_utf8_lossy(&old_bytes);
    let new_str = String::from_utf8_lossy(&new_bytes);

    let (added, removed) = compute_diff_lines(&old_str, &new_str);
    (added, removed, false)
}

fn compute_diff_lines(old_str: &str, new_str: &str) -> (u32, u32) {
    let mut added = 0;
    let mut removed = 0;

    let old_lines: Vec<&str> = old_str.lines().collect();
    let new_lines: Vec<&str> = new_str.lines().collect();

    let mut old_idx = 0;
    let mut new_idx = 0;

    while old_idx < old_lines.len() && new_idx < new_lines.len() {
        if old_lines[old_idx] == new_lines[new_idx] {
            old_idx += 1;
            new_idx += 1;
        } else if let Some(pos) = new_lines[new_idx..]
            .iter()
            .position(|&l| l == old_lines[old_idx])
        {
            added += pos as u32;
            new_idx += pos;
        } else {
            removed += 1;
            old_idx += 1;
        }
    }
    if old_idx < old_lines.len() {
        removed += (old_lines.len() - old_idx) as u32;
    }
    if new_idx < new_lines.len() {
        added += (new_lines.len() - new_idx) as u32;
    }

    (added, removed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_binary_buffer() {
        assert!(is_binary_buffer(b"hello\0world"));
        assert!(!is_binary_buffer(b"hello world\nline 2"));
    }

    #[test]
    fn test_compute_diff_lines() {
        // Complete replacement
        assert_eq!(compute_diff_lines("a\nb\nc", "x\ny\nz"), (3, 3));

        // Match in middle
        assert_eq!(compute_diff_lines("a\nb\nc", "x\nb\nz"), (2, 2));

        // Additions only
        assert_eq!(compute_diff_lines("a", "a\nb\nc"), (2, 0));

        // Deletions only
        assert_eq!(compute_diff_lines("a\nb\nc", "a"), (0, 2));

        // Empty file handling
        assert_eq!(compute_diff_lines("", "a\n"), (1, 0));
        assert_eq!(compute_diff_lines("a\n", ""), (0, 1));
        assert_eq!(compute_diff_lines("", ""), (0, 0));

        // Unchanged
        assert_eq!(compute_diff_lines("a\nb", "a\nb"), (0, 0));
    }
}

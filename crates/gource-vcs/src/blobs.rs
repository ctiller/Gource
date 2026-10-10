//! File sizes for git logs: the size of each blob a `--raw` log names.
//!
//! `git log --raw` lines carry the object name of the file after the
//! commit, not its size. [`blob_sizes`] collects the distinct names from a
//! log and asks git for all of them with one `git cat-file --batch-check`
//! process, so the cost is one extra process however long the history is.

use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Command, Stdio};

/// Blob sizes in bytes, keyed by the object name as written in the log
/// (abbreviated or not).
pub type BlobSizes = HashMap<String, u64>;

/// The new object name of a `git log --raw` line
/// (`:100644 100644 <old> <new> M\t<path>`), if it names a file that exists
/// after the commit.
pub fn raw_new_blob(line: &str) -> Option<&str> {
    let head = line.strip_prefix(':')?.split('\t').next()?;
    let mut f = head.split(' ');
    let (_old_mode, new_mode) = (f.next()?, f.next()?);
    let (_old, new) = (f.next()?, f.next()?);
    // Deletions have the all-zero name; submodules (mode 160000) are
    // commits, not blobs.
    if new.is_empty() || new.bytes().all(|b| b == b'0') || new_mode == "160000" {
        return None;
    }
    new.bytes().all(|b| b.is_ascii_hexdigit()).then_some(new)
}

/// Sizes of every blob named by the `--raw` lines of `log`, read from the
/// repository at `repo_dir`. Names git cannot resolve (missing, ambiguous
/// abbreviations) are left out.
pub fn blob_sizes(repo_dir: &Path, log: impl BufRead) -> Result<BlobSizes, String> {
    let mut seen = HashSet::new();
    let mut names = Vec::new();
    for line in log.split(b'\n') {
        let line = line.map_err(|e| e.to_string())?;
        if line.first() != Some(&b':') {
            continue;
        }
        if let Some(name) = raw_new_blob(&String::from_utf8_lossy(&line))
            && seen.insert(name.to_string())
        {
            names.push(name.to_string());
        }
    }
    if names.is_empty() {
        return Ok(BlobSizes::new());
    }
    let mut child = Command::new("git")
        .args(["cat-file", "--batch-check"])
        .current_dir(repo_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("cannot run git: {e}"))?;
    let mut stdin = child.stdin.take().ok_or("no stdin")?;
    let stdout = child.stdout.take().ok_or("no stdout")?;
    // Write on a thread: git answers while it reads, and both pipes fill.
    let input = names.clone();
    let writer = std::thread::spawn(move || {
        for n in &input {
            if writeln!(stdin, "{n}").is_err() {
                break;
            }
        }
    });
    let mut sizes = BlobSizes::with_capacity(names.len());
    for (name, answer) in names.iter().zip(BufReader::new(stdout).lines()) {
        let answer = answer.map_err(|e| e.to_string())?;
        // "<oid> blob <size>"; "<name> missing" / "<name> ambiguous" otherwise.
        let mut parts = answer.split(' ');
        if let (Some(_), Some("blob"), Some(size)) = (parts.next(), parts.next(), parts.next())
            && let Ok(size) = size.parse()
        {
            sizes.insert(name.clone(), size);
        }
    }
    let _ = writer.join();
    let _ = child.wait();
    Ok(sizes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_lines() {
        assert_eq!(
            raw_new_blob(":100644 100644 e69de29 3b18e51 M\tsrc/a.rs"),
            Some("3b18e51")
        );
        assert_eq!(
            raw_new_blob(":000000 100644 0000000 3b18e51 A\ta"),
            Some("3b18e51")
        );
        // Deleted, submodule, not a raw line.
        assert_eq!(raw_new_blob(":100644 000000 3b18e51 0000000 D\ta"), None);
        assert_eq!(raw_new_blob(":160000 160000 3b18e51 4b825dc M\tsub"), None);
        assert_eq!(raw_new_blob("12\t3\tfile"), None);
        assert_eq!(raw_new_blob(":100644 100644 a zz M\tx"), None);
        assert_eq!(raw_new_blob(":100644"), None);
    }

    #[test]
    fn no_raw_lines_needs_no_git() {
        let sizes = blob_sizes(Path::new("/nonexistent"), &b"user:x\n12\n"[..]).unwrap();
        assert!(sizes.is_empty());
    }
}

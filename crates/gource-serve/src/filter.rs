//! Server-side subscription filters.

use gource_model::wire::FilterSpec;
use gource_vcs::{Commit, CommitFilters};

/// A compiled [`FilterSpec`].
#[derive(Debug, Clone, Default)]
pub struct Filter {
    filters: CommitFilters,
    empty: bool,
}

fn compile(list: &[String]) -> Result<Vec<fancy_regex::Regex>, String> {
    list.iter()
        .map(|r| fancy_regex::Regex::new(r).map_err(|e| format!("invalid filter '{r}': {e}")))
        .collect()
}

impl Filter {
    pub fn compile(spec: &FilterSpec) -> Result<Self, String> {
        Ok(Self {
            filters: CommitFilters {
                file_filters: compile(&spec.file_filters)?,
                file_show_filters: compile(&spec.file_show_filters)?,
                user_filters: compile(&spec.user_filters)?,
                user_show_filters: compile(&spec.user_show_filters)?,
            },
            empty: spec.is_empty(),
        })
    }

    /// The commit as seen through the filter: files that are hidden are
    /// removed, and `None` if the user is hidden or no files remain.
    pub fn apply(&self, commit: &Commit) -> Option<Commit> {
        if self.empty {
            return (!commit.files.is_empty()).then(|| commit.clone());
        }
        if !self.filters.allows_user(&commit.username) {
            return None;
        }
        let files: Vec<_> = commit
            .files
            .iter()
            .filter(|f| {
                // Match the name as it appears in the log (no leading `/`),
                // like `--file-filter`.
                let raw = f.filename.strip_prefix('/').unwrap_or(&f.filename);
                self.filters.allows_file(raw)
            })
            .cloned()
            .collect();
        if files.is_empty() {
            return None;
        }
        Some(Commit {
            files,
            username: commit.username.clone(),
            timestamp: commit.timestamp,
            is_shadow: commit.is_shadow,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gource_vcs::CommitFile;

    fn commit(user: &str, files: &[&str]) -> Commit {
        Commit {
            timestamp: 5,
            username: user.into(),
            files: files
                .iter()
                .map(|f| CommitFile {
                    filename: f.to_string(),
                    ..Default::default()
                })
                .collect(),
            is_shadow: false,
        }
    }

    fn names(c: &Commit) -> Vec<&str> {
        c.files.iter().map(|f| f.filename.as_str()).collect()
    }

    #[test]
    fn empty_filter_passes_commits_with_files() {
        let f = Filter::compile(&FilterSpec::default()).unwrap();
        let c = commit("a", &["/x"]);
        assert_eq!(f.apply(&c), Some(c));
        assert_eq!(f.apply(&commit("a", &[])), None);
    }

    #[test]
    fn file_filters_hide_and_show() {
        let f = Filter::compile(&FilterSpec {
            file_filters: vec!["\\.o$".into()],
            ..Default::default()
        })
        .unwrap();
        let c = f.apply(&commit("a", &["/x.c", "/x.o"])).unwrap();
        assert_eq!(names(&c), ["/x.c"]);
        assert_eq!(f.apply(&commit("a", &["/x.o"])), None);

        let f = Filter::compile(&FilterSpec {
            file_show_filters: vec!["^src/".into()],
            ..Default::default()
        })
        .unwrap();
        let c = f.apply(&commit("a", &["/src/a", "/doc/b"])).unwrap();
        assert_eq!(names(&c), ["/src/a"]);
    }

    #[test]
    fn user_filters_hide_and_show() {
        let hide = Filter::compile(&FilterSpec {
            user_filters: vec!["bot".into()],
            ..Default::default()
        })
        .unwrap();
        assert_eq!(hide.apply(&commit("dependabot", &["/x"])), None);
        assert!(hide.apply(&commit("alice", &["/x"])).is_some());

        let show = Filter::compile(&FilterSpec {
            user_show_filters: vec!["^alice$".into()],
            ..Default::default()
        })
        .unwrap();
        assert!(show.apply(&commit("alice", &["/x"])).is_some());
        assert_eq!(show.apply(&commit("bob", &["/x"])), None);
    }

    #[test]
    fn invalid_regex_is_reported() {
        let err = Filter::compile(&FilterSpec {
            user_show_filters: vec!["(".into()],
            ..Default::default()
        })
        .unwrap_err();
        assert!(err.contains("invalid filter '('"), "{err}");
    }
}

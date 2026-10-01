//! The commit model (port of `RCommit` / `RCommitFile`).

/// White, the colour of files without an extension.
pub const WHITE: [f32; 3] = [1.0, 1.0, 1.0];

/// What happened to a file in a commit.
///
/// Parsers pass through whatever action code the log contains; the
/// simulation treats `Delete` as removal, `Add` as creation and everything
/// else as a modification.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub enum FileAction {
    /// "A"
    #[default]
    Add,
    /// "M"
    Modify,
    /// "D"
    Delete,
    /// Any other code, kept verbatim so `--output-custom-log` round-trips.
    Other(String),
}

impl FileAction {
    pub fn from_code(code: &str) -> Self {
        match code {
            "A" => FileAction::Add,
            "M" => FileAction::Modify,
            "D" => FileAction::Delete,
            other => FileAction::Other(other.to_owned()),
        }
    }

    pub fn code(&self) -> &str {
        match self {
            FileAction::Add => "A",
            FileAction::Modify => "M",
            FileAction::Delete => "D",
            FileAction::Other(code) => code,
        }
    }
}

/// A file touched by a commit.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CommitFile {
    /// UTF-8 filtered path, always starting with `/`.
    pub filename: String,
    pub action: FileAction,
    /// RGB colour from the log (custom format) or derived from the extension.
    pub colour: [f32; 3],
    pub lines_added: Option<u32>,
    pub lines_removed: Option<u32>,
    pub is_binary: bool,
    /// Whether this file represents uncommitted in-flight worktree state.
    pub is_shadow: bool,
}

/// A commit: who, when, and which files.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Commit {
    /// Unix timestamp in seconds.
    pub timestamp: i64,
    /// UTF-8 filtered user name.
    pub username: String,
    pub files: Vec<CommitFile>,
    /// Whether this commit represents uncommitted in-flight worktree state.
    pub is_shadow: bool,
}

impl Commit {
    /// Sanitise the user name to valid UTF-8 (`RCommit::postprocess`).
    pub fn postprocess(&mut self) {
        self.username = gource_core::utf8::filter_utf8(self.username.as_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_codes_round_trip() {
        for code in ["A", "M", "D", "R", "X"] {
            assert_eq!(FileAction::from_code(code).code(), code);
        }
        assert_eq!(FileAction::from_code("R"), FileAction::Other("R".into()));
    }

    #[test]
    fn postprocess_filters_username() {
        let mut c = Commit {
            username: "ok\u{fffd}".into(),
            ..Default::default()
        };
        c.postprocess();
        assert!(c.username.starts_with("ok"));
        assert_eq!(CommitFile::default().colour, [0.0; 3]);
        assert_eq!(WHITE, [1.0; 3]);
    }
}

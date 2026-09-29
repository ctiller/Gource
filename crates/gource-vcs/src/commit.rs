//! The commit model (port of `RCommit` / `RCommitFile`).

use glam::Vec3;

/// What happened to a file in a commit.
///
/// Parsers pass through whatever action code the log contains; the
/// simulation treats `Delete` as removal, `Add` as creation and everything
/// else as a modification.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum FileAction {
    /// "A"
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
#[derive(Debug, Clone, PartialEq)]
pub struct CommitFile {
    /// UTF-8 filtered path, always starting with `/`.
    pub filename: String,
    pub action: FileAction,
    /// Colour from the log (custom format) or derived from the extension.
    pub colour: Vec3,
}

/// A commit: who, when, and which files.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Commit {
    /// Unix timestamp in seconds.
    pub timestamp: i64,
    /// UTF-8 filtered user name.
    pub username: String,
    pub files: Vec<CommitFile>,
}

impl Commit {
    /// Add a file with derived colour from extension.
    ///
    /// Applies filters to the raw filename, derives colour with `options.hasher`,
    /// then normalises the filename (UTF-8 filtered with a leading '/').
    pub fn add_file(
        &mut self,
        raw_filename: &str,
        action: &str,
        options: &crate::options::VcsOptions,
    ) {
        let colour = file_colour(raw_filename, &options.hasher);
        self.add_file_with_colour(raw_filename, action, colour, options);
    }

    /// Add a file with explicit colour (e.g. from custom log format).
    pub fn add_file_with_colour(
        &mut self,
        raw_filename: &str,
        action: &str,
        colour: Vec3,
        options: &crate::options::VcsOptions,
    ) {
        if !options.filters.allows_file(raw_filename) {
            return;
        }

        let mut filtered = gource_core::utf8::filter_utf8(raw_filename.as_bytes());
        if !filtered.starts_with('/') {
            filtered.insert(0, '/');
        }

        self.files.push(CommitFile {
            filename: filtered,
            action: FileAction::from_code(action),
            colour,
        });
    }

    /// Postprocess the commit (e.g. UTF-8 sanitize the username).
    pub fn postprocess(&mut self) {
        self.username = gource_core::utf8::filter_utf8(self.username.as_bytes());
    }

    /// Validate the commit against user filters and check that files is non-empty.
    pub fn is_valid(&self, options: &crate::options::VcsOptions) -> bool {
        if !options.filters.allows_user(&self.username) {
            return false;
        }
        !self.files.is_empty()
    }
}

/// `RCommit::fileColour`: colour from the file extension (text after the last
/// `.` of the last path component, if non-empty), white otherwise.
pub fn file_colour(filename: &str, hasher: &gource_core::StringHasher) -> Vec3 {
    let slash = filename.rfind('/');
    let dot = filename.rfind('.');
    match dot {
        Some(dot) if dot + 1 < filename.len() && slash.is_none_or(|slash| slash < dot) => {
            hasher.colour_hash(&filename[dot + 1..])
        }
        _ => Vec3::ONE,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gource_core::StringHasher;

    #[test]
    fn action_codes_round_trip() {
        for code in ["A", "M", "D", "R", "X"] {
            assert_eq!(FileAction::from_code(code).code(), code);
        }
    }

    #[test]
    fn colour_from_extension() {
        let h = StringHasher::default();
        assert_eq!(file_colour("/src/main.rs", &h), h.colour_hash("rs"));
        assert_eq!(file_colour("/src.d/Makefile", &h), Vec3::ONE);
        assert_eq!(file_colour("/src/file.", &h), Vec3::ONE);
    }
}

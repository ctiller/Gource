//! Building commits from parsed logs: filters, UTF-8 normalisation and
//! extension colours. The types themselves live in [`gource_model`].

pub use gource_model::commit::{Commit, CommitFile, FileAction, WHITE};

/// Log-parser helpers on [`Commit`] (`RCommit::addFile` and friends).
pub trait CommitExt {
    /// Add a file with derived colour from extension.
    ///
    /// Applies filters to the raw filename, derives colour with `options.hasher`,
    /// then normalises the filename (UTF-8 filtered with a leading '/').
    fn add_file(&mut self, raw_filename: &str, action: &str, options: &crate::options::VcsOptions);

    /// Add a file with explicit colour (e.g. from custom log format).
    fn add_file_with_colour(
        &mut self,
        raw_filename: &str,
        action: &str,
        colour: [f32; 3],
        options: &crate::options::VcsOptions,
    );

    /// Add a file with stats and derived colour.
    fn add_file_with_stats(
        &mut self,
        raw_filename: &str,
        action: &str,
        lines_added: Option<u32>,
        lines_removed: Option<u32>,
        is_binary: bool,
        options: &crate::options::VcsOptions,
    );

    /// Add a file with explicit colour and diff stats.
    #[allow(clippy::too_many_arguments)]
    fn add_file_with_colour_and_stats(
        &mut self,
        raw_filename: &str,
        action: &str,
        colour: [f32; 3],
        lines_added: Option<u32>,
        lines_removed: Option<u32>,
        is_binary: bool,
        options: &crate::options::VcsOptions,
    );

    /// Validate the commit against user filters and check that files is non-empty.
    fn is_valid(&self, options: &crate::options::VcsOptions) -> bool;
}

impl CommitExt for Commit {
    fn add_file(&mut self, raw_filename: &str, action: &str, options: &crate::options::VcsOptions) {
        let colour = file_colour(raw_filename, &options.hasher);
        self.add_file_with_colour(raw_filename, action, colour, options);
    }

    fn add_file_with_colour(
        &mut self,
        raw_filename: &str,
        action: &str,
        colour: [f32; 3],
        options: &crate::options::VcsOptions,
    ) {
        self.add_file_with_colour_and_stats(
            raw_filename,
            action,
            colour,
            None,
            None,
            false,
            options,
        );
    }

    fn add_file_with_stats(
        &mut self,
        raw_filename: &str,
        action: &str,
        lines_added: Option<u32>,
        lines_removed: Option<u32>,
        is_binary: bool,
        options: &crate::options::VcsOptions,
    ) {
        let colour = file_colour(raw_filename, &options.hasher);
        self.add_file_with_colour_and_stats(
            raw_filename,
            action,
            colour,
            lines_added,
            lines_removed,
            is_binary,
            options,
        );
    }

    fn add_file_with_colour_and_stats(
        &mut self,
        raw_filename: &str,
        action: &str,
        colour: [f32; 3],
        lines_added: Option<u32>,
        lines_removed: Option<u32>,
        is_binary: bool,
        options: &crate::options::VcsOptions,
    ) {
        if !options.filters.allows_file(raw_filename) {
            return;
        }

        let mut filtered = gource_core::utf8::filter_utf8(raw_filename.as_bytes());
        if !filtered.starts_with('/') {
            filtered.insert(0, '/');
        }

        let is_shadow = self.is_shadow;
        self.files.push(CommitFile {
            filename: filtered,
            action: FileAction::from_code(action),
            colour,
            lines_added,
            lines_removed,
            is_binary,
            is_shadow,
        });
    }

    fn is_valid(&self, options: &crate::options::VcsOptions) -> bool {
        if !options.filters.allows_user(&self.username) {
            return false;
        }
        !self.files.is_empty()
    }
}

/// `RCommit::fileColour`: colour from the file extension (text after the last
/// `.` of the last path component, if non-empty), white otherwise.
pub fn file_colour(filename: &str, hasher: &gource_core::StringHasher) -> [f32; 3] {
    let slash = filename.rfind('/');
    let dot = filename.rfind('.');
    match dot {
        Some(dot) if dot + 1 < filename.len() && slash.is_none_or(|slash| slash < dot) => {
            hasher.colour_rgb(&filename[dot + 1..])
        }
        _ => WHITE,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gource_core::StringHasher;

    #[test]
    fn colour_from_extension() {
        let h = StringHasher::default();
        assert_eq!(file_colour("/src/main.rs", &h), h.colour_rgb("rs"));
        assert_eq!(file_colour("/src.d/Makefile", &h), WHITE);
        assert_eq!(file_colour("/src/file.", &h), WHITE);
    }
}

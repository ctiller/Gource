//! `--help` / `-H` text (port of `GourceSettings::help`).

pub use crate::descriptor::{HELP_PATH_INFO, HELP_SHORT_SUFFIX};

/// The help text printed by `gource --help` (`extended == false`) or
/// `gource -H` (`extended == true`). Must match the C++ output byte for byte
/// (reference captures: `help.txt` / `help_extended.txt` from the C++ build),
/// with the version from [`crate::GOURCE_VERSION`].
pub fn help_text(extended: bool) -> String {
    crate::descriptor::generate_help_text(extended)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_help_text_matches_golden() {
        let expected_short = include_str!("../tests/data/help.txt");
        let expected_extended = include_str!("../tests/data/help_extended.txt");

        assert_eq!(help_text(false), expected_short);
        assert_eq!(help_text(true), expected_extended);
    }
}

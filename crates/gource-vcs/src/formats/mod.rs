//! Log format parsers (port of `src/formats/*.cpp`).

pub mod apache;
pub mod bzr;
pub mod custom;
pub mod cvs2cl;
pub mod cvs_exp;
pub mod git;
pub mod gitraw;
pub mod hg;
pub mod svn;

use crate::options::VcsOptions;

/// The command line used to generate a log for a VCS (`--log-command`).
/// Must match the C++ output exactly (each format's `logCommand()`).
pub fn log_command(vcs: &str) -> Option<String> {
    log_command_with_options(vcs, &VcsOptions::default())
}

/// The command line used to generate a log for a VCS with explicit options.
pub fn log_command_with_options(vcs: &str, options: &VcsOptions) -> Option<String> {
    match vcs {
        "git" => Some(git::log_command_with_options(options)),
        "svn" => Some(svn::log_command(options)),
        "hg" => Some(hg::log_command(options)),
        "bzr" => Some(bzr::log_command(options)),
        "cvs" => Some(
            "gource: please use either 'cvs2cl' or 'cvs-exp'\nTry 'gource --help' for more information.\n".to_string(),
        ),
        "cvs-exp" => Some(cvs_exp::log_command()),
        "cvs2cl" => Some(cvs2cl::log_command()),
        "gitraw" => Some(gitraw::log_command()),
        _ => None,
    }
}

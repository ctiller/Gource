//! Command line handling before the window opens (port of `main.cpp` up to
//! `display.init`).
//!
//! Pure: produces an [`Outcome`] describing what to print and the exit
//! status, so it can be tested without spawning processes.

use std::path::Path;

use gource_settings::{CliAction, Config, help::help_text, parse_command_line};
use gource_vcs::VcsError;

/// `gSDLAppExec`: the program name used in messages.
pub const EXEC_NAME: &str = "gource";

/// The text `SDLAppQuit(error)` prints to stderr before exiting with
/// status 1.
pub fn quit_message(error: &str) -> String {
    format!("{EXEC_NAME}: {error}\nTry '{EXEC_NAME} --help' for more information.\n\n")
}

/// What to do after parsing the command line.
#[derive(Debug)]
pub enum Outcome {
    /// Print `stdout` and `stderr`, then exit with `code`.
    Exit {
        stdout: String,
        stderr: String,
        code: u8,
    },
    /// Open the window and run the visualisation.
    Run(Box<Config>),
}

impl Outcome {
    fn success(stdout: impl Into<String>) -> Self {
        Outcome::Exit {
            stdout: stdout.into(),
            stderr: String::new(),
            code: 0,
        }
    }

    /// `SDLAppQuit(error)`.
    pub fn quit(error: &str) -> Self {
        Outcome::Exit {
            stdout: String::new(),
            stderr: quit_message(error),
            code: 1,
        }
    }
}

/// Handle the command line (`args` excludes the program name) the way
/// `main.cpp` does: help, `--log-command`, `--save-config` and
/// `--output-custom-log` finish here; everything else runs the app.
pub fn handle_command_line(args: &[String]) -> Outcome {
    let action = match parse_command_line(args) {
        Ok(action) => action,
        Err(error) => return Outcome::quit(&error.0),
    };

    match action {
        CliAction::Help { extended } => Outcome::success(help_text(extended)),
        // `SDLAppInfo(logCommand())`.
        CliAction::PrintLogCommand { vcs } => {
            let cmd = gource_vcs::log_command(&vcs).unwrap_or_default();
            Outcome::success(format!("{cmd}\n"))
        }
        CliAction::SaveConfig { path, config } => match config.conf.save(Path::new(&path)) {
            Ok(()) => Outcome::success(""),
            Err(error) => Outcome::quit(&error.0),
        },
        CliAction::OutputCustomLog { output, config } => {
            let options = gource_sim::vcs_options(&config.gource);
            match gource_vcs::write_custom_log(&config.gource.path, &output, &options) {
                Ok(()) => Outcome::success(""),
                // `Gource::writeCustomLog` calls SDLAppQuit only when the log
                // mill reported an error; a missing log or an unwritable
                // output file end the program quietly.
                Err(VcsError::Message(message)) if !message.is_empty() => Outcome::quit(&message),
                Err(_) => Outcome::success(""),
            }
        }
        CliAction::Run(config) => Outcome::Run(Box::new(config)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    fn exit(outcome: Outcome) -> (String, String, u8) {
        let Outcome::Exit {
            stdout,
            stderr,
            code,
        } = outcome
        else {
            unreachable!()
        };
        (stdout, stderr, code)
    }

    #[test]
    fn quit_message_matches_sdlappquit() {
        assert_eq!(
            quit_message("unknown option foo"),
            "gource: unknown option foo\nTry 'gource --help' for more information.\n\n"
        );
    }

    #[test]
    fn help_goes_to_stdout() {
        let (stdout, stderr, code) = exit(handle_command_line(&args(&["--help"])));
        assert_eq!(code, 0);
        assert!(stderr.is_empty());
        assert_eq!(stdout, help_text(false));
        let (stdout, _, _) = exit(handle_command_line(&args(&["-H"])));
        assert_eq!(stdout, help_text(true));
    }

    #[test]
    fn log_command_is_printed_with_newline() {
        let (stdout, _, code) = exit(handle_command_line(&args(&["--log-command", "svn"])));
        assert_eq!(code, 0);
        assert_eq!(stdout, "svn log -r 1:HEAD --xml --verbose --quiet\n");
    }

    #[test]
    fn errors_use_sdlappquit_format() {
        let (stdout, stderr, code) = exit(handle_command_line(&args(&["--log-command", "cvs"])));
        assert_eq!(code, 1);
        assert!(stdout.is_empty());
        assert_eq!(
            stderr,
            quit_message("please use either 'cvs2cl' or 'cvs-exp'")
        );
        let (_, stderr, code) = exit(handle_command_line(&args(&["--no-such-option"])));
        assert_eq!(code, 1);
        assert!(stderr.starts_with("gource: unknown option"));
    }

    #[test]
    fn save_config_writes_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("out.conf");
        let path_str = path.to_str().unwrap();
        let (stdout, stderr, code) = exit(handle_command_line(&args(&[
            "--save-config",
            path_str,
            "--seconds-per-day",
            "3",
        ])));
        assert_eq!((stdout.as_str(), stderr.as_str(), code), ("", "", 0));
        let written = std::fs::read_to_string(&path).unwrap();
        assert!(written.contains("seconds-per-day=3"), "{written}");
    }

    #[test]
    fn save_config_failure_is_fatal() {
        let (_, stderr, code) = exit(handle_command_line(&args(&[
            "--save-config",
            "/nonexistent-dir/x/y.conf",
        ])));
        assert_eq!(code, 1);
        assert!(stderr.starts_with("gource: "), "{stderr}");
    }

    #[test]
    fn run_returns_config() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().to_str().unwrap();
        match handle_command_line(&args(&["-1280x720", path])) {
            Outcome::Run(config) => {
                assert_eq!(config.gource.path, path);
                assert_eq!(config.display.display_width, 1280);
                assert_eq!(config.display.display_height, 720);
            }
            other => panic!("unexpected {other:?}"),
        }
        // Like the C++, a path that doesn't exist is rejected up front.
        let (_, stderr, code) = exit(handle_command_line(&args(&["/no/such/path"])));
        assert_eq!(code, 1);
        assert!(
            stderr.contains("does not appear to be a valid file or directory"),
            "{stderr}"
        );
    }
}

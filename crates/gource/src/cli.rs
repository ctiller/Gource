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

/// Export history statistics as a CSV file (`--output-stats FILE`).
pub fn write_stats_csv(
    settings: &gource_settings::GourceSettings,
    output: &str,
) -> Result<(), VcsError> {
    use std::io::Write;

    let mut options = gource_sim::vcs_options(settings);
    options.include_numstat = true;

    let mut commitlog =
        gource_vcs::LogMill::fetch_blocking(&settings.path, &options).map_err(VcsError::Message)?;
    commitlog.wait_for_input(true);

    let mut builder = gource_history::HistoryBuilder::new(
        gource_history::CohortMode::Year,
        gource_history::ChurnDecayModel::LifoYoungestFirst,
    );

    while !commitlog.is_finished() {
        let commit = match commitlog.next_commit() {
            Some(c) => c,
            None => {
                if !commitlog.is_seekable() {
                    break;
                }
                continue;
            }
        };

        let files = commit
            .files
            .into_iter()
            .map(|cf| {
                let op = match cf.action {
                    gource_vcs::FileAction::Add => gource_history::ChangeOp::Add,
                    gource_vcs::FileAction::Delete => gource_history::ChangeOp::Delete,
                    _ => gource_history::ChangeOp::Modify,
                };
                let lines_added =
                    cf.lines_added
                        .unwrap_or(if cf.action == gource_vcs::FileAction::Delete {
                            0
                        } else {
                            10
                        });
                let lines_removed = cf.lines_removed.unwrap_or(0);
                gource_history::FileChangeInput {
                    path: cf.filename,
                    op,
                    lines_added,
                    lines_removed,
                    byte_size: None,
                    is_binary: cf.is_binary,
                }
            })
            .collect();

        builder.add_commit(gource_history::CommitInput {
            timestamp: commit.timestamp,
            username: commit.username,
            files,
        });
    }

    let history = builder.finish();
    let csv = history.export_csv();

    if output == "-" {
        let mut stdout = std::io::stdout();
        stdout.write_all(csv.as_bytes())?;
        stdout.flush()?;
    } else {
        let mut file = std::fs::File::create(output)?;
        file.write_all(csv.as_bytes())?;
        file.flush()?;
    }

    Ok(())
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
        CliAction::Run(config) => {
            if !config.gource.output_stats_filename.is_empty() {
                match write_stats_csv(&config.gource, &config.gource.output_stats_filename) {
                    Ok(()) => Outcome::success(""),
                    Err(VcsError::Message(msg)) if !msg.is_empty() => Outcome::quit(&msg),
                    Err(_) => Outcome::success(""),
                }
            } else {
                Outcome::Run(Box::new(config))
            }
        }
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

    #[test]
    fn output_stats_csv_writes_expected_content() {
        let dir = tempfile::tempdir().unwrap();
        let log_file = dir.path().join("repo.log");
        let stats_file = dir.path().join("stats.csv");

        // Custom log format with added and removed columns
        let log_content = "\
1609459200|alice|A|src/main.rs|100|0
1609545600|bob|M|src/main.rs|20|5
1609632000|alice|D|src/main.rs|0|115
";
        std::fs::write(&log_file, log_content).unwrap();

        let (stdout, stderr, code) = exit(handle_command_line(&args(&[
            "--output-stats",
            stats_file.to_str().unwrap(),
            log_file.to_str().unwrap(),
        ])));

        assert_eq!((stdout.as_str(), stderr.as_str(), code), ("", "", 0));

        let csv = std::fs::read_to_string(&stats_file).unwrap();
        let mut lines = csv.lines();
        assert_eq!(
            lines.next(),
            Some(
                "commit,timestamp,total_files,total_lines,lines_added,lines_removed,active_editors_30d,dominant_cohort"
            )
        );

        let row1 = lines.next().expect("row 1");
        assert!(row1.starts_with("0,1609459200,"), "row1: {row1}");
        assert!(row1.ends_with(",1,2021"), "row1: {row1}");

        let row2 = lines.next().expect("row 2");
        assert!(row2.starts_with("1,1609545600,"), "row2: {row2}");

        let row3 = lines.next().expect("row 3");
        assert!(row3.starts_with("2,1609632000,"), "row3: {row3}");
        assert!(row3.ends_with(",none"), "row3: {row3}");
    }

    #[test]
    fn output_stats_csv_to_stdout() {
        let dir = tempfile::tempdir().unwrap();
        let log_file = dir.path().join("repo.log");
        let log_content = "1609459200|alice|A|src/lib.rs\n";
        std::fs::write(&log_file, log_content).unwrap();

        let (stdout, stderr, code) = exit(handle_command_line(&args(&[
            "--output-stats",
            "-",
            log_file.to_str().unwrap(),
        ])));

        assert_eq!(code, 0);
        assert!(stderr.is_empty());
        // write_stats_csv directly streams to stdout, so handle_command_line returns Outcome::success("")
        assert_eq!(stdout, "");
    }

    #[test]
    fn output_stats_error_handling_no_vcs_or_commits() {
        let dir = tempfile::tempdir().unwrap();
        let empty_dir = dir.path().join("empty_repo");
        std::fs::create_dir(&empty_dir).unwrap();
        let stats_file = dir.path().join("stats.csv");

        let (stdout, stderr, code) = exit(handle_command_line(&args(&[
            "--output-stats",
            stats_file.to_str().unwrap(),
            empty_dir.to_str().unwrap(),
        ])));

        assert_eq!(code, 1);
        assert!(stdout.is_empty());
        assert!(stderr.starts_with("gource: "), "{stderr}");
    }

    #[test]
    fn output_stats_error_handling_unwritable_file() {
        let dir = tempfile::tempdir().unwrap();
        let log_file = dir.path().join("repo.log");
        std::fs::write(&log_file, "1609459200|alice|A|src/lib.rs\n").unwrap();

        let (stdout, stderr, code) = exit(handle_command_line(&args(&[
            "--output-stats",
            "/nonexistent_dir/impossible/out.csv",
            log_file.to_str().unwrap(),
        ])));

        // Unwritable output file fails with Io error which returns Outcome::success("")
        assert_eq!((stdout.as_str(), stderr.as_str(), code), ("", "", 0));
    }
}

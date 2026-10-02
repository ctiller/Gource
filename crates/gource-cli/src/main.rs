//! `gource-cli`: Bevy-free headless Gource binary.

use std::{io::Write, process::ExitCode};

use gource_cli::{Outcome, handle_command_line, run_headless};

fn exit_with(stdout: &str, stderr: &str, code: u8) -> ExitCode {
    let _ = std::io::stdout().write_all(stdout.as_bytes());
    let _ = std::io::stdout().flush();
    let _ = std::io::stderr().write_all(stderr.as_bytes());
    ExitCode::from(code)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let outcome = match handle_command_line(&args) {
        Outcome::Exit {
            stdout,
            stderr,
            code,
        } => Outcome::Exit {
            stdout,
            stderr,
            code,
        },
        Outcome::Run(config) => run_headless(*config, None),
    };
    match outcome {
        Outcome::Exit {
            stdout,
            stderr,
            code,
        } => exit_with(&stdout, &stderr, code),
        Outcome::Run(_) => ExitCode::SUCCESS,
    }
}

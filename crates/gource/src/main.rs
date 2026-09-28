//! `gource`: software version control visualization.

use std::{io::Write, process::ExitCode};

use gource::{
    app::{self, AppConfig},
    cli::{self, Outcome},
};
use gource_draw::PpmExporter;
use gource_sim::{AppOptions, GourceApp};

fn exit_with(stdout: &str, stderr: &str, code: u8) -> ExitCode {
    // Write errors (e.g. a closed pipe) can't be reported anywhere useful.
    let _ = std::io::stdout().write_all(stdout.as_bytes());
    let _ = std::io::stdout().flush();
    let _ = std::io::stderr().write_all(stderr.as_bytes());
    ExitCode::from(code)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();

    let config = match cli::handle_command_line(&args) {
        Outcome::Exit {
            stdout,
            stderr,
            code,
        } => return exit_with(&stdout, &stderr, code),
        Outcome::Run(config) => *config,
    };

    let display = config.display.clone();
    let log_level = config.gource.log_level;

    let exporter = if display.output_ppm_filename.is_empty() {
        None
    } else {
        match PpmExporter::new(&display.output_ppm_filename) {
            Ok(exporter) => Some(exporter),
            Err(_) => {
                let message = format!("could not write to '{}'", display.output_ppm_filename);
                return exit_with("", &cli::quit_message(&message), 1);
            }
        }
    };

    let options = AppOptions {
        recording: exporter.is_some(),
    };
    let sim = match GourceApp::new(config, options) {
        Ok(sim) => sim,
        Err(error) => return exit_with("", &cli::quit_message(&error.0), 1),
    };

    let code = app::run(
        Box::new(sim),
        AppConfig {
            display,
            log_level,
            exporter,
        },
    );
    ExitCode::from(code)
}

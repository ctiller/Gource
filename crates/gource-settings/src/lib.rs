//! Gource settings (port of `core/settings.cpp`, `core/conffile.cpp` and
//! `gource_settings.cpp`).
//!
//! Settings are plain data. Instead of the C++ global `gGourceSettings`, the
//! frontend parses the command line once into a [`CliAction`] and hands the
//! resulting [`Config`] to the simulation, which owns it.
//!
//! Flow (mirrors `main.cpp`):
//! 1. [`parse_command_line`] converts arguments into a [`ConfFile`]
//!    (sections `[display]` and `[gource]`), handling `--help`,
//!    `--log-command`, a positional `*.conf/*.cfg/*.ini` config file,
//!    `--load-config`, and the positional path argument.
//! 2. [`DisplaySettings::import`] and [`GourceSettings::import`] validate the
//!    conf and produce typed settings (errors use the C++ messages).
//! 3. `--save-config` writes the conf and exits; `--output-custom-log` is
//!    reported so the frontend can run it.

pub mod conffile;
pub mod display;
pub mod gource;
pub mod help;
pub mod patch;

pub use conffile::{ConfEntry, ConfFile, ConfSection};
pub use display::DisplaySettings;
pub use gource::{
    CameraMode, DashboardPanel, DashboardPeriod, FileColourMode, FileSizeMetric, GourceSettings,
    LogLevel,
};
pub use patch::*;

use std::path::Path;

/// Version string reported by `--help` (`GOURCE_VERSION`).
pub const GOURCE_VERSION: &str = "0.57";

/// Errors from parsing or validating settings. The message text must match
/// the C++ `ConfFileException` messages (e.g. "invalid days-per-second value",
/// "unknown option foo"), including the "... in file:line" suffixes where the
/// C++ code produces them.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct SettingsError(pub String);

/// Fully parsed configuration for a run.
#[derive(Debug, Clone)]
pub struct Config {
    /// The merged configuration (command line applied over any loaded file).
    /// Kept because multiple `[gource]` sections mean multiple repositories,
    /// each imported in turn by the shell.
    pub conf: ConfFile,
    pub display: DisplaySettings,
    /// Settings imported from the first `[gource]` section.
    pub gource: GourceSettings,
}

/// What the program should do after parsing the command line.
#[derive(Debug, Clone)]
pub enum CliAction {
    /// Print [`help::help_text`] (optionally extended) and exit successfully.
    Help { extended: bool },
    /// Print the log command for a VCS (`--log-command git`,
    /// `--git-log-command`, ...) and exit. The frontend looks the command up
    /// in `gource_vcs::log_command`.
    PrintLogCommand { vcs: String },
    /// `--save-config FILE`: the conf has been validated; write it and exit.
    SaveConfig { path: String, config: Config },
    /// `--output-custom-log FILE`: convert the log at `config.gource.path`.
    OutputCustomLog { output: String, config: Config },
    /// Run the visualisation.
    Run(Config),
}

#[derive(Default, Debug)]
struct CommandLineState {
    help: Option<bool>,
    log_command: Option<String>,
    load_config: String,
    save_config: String,
    output_custom_log: String,
    log_level: Option<LogLevel>,
}

fn handle_command_line_option(
    name: &str,
    value: &str,
    state: &mut CommandLineState,
) -> Result<(), SettingsError> {
    match name {
        "help" => {
            state.help = Some(false);
            Ok(())
        }
        "extended-help" => {
            state.help = Some(true);
            Ok(())
        }
        "load-config" => {
            if !value.is_empty() {
                state.load_config = value.to_owned();
                Ok(())
            } else {
                Err(SettingsError("invalid load-config value".to_owned()))
            }
        }
        "save-config" => {
            if !value.is_empty() {
                state.save_config = value.to_owned();
                Ok(())
            } else {
                Err(SettingsError("invalid save-config value".to_owned()))
            }
        }
        "git-log-command" => {
            state.log_command = Some("git".to_owned());
            Ok(())
        }
        "cvs-exp-command" => {
            state.log_command = Some("cvs-exp".to_owned());
            Ok(())
        }
        "cvs2cl-command" => {
            state.log_command = Some("cvs2cl".to_owned());
            Ok(())
        }
        "svn-log-command" => {
            state.log_command = Some("svn".to_owned());
            Ok(())
        }
        "hg-log-command" => {
            state.log_command = Some("hg".to_owned());
            Ok(())
        }
        "bzr-log-command" => {
            state.log_command = Some("bzr".to_owned());
            Ok(())
        }
        "log-command" => {
            if value == "cvs" {
                return Err(SettingsError(
                    "please use either 'cvs2cl' or 'cvs-exp'".to_owned(),
                ));
            }
            match value {
                "git" | "cvs-exp" | "cvs2cl" | "svn" | "hg" | "bzr" => {
                    state.log_command = Some(value.to_owned());
                    Ok(())
                }
                _ => Err(SettingsError("invalid log-command value".to_owned())),
            }
        }
        "output-custom-log" => {
            if !value.is_empty() {
                state.output_custom_log = value.to_owned();
                Ok(())
            } else {
                Err(SettingsError("invalid output-custom-log value".to_owned()))
            }
        }
        "log-level" => {
            if let Some(level) = LogLevel::parse(value) {
                state.log_level = Some(level);
                Ok(())
            } else {
                Err(SettingsError("invalid log-level value".to_owned()))
            }
        }
        _ => Err(SettingsError(format!("invalid {name} value"))),
    }
}

fn get_option_type(opt: &str) -> Option<&'static str> {
    match opt {
        "viewport" => Some("string"),
        "windowed" => Some("bool"),
        "screen" => Some("int"),
        "window-position" => Some("string"),
        "fullscreen" => Some("bool"),
        "frameless" => Some("bool"),
        "transparent" => Some("bool"),
        "multi-sampling" => Some("bool"),
        "no-vsync" => Some("bool"),
        "high-dpi" => Some("bool"),
        "output-ppm-stream" => Some("string"),
        "output-framerate" => Some("int"),

        "help" => Some("bool"),
        "extended-help" => Some("bool"),
        "stop-on-idle" => Some("bool"),
        "stop-at-end" => Some("bool"),
        "dont-stop" => Some("bool"),
        "loop" => Some("bool"),
        "realtime" => Some("bool"),
        "no-time-travel" => Some("bool"),
        "colour-images" => Some("bool"),
        "hide-date" => Some("bool"),
        "hide-files" => Some("bool"),
        "hide-users" => Some("bool"),
        "hide-tree" => Some("bool"),
        "hide-usernames" => Some("bool"),
        "hide-filenames" => Some("bool"),
        "hide-dirnames" => Some("bool"),
        "hide-progress" => Some("bool"),
        "hide-bloom" => Some("bool"),
        "hide-mouse" => Some("bool"),
        "hide-root" => Some("bool"),
        "highlight-users" => Some("bool"),
        "highlight-dirs" => Some("bool"),
        "file-extensions" => Some("bool"),
        "file-extension-fallback" => Some("bool"),
        "fixed-user-size" => Some("bool"),
        "author-time" => Some("bool"),
        "key" => Some("bool"),
        "ffp" => Some("bool"),

        "disable-auto-rotate" => Some("bool"),
        "disable-auto-skip" => Some("bool"),
        "disable-input" => Some("bool"),

        "git-log-command" => Some("bool"),
        "cvs-exp-command" => Some("bool"),
        "cvs2cl-command" => Some("bool"),
        "svn-log-command" => Some("bool"),
        "hg-log-command" => Some("bool"),
        "bzr-log-command" => Some("bool"),

        "bloom-intensity" => Some("float"),
        "bloom-multiplier" => Some("float"),
        "elasticity" => Some("float"),
        "seconds-per-day" => Some("float"),
        "auto-skip-seconds" => Some("float"),
        "stop-at-time" => Some("float"),
        "max-user-speed" => Some("float"),
        "user-friction" => Some("float"),
        "padding" => Some("float"),
        "time-scale" => Some("float"),
        "dir-name-position" => Some("float"),
        "loop-delay-seconds" => Some("float"),

        "max-files" => Some("int"),
        "font-size" => Some("int"),
        "font-scale" => Some("float"),
        "file-font-size" => Some("int"),
        "dir-font-size" => Some("int"),
        "user-font-size" => Some("int"),
        "hash-seed" => Some("int"),

        "user-filter" => Some("multi-value"),
        "user-show-filter" => Some("multi-value"),
        "file-filter" => Some("multi-value"),
        "file-show-filter" => Some("multi-value"),
        "follow-user" => Some("multi-value"),
        "highlight-user" => Some("multi-value"),

        "log-level" => Some("string"),
        "background-image" => Some("string"),
        "logo" => Some("string"),
        "logo-offset" => Some("string"),
        "log-command" => Some("string"),
        "load-config" => Some("string"),
        "save-config" => Some("string"),
        "output-custom-log" => Some("string"),
        "path" => Some("string"),
        "background-colour" => Some("string"),
        "file-idle-time" => Some("string"),
        "file-idle-time-at-end" => Some("string"),
        "user-image-dir" => Some("string"),
        "default-user-image" => Some("string"),
        "date-format" => Some("string"),
        "log-format" => Some("string"),
        "git-branch" => Some("string"),
        "start-position" => Some("string"),
        "start-date" => Some("string"),
        "stop-date" => Some("string"),
        "stop-position" => Some("string"),
        "crop" => Some("string"),
        "hide" => Some("string"),
        "max-file-lag" => Some("string"),
        "user-scale" => Some("string"),
        "camera-mode" => Some("string"),
        "title" => Some("string"),
        "font-file" => Some("string"),
        "font-colour" => Some("string"),
        "highlight-colour" => Some("string"),
        "selection-colour" => Some("string"),
        "dir-colour" => Some("string"),

        "caption-file" => Some("string"),
        "caption-size" => Some("int"),
        "caption-duration" => Some("float"),
        "caption-colour" => Some("string"),
        "caption-offset" => Some("int"),

        "filename-colour" => Some("string"),
        "filename-time" => Some("float"),

        "dir-name-depth" => Some("int"),

        // Evolution settings
        "file-size-metric" => Some("string"),
        "file-pulse" => Some("float"),
        "file-colour-mode" => Some("string"),
        "dashboard" => Some("string"),
        "dashboard-period" => Some("string"),
        "dashboard-window" => Some("string"),
        "hide-dashboards" => Some("bool"),
        "output-stats" => Some("string"),
        "cache-dir" => Some("string"),
        "no-cache" => Some("bool"),
        "seed" => Some("int"),

        _ => None,
    }
}

fn get_option_alias(opt: &str) -> &str {
    match opt {
        "p" => "start-position",
        "a" => "auto-skip-seconds",
        "s" => "seconds-per-day",
        "t" => "stop-at-time",
        "i" => "file-idle-time",
        "e" => "elasticity",
        "h" | "?" => "help",
        "H" => "extended-help",
        "b" => "background-colour",
        "c" => "time-scale",
        "background" => "background-colour",
        "disable-bloom" => "hide-bloom",
        "disable-progress" => "hide-progress",
        "highlight-all-users" => "highlight-users",

        "f" => "fullscreen",
        "w" => "windowed",
        "o" => "output-ppm-stream",
        "r" => "output-framerate",

        _ => opt,
    }
}

fn get_option_section(opt: &str) -> &str {
    match opt {
        "viewport" | "windowed" | "fullscreen" | "frameless" | "screen" | "window-position"
        | "multi-sampling" | "output-ppm-stream" | "output-framerate" | "transparent"
        | "no-vsync" | "high-dpi" => "display",

        "help" | "extended-help" | "log-command" | "git-log-command" | "cvs-exp-command"
        | "cvs2cl-command" | "hg-log-command" | "bzr-log-command" | "svn-log-command"
        | "load-config" | "save-config" | "output-custom-log" | "log-level" => "command-line",

        _ => "gource",
    }
}

fn parse_args(
    arguments: &[String],
    conffile: &mut ConfFile,
    files: &mut Option<&mut Vec<String>>,
    cmd_state: &mut CommandLineState,
) -> Result<(), SettingsError> {
    let mut i = 0;
    while i < arguments.len() {
        let mut arg = arguments[i].clone();

        // remove leading hyphens
        let mut is_option = false;
        while arg.len() > 1 && arg.starts_with('-') {
            arg = arg[1..].to_string();
            is_option = true;
        }

        if arg.is_empty() {
            i += 1;
            continue;
        }

        if !is_option {
            if let Some(f) = files {
                f.push(arg);
            }
            i += 1;
            continue;
        }

        // translate args with aliases
        let arg = get_option_alias(&arg);

        // NUMBERxNUMBER is a magic alias for viewport
        if arg.len() > 1
            && arg.rfind('x').is_some()
            && let Some((width, height, _)) = display::parse_viewport(arg)
            && width > 0
            && height > 0
        {
            if conffile.section("display").is_none() {
                conffile.add_section("display");
            }
            conffile.set_entry("display", "viewport", arg);
            i += 1;
            continue;
        }

        let arg_type = match get_option_type(arg) {
            Some(t) => t,
            None => {
                return Err(SettingsError(format!("unknown option {arg}")));
            }
        };

        let arg_value = if arg_type == "bool" {
            "true".to_owned()
        } else if (i + 1) < arguments.len() {
            i += 1;
            arguments[i].clone()
        } else {
            String::new()
        };

        let section_name = get_option_section(arg);

        if section_name == "command-line" {
            handle_command_line_option(arg, &arg_value, cmd_state)?;
            i += 1;
            continue;
        }

        if conffile.section(section_name).is_none() {
            conffile.add_section(section_name);
        }

        for sec in conffile
            .sections
            .iter_mut()
            .filter(|s| s.name == section_name)
        {
            if arg_type == "multi-value" {
                sec.add_entry(arg, &arg_value);
            } else {
                sec.set_entry(arg, &arg_value);
            }
        }

        i += 1;
    }
    Ok(())
}

/// Parse the command line (excluding the program name) the way `main.cpp`
/// does. Errors carry the C++ messages; the frontend prints them as
/// `"Error: ..."`/help as appropriate.
pub fn parse_command_line(args: &[String]) -> Result<CliAction, SettingsError> {
    let mut conf = ConfFile::new();
    let mut files: Vec<String> = Vec::new();
    let mut cmd_state = CommandLineState::default();

    parse_args(args, &mut conf, &mut Some(&mut files), &mut cmd_state)?;

    if let Some(extended) = cmd_state.help {
        return Ok(CliAction::Help { extended });
    }

    if let Some(vcs) = cmd_state.log_command {
        return Ok(CliAction::PrintLogCommand { vcs });
    }

    if cmd_state.load_config.is_empty() && !files.is_empty() {
        let mut found_conf_idx: Option<usize> = None;
        for (idx, file) in files.iter().enumerate() {
            let file_length = file.len();
            let is_conf_ext = (file.ends_with(".conf") && file_length > 5)
                || (file.ends_with(".cfg") && file_length > 4)
                || (file.ends_with(".ini") && file_length > 4);

            if is_conf_ext && ConfFile::load(Path::new(file)).is_ok() {
                cmd_state.load_config = file.clone();
                found_conf_idx = Some(idx);
                break;
            }
        }
        if let Some(idx) = found_conf_idx {
            files.remove(idx);
        }
    }

    // load config if specified
    if !cmd_state.load_config.is_empty() {
        conf = ConfFile::load(Path::new(&cmd_state.load_config))?;
        let mut dummy_files = None;
        parse_args(args, &mut conf, &mut dummy_files, &mut cmd_state)?;
    }

    // set path
    if !files.is_empty() {
        let path = files[files.len() - 1].clone();
        if conf.count_sections("gource") > 0 {
            for sec in conf.sections.iter_mut().filter(|s| s.name == "gource") {
                sec.set_entry("path", &path);
            }
        } else {
            conf.set_entry("gource", "path", &path);
        }
    }

    // apply the config / see if it's valid
    let display = DisplaySettings::import(&conf)?;
    if conf.section("gource").is_none() {
        conf.add_section("gource");
    }
    let mut gource = GourceSettings::import(&conf, None)?;

    if let Some(level) = cmd_state.log_level {
        gource.log_level = level;
    }

    let config = Config {
        conf,
        display,
        gource,
    };

    if !cmd_state.save_config.is_empty() {
        return Ok(CliAction::SaveConfig {
            path: cmd_state.save_config,
            config,
        });
    }

    if !cmd_state.output_custom_log.is_empty() {
        return Ok(CliAction::OutputCustomLog {
            output: cmd_state.output_custom_log,
            config,
        });
    }

    Ok(CliAction::Run(config))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_help_actions() {
        let res = parse_command_line(&["--help".to_string()]).unwrap();
        assert!(matches!(res, CliAction::Help { extended: false }));

        let res = parse_command_line(&["-H".to_string()]).unwrap();
        assert!(matches!(res, CliAction::Help { extended: true }));
    }

    #[test]
    fn test_log_commands() {
        let res = parse_command_line(&["--git-log-command".to_string()]).unwrap();
        match res {
            CliAction::PrintLogCommand { vcs } => assert_eq!(vcs, "git"),
            _ => panic!("expected PrintLogCommand"),
        }

        let res = parse_command_line(&["--log-command".to_string(), "svn".to_string()]).unwrap();
        match res {
            CliAction::PrintLogCommand { vcs } => assert_eq!(vcs, "svn"),
            _ => panic!("expected PrintLogCommand"),
        }

        let err =
            parse_command_line(&["--log-command".to_string(), "cvs".to_string()]).unwrap_err();
        assert_eq!(err.0, "please use either 'cvs2cl' or 'cvs-exp'");
    }

    #[test]
    fn test_unknown_option() {
        let err = parse_command_line(&["--bad-option".to_string()]).unwrap_err();
        assert_eq!(err.0, "unknown option bad-option");
    }

    #[test]
    fn test_run_action() {
        let res = parse_command_line(&[".".to_string()]).unwrap();
        match res {
            CliAction::Run(cfg) => {
                assert_eq!(cfg.gource.path, ".");
                assert_eq!(cfg.display.display_width, 1024);
            }
            _ => panic!("expected Run"),
        }
    }
}

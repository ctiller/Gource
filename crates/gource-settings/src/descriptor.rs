//! Setting descriptor table defining metadata, help text, and man documentation for all options.
//! Generated from the single source of truth.

use crate::patch::SettingId;

/// Section of configuration file or invocation target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Section {
    Gource,
    Display,
    CommandLine,
}

impl Section {
    pub fn as_str(&self) -> &'static str {
        match self {
            Section::Gource => "gource",
            Section::Display => "display",
            Section::CommandLine => "command-line",
        }
    }
}

/// Setting value kind / CLI argument type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    Flag,
    Int,
    Float,
    String,
    Colour,
    MultiValue,
    Enum(&'static [&'static str]),
    Custom,
}

impl Kind {
    /// String representation compatible with `get_option_type`.
    pub fn type_name(&self) -> &'static str {
        match self {
            Kind::Flag => "bool",
            Kind::Int => "int",
            Kind::Float => "float",
            Kind::String | Kind::Colour | Kind::Enum(_) | Kind::Custom => "string",
            Kind::MultiValue => "multi-value",
        }
    }
}

/// Complete descriptor for a setting.
#[derive(Debug, Clone, Copy)]
pub struct SettingDesc {
    /// Long option name, e.g. "seconds-per-day".
    pub name: &'static str,
    /// Short option flag, e.g. Some("s").
    pub short: Option<&'static str>,
    /// Conf section (gource, display, or command-line).
    pub section: Section,
    /// Kind of setting (Flag, Int, Float, String, Colour, MultiValue, Enum, Custom).
    pub kind: Kind,
    /// Metavar in help output if any.
    pub metavar: Option<&'static str>,
    /// Default value as string (empty if none/flag).
    pub default: &'static str,
    /// Numeric range (min, max) if applicable.
    pub range: Option<(f64, f64)>,
    /// Logical grouping for help output.
    pub group: &'static str,
    /// The formatted --help lines.
    pub help: &'static str,
    /// Long man-page paragraph(s).
    pub man: &'static str,
    /// True if only displayed in extended help (-H).
    pub extended: bool,
    /// Live-patchable setting identifier, if supported.
    pub live: Option<SettingId>,
}

/// The master table of all 112 options displayed in help / man page, ordered as in help output.
pub static SETTINGS: &[SettingDesc] = &[
    SettingDesc {
        name: "help",
        short: Some("h"),
        section: Section::CommandLine,
        kind: Kind::Flag,
        metavar: None,
        default: "",
        range: None,
        group: "general",
        help: r#"  -h, --help                       Help"#,
        man: r#"Help ('\fB-H\fR' for extended help)."#,
        extended: false,
        live: None,
    },
    SettingDesc {
        name: "viewport",
        short: Some("WIDTHxHEIGHT"),
        section: Section::Display,
        kind: Kind::String,
        metavar: None,
        default: "1024x768",
        range: None,
        group: "display",
        help: r#"  -WIDTHxHEIGHT, --viewport        Set viewport size"#,
        man: r#"Set the viewport size. If \-f is also supplied, will attempt to set the video mode to this also. Add ! to make the window non-resizable."#,
        extended: false,
        live: None,
    },
    SettingDesc {
        name: "fullscreen",
        short: Some("f"),
        section: Section::Display,
        kind: Kind::Flag,
        metavar: None,
        default: "",
        range: None,
        group: "display",
        help: r#"  -f, --fullscreen                 Fullscreen"#,
        man: r#"Fullscreen."#,
        extended: false,
        live: None,
    },
    SettingDesc {
        name: "screen",
        short: None,
        section: Section::Display,
        kind: Kind::Int,
        metavar: Some("SCREEN"),
        default: "-1",
        range: None,
        group: "display",
        help: r#"      --screen SCREEN              Screen number"#,
        man: r#"Set the number of the screen to display on."#,
        extended: false,
        live: None,
    },
    SettingDesc {
        name: "multi-sampling",
        short: None,
        section: Section::Display,
        kind: Kind::Flag,
        metavar: None,
        default: "",
        range: None,
        group: "display",
        help: r#"      --multi-sampling             Enable multi-sampling"#,
        man: r#"Enable multi-sampling."#,
        extended: false,
        live: None,
    },
    SettingDesc {
        name: "high-dpi",
        short: None,
        section: Section::Display,
        kind: Kind::Flag,
        metavar: None,
        default: "",
        range: None,
        group: "display",
        help: r#"      --high-dpi                   Request a high DPI display"#,
        man: r#"Request a high DPI display when creating the window.

On some platforms such as MacOS, the window resolution is specified in points instead of pixels.
The \-\-high-dpi flag may be required to access some higher resolutions.

E.g. requesting a high DPI 800x600 window may produce a window that is 1600x1200 pixels."#,
        extended: false,
        live: None,
    },
    SettingDesc {
        name: "no-vsync",
        short: None,
        section: Section::Display,
        kind: Kind::Flag,
        metavar: None,
        default: "",
        range: None,
        group: "display",
        help: r#"      --no-vsync                   Disable vsync"#,
        man: r#"Disable vsync."#,
        extended: false,
        live: None,
    },
    SettingDesc {
        name: "start-date",
        short: None,
        section: Section::Gource,
        kind: Kind::String,
        metavar: Some("'YYYY-MM-DD hh:mm:ss +tz'"),
        default: "",
        range: None,
        group: "dates",
        help: r#"  --start-date 'YYYY-MM-DD hh:mm:ss +tz'  Start at a date and optional time"#,
        man: r#"Start with the first entry after the supplied date and optional time.

If a time zone offset isn't specified the local time zone is used.

Example accepted formats:

    "2012-06-30"
    "2012-06-30 12:00"
    "2012-06-30 12:00:00 +12""#,
        extended: false,
        live: None,
    },
    SettingDesc {
        name: "stop-date",
        short: None,
        section: Section::Gource,
        kind: Kind::String,
        metavar: Some("'YYYY-MM-DD hh:mm:ss +tz'"),
        default: "",
        range: None,
        group: "dates",
        help: r#"  --stop-date  'YYYY-MM-DD hh:mm:ss +tz'  Stop at a date and optional time"#,
        man: r#"Stop at the last entry prior to the supplied date and optional time.

Uses the same format as \-\-start\-date."#,
        extended: false,
        live: None,
    },
    SettingDesc {
        name: "start-position",
        short: Some("p"),
        section: Section::Gource,
        kind: Kind::String,
        metavar: Some("POSITION"),
        default: "",
        range: Some((0.00, 1.00)),
        group: "playback-control",
        help: r#"  -p, --start-position POSITION    Start at some position (0.0-1.0 or 'random')"#,
        man: r#"Begin at some position in the log (between 0.0 and 1.0 or 'random')."#,
        extended: false,
        live: None,
    },
    SettingDesc {
        name: "stop-position",
        short: None,
        section: Section::Gource,
        kind: Kind::String,
        metavar: Some("POSITION"),
        default: "",
        range: None,
        group: "playback-control",
        help: r#"      --stop-position  POSITION    Stop at some position"#,
        man: r#"Stop (exit) at some position in the log (does not work with STDIN)."#,
        extended: false,
        live: None,
    },
    SettingDesc {
        name: "stop-at-time",
        short: Some("t"),
        section: Section::Gource,
        kind: Kind::Float,
        metavar: Some("SECONDS"),
        default: "",
        range: None,
        group: "playback-control",
        help: r#"  -t, --stop-at-time SECONDS       Stop after a specified number of seconds"#,
        man: r#"Stop (exit) after a specified number of seconds."#,
        extended: false,
        live: None,
    },
    SettingDesc {
        name: "stop-at-end",
        short: None,
        section: Section::Gource,
        kind: Kind::Flag,
        metavar: None,
        default: "",
        range: None,
        group: "playback-control",
        help: r#"      --stop-at-end                Stop at end of the log"#,
        man: r#"Stop (exit) at the end of the log / stream."#,
        extended: false,
        live: None,
    },
    SettingDesc {
        name: "dont-stop",
        short: None,
        section: Section::Gource,
        kind: Kind::Flag,
        metavar: None,
        default: "",
        range: None,
        group: "playback-control",
        help: r#"      --dont-stop                  Keep running after the end of the log"#,
        man: r#"Keep running after the end of the log."#,
        extended: false,
        live: None,
    },
    SettingDesc {
        name: "loop",
        short: None,
        section: Section::Gource,
        kind: Kind::Flag,
        metavar: None,
        default: "",
        range: None,
        group: "playback-control",
        help: r#"      --loop                       Loop at the end of the log"#,
        man: r#"Loop back to the start of the log when the end is reached."#,
        extended: false,
        live: Some(SettingId::Loop),
    },
    SettingDesc {
        name: "auto-skip-seconds",
        short: Some("a"),
        section: Section::Gource,
        kind: Kind::Float,
        metavar: Some("SECONDS"),
        default: "3",
        range: Some((0.00, 60.00)),
        group: "simulation-speed",
        help: r#"  -a, --auto-skip-seconds SECONDS  Auto skip to next entry if nothing happens
                                   for a number of seconds (default: 3)"#,
        man: r#"Automatically skip to next entry if nothing happens for a specified number of seconds."#,
        extended: false,
        live: Some(SettingId::AutoSkipSeconds),
    },
    SettingDesc {
        name: "disable-auto-skip",
        short: None,
        section: Section::Gource,
        kind: Kind::Flag,
        metavar: None,
        default: "",
        range: None,
        group: "simulation-speed",
        help: r#"      --disable-auto-skip          Disable auto skip"#,
        man: r#"Disable auto skip."#,
        extended: false,
        live: None,
    },
    SettingDesc {
        name: "seconds-per-day",
        short: Some("s"),
        section: Section::Gource,
        kind: Kind::Float,
        metavar: Some("SECONDS"),
        default: "10",
        range: Some((0.01, 60.00)),
        group: "simulation-speed",
        help: r#"  -s, --seconds-per-day SECONDS    Speed in seconds per day (default: 10)"#,
        man: r#"Speed of simulation in seconds per day."#,
        extended: false,
        live: Some(SettingId::SecondsPerDay),
    },
    SettingDesc {
        name: "realtime",
        short: None,
        section: Section::Gource,
        kind: Kind::Flag,
        metavar: None,
        default: "",
        range: None,
        group: "simulation-speed",
        help: r#"      --realtime                   Realtime playback speed"#,
        man: r#"Realtime playback speed."#,
        extended: false,
        live: None,
    },
    SettingDesc {
        name: "no-time-travel",
        short: None,
        section: Section::Gource,
        kind: Kind::Flag,
        metavar: None,
        default: "",
        range: None,
        group: "simulation-speed",
        help: r#"      --no-time-travel             Use the time of the last commit if the
                                   time of a commit is in the past"#,
        man: r#"Use the time of the last commit if the time of a commit is in the past."#,
        extended: false,
        live: None,
    },
    SettingDesc {
        name: "author-time",
        short: None,
        section: Section::Gource,
        kind: Kind::Flag,
        metavar: None,
        default: "",
        range: None,
        group: "simulation-speed",
        help: r#"      --author-time                Use the timestamp of the author instead of
                                   the timestamp of the committer"#,
        man: r#"Use the timestamp of the author instead of the timestamp of the committer."#,
        extended: false,
        live: None,
    },
    SettingDesc {
        name: "time-scale",
        short: Some("c"),
        section: Section::Gource,
        kind: Kind::Float,
        metavar: Some("SCALE"),
        default: "1.0",
        range: Some((0.10, 4.00)),
        group: "simulation-speed",
        help: r#"  -c, --time-scale SCALE           Change simulation time scale (default: 1.0)"#,
        man: r#"Change simulation time scale. This affects the movement speed of user avatars.

E.g. 0.5 for half speed, 2 for double speed."#,
        extended: false,
        live: Some(SettingId::TimeScale),
    },
    SettingDesc {
        name: "elasticity",
        short: Some("e"),
        section: Section::Gource,
        kind: Kind::Float,
        metavar: Some("FLOAT"),
        default: "0.0",
        range: Some((0.00, 1.00)),
        group: "simulation-speed",
        help: r#"  -e, --elasticity FLOAT           Elasticity of nodes (default: 0.0)"#,
        man: r#"Elasticity of nodes."#,
        extended: false,
        live: Some(SettingId::Elasticity),
    },
    SettingDesc {
        name: "key",
        short: None,
        section: Section::Gource,
        kind: Kind::Flag,
        metavar: None,
        default: "",
        range: None,
        group: "keys",
        help: r#"  --key                            Show file extension key"#,
        man: r#"Show file extension key."#,
        extended: false,
        live: None,
    },
    SettingDesc {
        name: "user-image-dir",
        short: None,
        section: Section::Gource,
        kind: Kind::String,
        metavar: Some("DIRECTORY"),
        default: "",
        range: None,
        group: "user-images",
        help: r#"  --user-image-dir DIRECTORY       Dir containing images to use as avatars"#,
        man: r#"Directory containing .jpg or .png images of users (eg "Full Name.png") to use as avatars."#,
        extended: false,
        live: None,
    },
    SettingDesc {
        name: "default-user-image",
        short: None,
        section: Section::Gource,
        kind: Kind::String,
        metavar: Some("IMAGE"),
        default: "",
        range: None,
        group: "user-images",
        help: r#"  --default-user-image IMAGE       Default user image file"#,
        man: r#"Path of .jpg to use as the default user image."#,
        extended: false,
        live: None,
    },
    SettingDesc {
        name: "fixed-user-size",
        short: None,
        section: Section::Gource,
        kind: Kind::Flag,
        metavar: None,
        default: "",
        range: None,
        group: "user-images",
        help: r#"  --fixed-user-size                Use a fixed size throughout"#,
        man: r#"Forces the size of the user image to remain fixed throughout."#,
        extended: false,
        live: None,
    },
    SettingDesc {
        name: "colour-images",
        short: None,
        section: Section::Gource,
        kind: Kind::Flag,
        metavar: None,
        default: "",
        range: None,
        group: "user-images",
        help: r#"  --colour-images                  Colourize user images"#,
        man: r#"Colourize user images."#,
        extended: false,
        live: None,
    },
    SettingDesc {
        name: "file-idle-time",
        short: Some("i"),
        section: Section::Gource,
        kind: Kind::String,
        metavar: Some("SECONDS"),
        default: "0",
        range: Some((0.00, 60.00)),
        group: "file-idle",
        help: r#"  -i, --file-idle-time SECONDS     Time files remain idle (default: 0)"#,
        man: r#"Time in seconds files remain idle before they are removed or 0 for no limit."#,
        extended: false,
        live: Some(SettingId::FileIdleTime),
    },
    SettingDesc {
        name: "file-idle-time-at-end",
        short: None,
        section: Section::Gource,
        kind: Kind::String,
        metavar: Some("SECONDS"),
        default: "0",
        range: None,
        group: "file-idle",
        help: r#"  --file-idle-time-at-end SECONDS  Time files remain idle at end (default: 0)"#,
        man: r#"Time in seconds files remain idle at the end before they are removed."#,
        extended: false,
        live: None,
    },
    SettingDesc {
        name: "max-files",
        short: None,
        section: Section::Gource,
        kind: Kind::Int,
        metavar: Some("NUMBER"),
        default: "0",
        range: Some((0.00, 100000.00)),
        group: "file-limits",
        help: r#"  --max-files NUMBER      Max number of files or 0 for no limit"#,
        man: r#"Set the maximum number of files or 0 for no limit. Excess files will be discarded."#,
        extended: false,
        live: Some(SettingId::MaxFiles),
    },
    SettingDesc {
        name: "max-file-lag",
        short: None,
        section: Section::Gource,
        kind: Kind::String,
        metavar: Some("SECONDS"),
        default: "",
        range: Some((0.10, 60.00)),
        group: "file-limits",
        help: r#"  --max-file-lag SECONDS  Max time files of a commit can take to appear"#,
        man: r#"Max time files of a commit can take to appear. Use \-1 for no limit."#,
        extended: false,
        live: Some(SettingId::MaxFileLag),
    },
    SettingDesc {
        name: "log-command",
        short: None,
        section: Section::CommandLine,
        kind: Kind::String,
        metavar: Some("VCS"),
        default: "",
        range: None,
        group: "vcs-logging",
        help: r#"  --log-command VCS       Show the VCS log command (git,svn,hg,bzr,cvs2cl)"#,
        man: r#"Show the log command used by gource (git,svn,hg,bzr,cvs2cl)."#,
        extended: false,
        live: None,
    },
    SettingDesc {
        name: "log-format",
        short: None,
        section: Section::Gource,
        kind: Kind::String,
        metavar: Some("VCS"),
        default: "",
        range: None,
        group: "vcs-logging",
        help: r#"  --log-format  VCS       Specify the log format (git,svn,hg,bzr,cvs2cl,custom)"#,
        man: r#"Specify format of the log being read (git,svn,hg,bzr,cvs2cl,custom). Required when reading from STDIN."#,
        extended: false,
        live: None,
    },
    SettingDesc {
        name: "load-config",
        short: None,
        section: Section::CommandLine,
        kind: Kind::String,
        metavar: Some("CONF_FILE"),
        default: "",
        range: None,
        group: "config-file",
        help: r#"  --load-config CONF_FILE  Load a config file"#,
        man: r#"Load a config file."#,
        extended: false,
        live: None,
    },
    SettingDesc {
        name: "save-config",
        short: None,
        section: Section::CommandLine,
        kind: Kind::String,
        metavar: Some("CONF_FILE"),
        default: "",
        range: None,
        group: "config-file",
        help: r#"  --save-config CONF_FILE  Save a config file with the current options"#,
        man: r#"Save a config file with the current options."#,
        extended: false,
        live: None,
    },
    SettingDesc {
        name: "output-ppm-stream",
        short: Some("o"),
        section: Section::Display,
        kind: Kind::String,
        metavar: Some("FILE"),
        default: "",
        range: None,
        group: "video-output",
        help: r#"  -o, --output-ppm-stream FILE    Output PPM stream to a file ('-' for STDOUT)"#,
        man: r#"Output a PPM image stream to a file ('\-' for STDOUT).

This will automatically hide the progress bar initially and enable 'stop\-at\-end' unless other behaviour is specified."#,
        extended: false,
        live: None,
    },
    SettingDesc {
        name: "output-framerate",
        short: Some("r"),
        section: Section::Display,
        kind: Kind::Int,
        metavar: Some("FPS"),
        default: "60",
        range: None,
        group: "video-output",
        help: r#"  -r, --output-framerate  FPS     Framerate of output (25,30,60)"#,
        man: r#"Framerate of output (25,30,60). Used with \-\-output\-ppm\-stream."#,
        extended: false,
        live: None,
    },
    SettingDesc {
        name: "live",
        short: None,
        section: Section::Gource,
        kind: Kind::Flag,
        metavar: None,
        default: "",
        range: None,
        group: "live-mode",
        help: r#"  --live                          Enable live stream/watch mode"#,
        man: r#"Enable live stream/watch mode. Gource will periodically poll the repository or data source for new commits and stream them in real time."#,
        extended: false,
        live: None,
    },
    SettingDesc {
        name: "live-interval",
        short: None,
        section: Section::Gource,
        kind: Kind::Float,
        metavar: Some("SECONDS"),
        default: "5.0",
        range: None,
        group: "live-mode",
        help: r#"  --live-interval SECONDS         Polling interval for live updates (default: 5.0)"#,
        man: r#"Polling interval in seconds for live updates (default: 5.0)."#,
        extended: false,
        live: None,
    },
    SettingDesc {
        name: "live-fetch",
        short: None,
        section: Section::Gource,
        kind: Kind::Flag,
        metavar: None,
        default: "",
        range: None,
        group: "live-mode",
        help: r#"  --live-fetch                    Fetch remote repository changes in live mode"#,
        man: r#"Fetch remote repository changes automatically when polling in live mode."#,
        extended: false,
        live: None,
    },
    SettingDesc {
        name: "github",
        short: None,
        section: Section::Gource,
        kind: Kind::String,
        metavar: Some("TARGET"),
        default: "",
        range: None,
        group: "live-mode",
        help: r#"  --github TARGET                 Watch a GitHub repository (owner/repo or URL)"#,
        man: r#"Watch a GitHub repository directly by name ("owner/repo") or full URL using GitHub REST/Events API."#,
        extended: false,
        live: None,
    },
    SettingDesc {
        name: "github-token",
        short: None,
        section: Section::Gource,
        kind: Kind::String,
        metavar: Some("TOKEN"),
        default: "",
        range: None,
        group: "live-mode",
        help: r#"  --github-token TOKEN            GitHub personal access token for API requests"#,
        man: r#"GitHub personal access token for authenticated API requests and higher rate limits."#,
        extended: false,
        live: None,
    },
    SettingDesc {
        name: "watch-paths",
        short: None,
        section: Section::Gource,
        kind: Kind::MultiValue,
        metavar: Some("PATH"),
        default: "",
        range: None,
        group: "live-mode",
        help: r#"  --watch-paths PATH              Watch multiple repositories (comma/colon separated or repeated)"#,
        man: r#"Watch multiple repositories simultaneously. Paths can be comma-separated, colon-separated, or specified by passing multiple \-\-watch\-paths options. In live mode, logs from all repositories are merged chronologically into a single unified tree visualization."#,
        extended: false,
        live: None,
    },
    SettingDesc {
        name: "watch-worktrees",
        short: None,
        section: Section::Gource,
        kind: Kind::Flag,
        metavar: None,
        default: "",
        range: None,
        group: "live-mode",
        help: r#"  --watch-worktrees               Watch git worktrees for uncommitted in-flight changes"#,
        man: r#"Watch git worktrees for uncommitted in-flight changes. Working directory modifications across all linked git worktrees appear as a semi-transparent "shadow world" attributed to their respective authors or worktrees. When changes are committed, the shadow files solidify into permanent repository commits."#,
        extended: false,
        live: None,
    },
    SettingDesc {
        name: "git-backend",
        short: None,
        section: Section::Gource,
        kind: Kind::String,
        metavar: Some("BACKEND"),
        default: "auto",
        range: None,
        group: "live-mode",
        help: r#"  --git-backend BACKEND           Git backend (auto, cli, in-process; default: auto)"#,
        man: r#"Git backend implementation to use: 'auto' (automatic detection), 'cli' (spawn git subprocess), or 'in-process' (native in-process git library). Default is 'auto'."#,
        extended: false,
        live: None,
    },
    SettingDesc {
        name: "window-position",
        short: None,
        section: Section::Display,
        kind: Kind::String,
        metavar: Some("XxY"),
        default: "-1x-1",
        range: None,
        group: "window-geometry",
        help: r#"  --window-position XxY    Initial window position"#,
        man: r#"Initial window position on your desktop which may be made up of multiple monitors.

This will override the screen setting so don't specify both."#,
        extended: true,
        live: None,
    },
    SettingDesc {
        name: "frameless",
        short: None,
        section: Section::Display,
        kind: Kind::Flag,
        metavar: None,
        default: "",
        range: None,
        group: "window-geometry",
        help: r#"  --frameless              Frameless window"#,
        man: r#"Frameless window."#,
        extended: true,
        live: None,
    },
    SettingDesc {
        name: "output-custom-log",
        short: None,
        section: Section::CommandLine,
        kind: Kind::String,
        metavar: Some("FILE"),
        default: "",
        range: None,
        group: "custom-log",
        help: r#"  --output-custom-log FILE  Output a custom format log file ('-' for STDOUT)."#,
        man: r#"Output a custom format log file ('\-' for STDOUT)."#,
        extended: true,
        live: None,
    },
    SettingDesc {
        name: "background-colour",
        short: Some("b"),
        section: Section::Gource,
        kind: Kind::Colour,
        metavar: Some("FFFFFF"),
        default: "",
        range: None,
        group: "background",
        help: r#"  -b, --background-colour  FFFFFF    Background colour in hex"#,
        man: r#"Background colour in hex."#,
        extended: true,
        live: Some(SettingId::BackgroundColour),
    },
    SettingDesc {
        name: "background-image",
        short: None,
        section: Section::Gource,
        kind: Kind::String,
        metavar: Some("IMAGE"),
        default: "",
        range: None,
        group: "background",
        help: r#"      --background-image   IMAGE     Set a background image"#,
        man: r#"Set a background image."#,
        extended: true,
        live: None,
    },
    SettingDesc {
        name: "bloom-multiplier",
        short: None,
        section: Section::Gource,
        kind: Kind::Float,
        metavar: None,
        default: "1.0",
        range: Some((0.00, 5.00)),
        group: "bloom",
        help: r#"  --bloom-multiplier       Adjust the amount of bloom (default: 1.0)"#,
        man: r#"Adjust the amount of bloom."#,
        extended: true,
        live: Some(SettingId::BloomMultiplier),
    },
    SettingDesc {
        name: "bloom-intensity",
        short: None,
        section: Section::Gource,
        kind: Kind::Float,
        metavar: None,
        default: "0.75",
        range: Some((0.00, 2.00)),
        group: "bloom",
        help: r#"  --bloom-intensity        Adjust the intensity of the bloom (default: 0.75)"#,
        man: r#"Adjust the intensity of the bloom."#,
        extended: true,
        live: Some(SettingId::BloomIntensity),
    },
    SettingDesc {
        name: "camera-mode",
        short: None,
        section: Section::Gource,
        kind: Kind::String,
        metavar: Some("MODE"),
        default: "",
        range: None,
        group: "camera",
        help: r#"  --camera-mode MODE       Camera mode (overview,track)"#,
        man: r#"Camera mode (overview,track)."#,
        extended: true,
        live: Some(SettingId::CameraMode),
    },
    SettingDesc {
        name: "crop",
        short: None,
        section: Section::Gource,
        kind: Kind::String,
        metavar: Some("AXIS"),
        default: "",
        range: None,
        group: "camera",
        help: r#"  --crop AXIS              Crop view on an axis (vertical,horizontal)"#,
        man: r#"Crop view on an axis (vertical,horizontal)."#,
        extended: true,
        live: None,
    },
    SettingDesc {
        name: "padding",
        short: None,
        section: Section::Gource,
        kind: Kind::Float,
        metavar: Some("FLOAT"),
        default: "1.1",
        range: Some((0.00, 10.00)),
        group: "camera",
        help: r#"  --padding FLOAT          Camera view padding (default: 1.1)"#,
        man: r#"Camera view padding."#,
        extended: true,
        live: None,
    },
    SettingDesc {
        name: "disable-auto-rotate",
        short: None,
        section: Section::Gource,
        kind: Kind::Flag,
        metavar: None,
        default: "",
        range: None,
        group: "auto-rotate",
        help: r#"  --disable-auto-rotate    Disable automatic camera rotation"#,
        man: r#"Disable automatic camera rotation."#,
        extended: true,
        live: None,
    },
    SettingDesc {
        name: "disable-input",
        short: None,
        section: Section::Gource,
        kind: Kind::Flag,
        metavar: None,
        default: "",
        range: None,
        group: "input",
        help: r#"  --disable-input          Disable keyboard and mouse input"#,
        man: r#"Disable keyboard and mouse input."#,
        extended: true,
        live: None,
    },
    SettingDesc {
        name: "date-format",
        short: None,
        section: Section::Gource,
        kind: Kind::String,
        metavar: Some("FORMAT"),
        default: "",
        range: None,
        group: "date-formatting",
        help: r#"  --date-format FORMAT     Specify display date string (strftime format)"#,
        man: r#"Specify display date string (strftime format)."#,
        extended: true,
        live: None,
    },
    SettingDesc {
        name: "font-file",
        short: None,
        section: Section::Gource,
        kind: Kind::String,
        metavar: Some("FILE"),
        default: "",
        range: None,
        group: "fonts",
        help: r#"  --font-file FILE         Specify the font"#,
        man: r#"Specify the font. Should work with most font file formats supported by FreeType, such as TTF and OTF, among others."#,
        extended: true,
        live: None,
    },
    SettingDesc {
        name: "font-scale",
        short: None,
        section: Section::Gource,
        kind: Kind::Float,
        metavar: Some("SCALE"),
        default: "1.0",
        range: Some((0.10, 4.00)),
        group: "fonts",
        help: r#"  --font-scale SCALE       Scale the size of all fonts"#,
        man: r#"Scale the size of all fonts."#,
        extended: true,
        live: Some(SettingId::FontScale),
    },
    SettingDesc {
        name: "font-size",
        short: None,
        section: Section::Gource,
        kind: Kind::Int,
        metavar: Some("SIZE"),
        default: "22",
        range: None,
        group: "fonts",
        help: r#"  --font-size SIZE         Font size used by date and title"#,
        man: r#"Font size used by the date and title."#,
        extended: true,
        live: None,
    },
    SettingDesc {
        name: "file-font-size",
        short: None,
        section: Section::Gource,
        kind: Kind::Int,
        metavar: Some("SIZE"),
        default: "16",
        range: None,
        group: "fonts",
        help: r#"  --file-font-size SIZE    Font size for filenames"#,
        man: r#"Font size of filenames."#,
        extended: true,
        live: None,
    },
    SettingDesc {
        name: "dir-font-size",
        short: None,
        section: Section::Gource,
        kind: Kind::Int,
        metavar: Some("SIZE"),
        default: "18",
        range: None,
        group: "fonts",
        help: r#"  --dir-font-size SIZE     Font size for directory names"#,
        man: r#"Font size of directory names."#,
        extended: true,
        live: None,
    },
    SettingDesc {
        name: "user-font-size",
        short: None,
        section: Section::Gource,
        kind: Kind::Int,
        metavar: Some("SIZE"),
        default: "18",
        range: None,
        group: "fonts",
        help: r#"  --user-font-size SIZE    Font size for user names"#,
        man: r#"Font size of user names."#,
        extended: true,
        live: None,
    },
    SettingDesc {
        name: "font-colour",
        short: None,
        section: Section::Gource,
        kind: Kind::Colour,
        metavar: Some("FFFFFF"),
        default: "",
        range: None,
        group: "fonts",
        help: r#"  --font-colour FFFFFF     Font colour used by date and title in hex"#,
        man: r#"Font colour used by the date and title in hex."#,
        extended: true,
        live: Some(SettingId::TextColour),
    },
    SettingDesc {
        name: "file-extensions",
        short: None,
        section: Section::Gource,
        kind: Kind::Flag,
        metavar: None,
        default: "",
        range: None,
        group: "file-display",
        help: r#"  --file-extensions          Show filename extensions only"#,
        man: r#"Show filename extensions only."#,
        extended: true,
        live: None,
    },
    SettingDesc {
        name: "file-extension-fallback",
        short: None,
        section: Section::Gource,
        kind: Kind::Flag,
        metavar: None,
        default: "",
        range: None,
        group: "file-display",
        help: r#"  --file-extension-fallback  Use filename as extension if the extension
                             is missing or empty"#,
        man: r#"Use filename as extension if the extension is missing or empty."#,
        extended: true,
        live: None,
    },
    SettingDesc {
        name: "git-branch",
        short: None,
        section: Section::Gource,
        kind: Kind::String,
        metavar: None,
        default: "",
        range: None,
        group: "git-branch-ext",
        help: r#"  --git-branch             Get the git log of a particular branch"#,
        man: r#"Get the git log of a branch other than the current one."#,
        extended: true,
        live: None,
    },
    SettingDesc {
        name: "hide",
        short: None,
        section: Section::Gource,
        kind: Kind::String,
        metavar: Some("DISPLAY_ELEMENT"),
        default: "",
        range: None,
        group: "hide-elements",
        help: r#"  --hide DISPLAY_ELEMENT   bloom,dashboards,date,dirnames,files,filenames,
                           mouse,progress,root,tree,users,usernames"#,
        man: r#"Hide one or more display elements from the list below:

    bloom       \- bloom effect
    dashboards  \- evolution dashboard panels
    date        \- current date
    dirnames    \- names of directories
    files       \- file icons
    filenames   \- names of files
    mouse       \- mouse cursor
    progress    \- progress bar widget
    root        \- root directory of the tree
    tree        \- animated tree structure
    users       \- user avatars
    usernames   \- names of users

Separate multiple elements with commas (eg "mouse,progress,dashboards")"#,
        extended: true,
        live: Some(SettingId::HideFlags),
    },
    SettingDesc {
        name: "logo",
        short: None,
        section: Section::Gource,
        kind: Kind::String,
        metavar: Some("IMAGE"),
        default: "",
        range: None,
        group: "logo",
        help: r#"  --logo IMAGE             Logo to display in the foreground"#,
        man: r#"Logo to display in the foreground."#,
        extended: true,
        live: None,
    },
    SettingDesc {
        name: "logo-offset",
        short: None,
        section: Section::Gource,
        kind: Kind::String,
        metavar: Some("XxY"),
        default: "",
        range: None,
        group: "logo",
        help: r#"  --logo-offset XxY        Offset position of the logo"#,
        man: r#"Offset position of the logo."#,
        extended: true,
        live: None,
    },
    SettingDesc {
        name: "loop-delay-seconds",
        short: None,
        section: Section::Gource,
        kind: Kind::Float,
        metavar: Some("SECONDS"),
        default: "3",
        range: None,
        group: "looping",
        help: r#"  --loop-delay-seconds SECONDS Seconds to delay before looping (default: 3)"#,
        man: r#"Seconds to delay before looping."#,
        extended: true,
        live: None,
    },
    SettingDesc {
        name: "title",
        short: None,
        section: Section::Gource,
        kind: Kind::String,
        metavar: Some("TITLE"),
        default: "",
        range: None,
        group: "title-ext",
        help: r#"  --title TITLE            Set a title"#,
        man: r#"Set a title"#,
        extended: true,
        live: Some(SettingId::Title),
    },
    SettingDesc {
        name: "transparent",
        short: None,
        section: Section::Display,
        kind: Kind::Flag,
        metavar: None,
        default: "",
        range: None,
        group: "transparency",
        help: r#"  --transparent            Make the background transparent"#,
        man: r#"Make the background transparent. Only really useful for screenshots."#,
        extended: true,
        live: None,
    },
    SettingDesc {
        name: "user-filter",
        short: None,
        section: Section::Gource,
        kind: Kind::MultiValue,
        metavar: Some("REGEX"),
        default: "",
        range: None,
        group: "user-filters",
        help: r#"  --user-filter REGEX      Ignore usernames matching this regex"#,
        man: r#"Filter usernames matching the specified regular expression."#,
        extended: true,
        live: Some(SettingId::UserFilterRegex),
    },
    SettingDesc {
        name: "user-show-filter",
        short: None,
        section: Section::Gource,
        kind: Kind::MultiValue,
        metavar: Some("REGEX"),
        default: "",
        range: None,
        group: "user-filters",
        help: r#"  --user-show-filter REGEX Show only usernames matching this regex"#,
        man: r#"Show only usernames matching the specified regular expression."#,
        extended: true,
        live: Some(SettingId::UserShowFilterRegex),
    },
    SettingDesc {
        name: "file-filter",
        short: None,
        section: Section::Gource,
        kind: Kind::MultiValue,
        metavar: Some("REGEX"),
        default: "",
        range: None,
        group: "file-filters",
        help: r#"  --file-filter REGEX      Ignore file paths matching this regex"#,
        man: r#"Filter out file paths matching the specified regular expression."#,
        extended: true,
        live: Some(SettingId::FileFilterRegex),
    },
    SettingDesc {
        name: "file-show-filter",
        short: None,
        section: Section::Gource,
        kind: Kind::MultiValue,
        metavar: Some("REGEX"),
        default: "",
        range: None,
        group: "file-filters",
        help: r#"  --file-show-filter REGEX Show only file paths matching this regex"#,
        man: r#"Show only file paths matching the specified regular expression."#,
        extended: true,
        live: Some(SettingId::FileShowFilterRegex),
    },
    SettingDesc {
        name: "user-friction",
        short: None,
        section: Section::Gource,
        kind: Kind::Float,
        metavar: Some("SECONDS"),
        default: "0.67",
        range: Some((0.10, 10.00)),
        group: "user-dynamics",
        help: r#"  --user-friction SECONDS  Change the rate users slow down (default: 0.67)"#,
        man: r#"Time users take to come to a halt."#,
        extended: true,
        live: Some(SettingId::UserFriction),
    },
    SettingDesc {
        name: "user-scale",
        short: None,
        section: Section::Gource,
        kind: Kind::String,
        metavar: Some("SCALE"),
        default: "1.0",
        range: Some((0.10, 10.00)),
        group: "user-dynamics",
        help: r#"  --user-scale SCALE       Change scale of users (default: 1.0)"#,
        man: r#"Change scale of user avatars."#,
        extended: true,
        live: Some(SettingId::UserScale),
    },
    SettingDesc {
        name: "max-user-speed",
        short: None,
        section: Section::Gource,
        kind: Kind::Float,
        metavar: Some("UNITS"),
        default: "500",
        range: Some((10.00, 2000.00)),
        group: "user-dynamics",
        help: r#"  --max-user-speed UNITS   Speed users can travel per second (default: 500)"#,
        man: r#"Max speed users can travel per second."#,
        extended: true,
        live: Some(SettingId::UserSpeed),
    },
    SettingDesc {
        name: "follow-user",
        short: None,
        section: Section::Gource,
        kind: Kind::MultiValue,
        metavar: Some("USER"),
        default: "",
        range: None,
        group: "user-highlighting",
        help: r#"  --follow-user USER       Camera will automatically follow this user"#,
        man: r#"Have the camera automatically follow a particular user."#,
        extended: true,
        live: None,
    },
    SettingDesc {
        name: "highlight-dirs",
        short: None,
        section: Section::Gource,
        kind: Kind::Flag,
        metavar: None,
        default: "",
        range: None,
        group: "user-highlighting",
        help: r#"  --highlight-dirs         Highlight the names of all directories"#,
        man: r#"Highlight the names of all directories."#,
        extended: true,
        live: None,
    },
    SettingDesc {
        name: "highlight-user",
        short: None,
        section: Section::Gource,
        kind: Kind::MultiValue,
        metavar: Some("USER"),
        default: "",
        range: None,
        group: "user-highlighting",
        help: r#"  --highlight-user USER    Highlight the names of a particular user"#,
        man: r#"Highlight the names of a particular user."#,
        extended: true,
        live: None,
    },
    SettingDesc {
        name: "highlight-users",
        short: None,
        section: Section::Gource,
        kind: Kind::Flag,
        metavar: None,
        default: "",
        range: None,
        group: "user-highlighting",
        help: r#"  --highlight-users        Highlight the names of all users"#,
        man: r#"Highlight the names of all users."#,
        extended: true,
        live: None,
    },
    SettingDesc {
        name: "highlight-colour",
        short: None,
        section: Section::Gource,
        kind: Kind::Colour,
        metavar: None,
        default: "",
        range: None,
        group: "colouring",
        help: r#"  --highlight-colour       Font colour for highlighted users in hex."#,
        man: r#"Font colour for highlighted users in hex."#,
        extended: true,
        live: Some(SettingId::HighlightColour),
    },
    SettingDesc {
        name: "selection-colour",
        short: None,
        section: Section::Gource,
        kind: Kind::Colour,
        metavar: None,
        default: "",
        range: None,
        group: "colouring",
        help: r#"  --selection-colour       Font colour for selected users and files."#,
        man: r#"Font colour for selected users and files."#,
        extended: true,
        live: Some(SettingId::SelectionColour),
    },
    SettingDesc {
        name: "filename-colour",
        short: None,
        section: Section::Gource,
        kind: Kind::Colour,
        metavar: None,
        default: "",
        range: None,
        group: "colouring",
        help: r#"  --filename-colour        Font colour for filenames."#,
        man: r#"Font colour for filenames."#,
        extended: true,
        live: None,
    },
    SettingDesc {
        name: "dir-colour",
        short: None,
        section: Section::Gource,
        kind: Kind::Colour,
        metavar: None,
        default: "",
        range: None,
        group: "colouring",
        help: r#"  --dir-colour             Font colour for directories."#,
        man: r#"Font colour for directories."#,
        extended: true,
        live: Some(SettingId::DirColour),
    },
    SettingDesc {
        name: "dir-name-depth",
        short: None,
        section: Section::Gource,
        kind: Kind::Int,
        metavar: Some("DEPTH"),
        default: "0",
        range: None,
        group: "dir-names",
        help: r#"  --dir-name-depth DEPTH    Draw names of directories down to a specific depth."#,
        man: r#"Draw names of directories down to a specific depth in the tree."#,
        extended: true,
        live: None,
    },
    SettingDesc {
        name: "dir-name-position",
        short: None,
        section: Section::Gource,
        kind: Kind::Float,
        metavar: Some("FLOAT"),
        default: "0.5",
        range: Some((0.00, 1.00)),
        group: "dir-names",
        help: r#"  --dir-name-position FLOAT Position along edge of the directory name
                            (between 0.0 and 1.0, default is 0.5)."#,
        man: r#"Position along edge of the directory name (between 0.1 and 1.0, default is 0.5)."#,
        extended: true,
        live: None,
    },
    SettingDesc {
        name: "filename-time",
        short: None,
        section: Section::Gource,
        kind: Kind::Float,
        metavar: Some("SECONDS"),
        default: "4.0",
        range: None,
        group: "filename-display",
        help: r#"  --filename-time SECONDS  Duration to keep filenames on screen (default: 4.0)"#,
        man: r#"Duration to keep filenames on screen (>= 2.0)."#,
        extended: true,
        live: None,
    },
    SettingDesc {
        name: "caption-file",
        short: None,
        section: Section::Gource,
        kind: Kind::String,
        metavar: Some("FILE"),
        default: "",
        range: None,
        group: "captions",
        help: r#"  --caption-file FILE         Caption file"#,
        man: r#"Caption file (see Caption Log Format)."#,
        extended: true,
        live: None,
    },
    SettingDesc {
        name: "caption-size",
        short: None,
        section: Section::Gource,
        kind: Kind::Int,
        metavar: Some("SIZE"),
        default: "16",
        range: None,
        group: "captions",
        help: r#"  --caption-size SIZE         Caption font size"#,
        man: r#"Caption size."#,
        extended: true,
        live: None,
    },
    SettingDesc {
        name: "caption-colour",
        short: None,
        section: Section::Gource,
        kind: Kind::Colour,
        metavar: Some("FFFFFF"),
        default: "FFFFFF",
        range: None,
        group: "captions",
        help: r#"  --caption-colour FFFFFF     Caption colour in hex"#,
        man: r#"Caption colour in hex."#,
        extended: true,
        live: None,
    },
    SettingDesc {
        name: "caption-duration",
        short: None,
        section: Section::Gource,
        kind: Kind::Float,
        metavar: Some("SECONDS"),
        default: "10.0",
        range: None,
        group: "captions",
        help: r#"  --caption-duration SECONDS  Caption duration (default: 10.0)"#,
        man: r#"Caption duration."#,
        extended: true,
        live: None,
    },
    SettingDesc {
        name: "caption-offset",
        short: None,
        section: Section::Gource,
        kind: Kind::Int,
        metavar: Some("X"),
        default: "0",
        range: None,
        group: "captions",
        help: r#"  --caption-offset X          Caption horizontal offset"#,
        man: r#"Caption horizontal offset (0 to centre captions)."#,
        extended: true,
        live: None,
    },
    SettingDesc {
        name: "hash-seed",
        short: None,
        section: Section::Gource,
        kind: Kind::Int,
        metavar: Some("SEED"),
        default: "31",
        range: None,
        group: "hash-seeding",
        help: r#"  --hash-seed SEED         Change the seed of hash function."#,
        man: r#"Change the seed of hash function."#,
        extended: true,
        live: None,
    },
    SettingDesc {
        name: "worktree-poll-interval",
        short: None,
        section: Section::Gource,
        kind: Kind::Float,
        metavar: Some("SECONDS"),
        default: "0.25",
        range: None,
        group: "worktree-tuning",
        help: r#"  --worktree-poll-interval SECONDS  Worktree polling debounce interval (default: 0.25)"#,
        man: r#"Debounce and polling interval in seconds for scanning worktree status (default: 0.25)."#,
        extended: true,
        live: None,
    },
    SettingDesc {
        name: "shadow-alpha",
        short: None,
        section: Section::Gource,
        kind: Kind::Float,
        metavar: Some("FLOAT"),
        default: "0.45",
        range: None,
        group: "worktree-tuning",
        help: r#"  --shadow-alpha FLOAT              Alpha opacity of shadow in-flight files (default: 0.45)"#,
        man: r#"Alpha opacity (0.0 to 1.0) of shadow in-flight files and commit beams in worktree watching mode (default: 0.45)."#,
        extended: true,
        live: None,
    },
    SettingDesc {
        name: "file-size-metric",
        short: None,
        section: Section::Gource,
        kind: Kind::String,
        metavar: Some("METRIC"),
        default: "none",
        range: None,
        group: "evolution",
        help: r#"  --file-size-metric METRIC         File size metric (none, size, lines, diff, churn)"#,
        man: r#"Metric used to scale file node sizes: 'none' (fixed default size), 'size' (file byte size), 'lines' (line count), 'diff' (lines modified in commit), or 'churn' (cumulative edits)."#,
        extended: true,
        live: Some(SettingId::FileSizeMetric),
    },
    SettingDesc {
        name: "file-pulse",
        short: None,
        section: Section::Gource,
        kind: Kind::Float,
        metavar: Some("SECONDS"),
        default: "0.0",
        range: Some((0.00, 10.00)),
        group: "evolution",
        help: r#"  --file-pulse SECONDS              Pulse duration for modified files (0 to disable)"#,
        man: r#"Duration in seconds for modified files to pulse with an animated accent ring upon commit (0 to disable)."#,
        extended: true,
        live: Some(SettingId::FilePulse),
    },
    SettingDesc {
        name: "file-colour-mode",
        short: None,
        section: Section::Gource,
        kind: Kind::Colour,
        metavar: Some("MODE"),
        default: "extension",
        range: None,
        group: "evolution",
        help: r#"  --file-colour-mode MODE           File colouring mode (extension, age, churn, cohort)"#,
        man: r#"File colouring scheme: 'extension' (coloured by file extension), 'age' (heat map based on file age), 'churn' (heat map based on modification frequency), or 'cohort' (coloured by initial commit epoch)."#,
        extended: true,
        live: Some(SettingId::FileColourMode),
    },
    SettingDesc {
        name: "dashboard",
        short: None,
        section: Section::Gource,
        kind: Kind::String,
        metavar: Some("PANELS"),
        default: "",
        range: None,
        group: "evolution",
        help: r#"  --dashboard PANELS                Evolution dashboard panels (lines, diff, editors,
                                    commits, theseus, churn, all)"#,
        man: r#"Comma-separated list of evolution analytics dashboards to render on screen: 'lines', 'diff', 'editors', 'commits', 'theseus', 'churn', or 'all'."#,
        extended: true,
        live: None,
    },
    SettingDesc {
        name: "dashboard-period",
        short: None,
        section: Section::Gource,
        kind: Kind::String,
        metavar: Some("PERIOD"),
        default: "day",
        range: None,
        group: "evolution",
        help: r#"  --dashboard-period PERIOD         Dashboard aggregation period (day, week, month, year)"#,
        man: r#"Aggregation time period for dashboard charts: 'day', 'week', 'month', or 'year' (default: 'day')."#,
        extended: true,
        live: Some(SettingId::DashboardPeriod),
    },
    SettingDesc {
        name: "dashboard-window",
        short: None,
        section: Section::Gource,
        kind: Kind::String,
        metavar: Some("DAYS"),
        default: "30",
        range: Some((1.00, 365.00)),
        group: "evolution",
        help: r#"  --dashboard-window DAYS           Dashboard rolling window in days (e.g. 30d)"#,
        man: r#"Rolling aggregation window for dashboard statistics in days, e.g. "30d" (default: 30)."#,
        extended: true,
        live: Some(SettingId::DashboardWindowDays),
    },
    SettingDesc {
        name: "output-stats",
        short: None,
        section: Section::Gource,
        kind: Kind::String,
        metavar: Some("FILE"),
        default: "",
        range: None,
        group: "evolution",
        help: r#"  --output-stats FILE               Write evolution summary statistics JSON on exit"#,
        man: r#"Write evolution summary metrics and statistics as a JSON document to FILE upon simulation exit."#,
        extended: true,
        live: None,
    },
    SettingDesc {
        name: "cache-dir",
        short: None,
        section: Section::Gource,
        kind: Kind::String,
        metavar: Some("DIR"),
        default: "",
        range: None,
        group: "evolution",
        help: r#"  --cache-dir DIR                   Cache directory for repository metadata"#,
        man: r#"Cache directory for persisting precomputed repository metadata and parsed history."#,
        extended: true,
        live: None,
    },
    SettingDesc {
        name: "no-cache",
        short: None,
        section: Section::Gource,
        kind: Kind::Flag,
        metavar: None,
        default: "",
        range: None,
        group: "evolution",
        help: r#"  --no-cache                        Disable cache reads and writes"#,
        man: r#"Disable reading from and writing to the repository metadata cache."#,
        extended: true,
        live: None,
    },
    SettingDesc {
        name: "seed",
        short: None,
        section: Section::Gource,
        kind: Kind::Int,
        metavar: Some("NUMBER"),
        default: "",
        range: None,
        group: "evolution",
        help: r#"  --seed NUMBER                     Random seed for reproducible layouts"#,
        man: r#"Random seed number for reproducible node layouts and simulations."#,
        extended: true,
        live: None,
    },
    SettingDesc {
        name: "path",
        short: None,
        section: Section::Gource,
        kind: Kind::String,
        metavar: Some("PATH"),
        default: ".",
        range: None,
        group: "path-ext",
        help: r#"  --path PATH"#,
        man: r#"Either a supported version control directory, a pre-generated log file (see log commands or the custom log format), a Gource conf file or '-' to read STDIN.

If path is omitted, gource will attempt to read a log from the current directory."#,
        extended: true,
        live: None,
    },
];

/// Additional settings not listed in help/man (internal, legacy aliases, headless video export).
pub static EXTRA_SETTINGS: &[SettingDesc] = &[
    SettingDesc {
        name: "windowed",
        short: None,
        section: Section::Display,
        kind: Kind::Flag,
        metavar: None,
        default: "",
        range: None,
        group: "extra",
        help: "",
        man: "",
        extended: true,
        live: None,
    },
    SettingDesc {
        name: "output-video",
        short: None,
        section: Section::Display,
        kind: Kind::String,
        metavar: None,
        default: "",
        range: None,
        group: "extra",
        help: "",
        man: "",
        extended: true,
        live: None,
    },
    SettingDesc {
        name: "video-codec",
        short: None,
        section: Section::Display,
        kind: Kind::String,
        metavar: None,
        default: "",
        range: None,
        group: "extra",
        help: "",
        man: "",
        extended: true,
        live: None,
    },
    SettingDesc {
        name: "video-bitrate",
        short: None,
        section: Section::Display,
        kind: Kind::String,
        metavar: None,
        default: "",
        range: None,
        group: "extra",
        help: "",
        man: "",
        extended: true,
        live: None,
    },
    SettingDesc {
        name: "video-fps",
        short: None,
        section: Section::Display,
        kind: Kind::Int,
        metavar: None,
        default: "",
        range: None,
        group: "extra",
        help: "",
        man: "",
        extended: true,
        live: None,
    },
    SettingDesc {
        name: "extended-help",
        short: None,
        section: Section::CommandLine,
        kind: Kind::Flag,
        metavar: None,
        default: "",
        range: None,
        group: "extra",
        help: "",
        man: "",
        extended: true,
        live: None,
    },
    SettingDesc {
        name: "stop-on-idle",
        short: None,
        section: Section::Gource,
        kind: Kind::Flag,
        metavar: None,
        default: "",
        range: None,
        group: "extra",
        help: "",
        man: "",
        extended: true,
        live: None,
    },
    SettingDesc {
        name: "hide-date",
        short: None,
        section: Section::Gource,
        kind: Kind::Flag,
        metavar: None,
        default: "",
        range: None,
        group: "extra",
        help: "",
        man: "",
        extended: true,
        live: None,
    },
    SettingDesc {
        name: "hide-files",
        short: None,
        section: Section::Gource,
        kind: Kind::Flag,
        metavar: None,
        default: "",
        range: None,
        group: "extra",
        help: "",
        man: "",
        extended: true,
        live: None,
    },
    SettingDesc {
        name: "hide-users",
        short: None,
        section: Section::Gource,
        kind: Kind::Flag,
        metavar: None,
        default: "",
        range: None,
        group: "extra",
        help: "",
        man: "",
        extended: true,
        live: None,
    },
    SettingDesc {
        name: "hide-tree",
        short: None,
        section: Section::Gource,
        kind: Kind::Flag,
        metavar: None,
        default: "",
        range: None,
        group: "extra",
        help: "",
        man: "",
        extended: true,
        live: None,
    },
    SettingDesc {
        name: "hide-usernames",
        short: None,
        section: Section::Gource,
        kind: Kind::Flag,
        metavar: None,
        default: "",
        range: None,
        group: "extra",
        help: "",
        man: "",
        extended: true,
        live: None,
    },
    SettingDesc {
        name: "hide-filenames",
        short: None,
        section: Section::Gource,
        kind: Kind::Flag,
        metavar: None,
        default: "",
        range: None,
        group: "extra",
        help: "",
        man: "",
        extended: true,
        live: None,
    },
    SettingDesc {
        name: "hide-dirnames",
        short: None,
        section: Section::Gource,
        kind: Kind::Flag,
        metavar: None,
        default: "",
        range: None,
        group: "extra",
        help: "",
        man: "",
        extended: true,
        live: None,
    },
    SettingDesc {
        name: "hide-progress",
        short: None,
        section: Section::Gource,
        kind: Kind::Flag,
        metavar: None,
        default: "",
        range: None,
        group: "extra",
        help: "",
        man: "",
        extended: true,
        live: None,
    },
    SettingDesc {
        name: "hide-bloom",
        short: None,
        section: Section::Gource,
        kind: Kind::Flag,
        metavar: None,
        default: "",
        range: None,
        group: "extra",
        help: "",
        man: "",
        extended: true,
        live: None,
    },
    SettingDesc {
        name: "hide-mouse",
        short: None,
        section: Section::Gource,
        kind: Kind::Flag,
        metavar: None,
        default: "",
        range: None,
        group: "extra",
        help: "",
        man: "",
        extended: true,
        live: None,
    },
    SettingDesc {
        name: "hide-root",
        short: None,
        section: Section::Gource,
        kind: Kind::Flag,
        metavar: None,
        default: "",
        range: None,
        group: "extra",
        help: "",
        man: "",
        extended: true,
        live: None,
    },
    SettingDesc {
        name: "hide-dashboards",
        short: None,
        section: Section::Gource,
        kind: Kind::Flag,
        metavar: None,
        default: "",
        range: None,
        group: "extra",
        help: "",
        man: "",
        extended: true,
        live: Some(SettingId::HideDashboards),
    },
    SettingDesc {
        name: "ffp",
        short: None,
        section: Section::Gource,
        kind: Kind::Flag,
        metavar: None,
        default: "",
        range: None,
        group: "extra",
        help: "",
        man: "",
        extended: true,
        live: None,
    },
    SettingDesc {
        name: "git-log-command",
        short: None,
        section: Section::CommandLine,
        kind: Kind::Flag,
        metavar: None,
        default: "",
        range: None,
        group: "extra",
        help: "",
        man: "",
        extended: true,
        live: None,
    },
    SettingDesc {
        name: "cvs-exp-command",
        short: None,
        section: Section::CommandLine,
        kind: Kind::Flag,
        metavar: None,
        default: "",
        range: None,
        group: "extra",
        help: "",
        man: "",
        extended: true,
        live: None,
    },
    SettingDesc {
        name: "cvs2cl-command",
        short: None,
        section: Section::CommandLine,
        kind: Kind::Flag,
        metavar: None,
        default: "",
        range: None,
        group: "extra",
        help: "",
        man: "",
        extended: true,
        live: None,
    },
    SettingDesc {
        name: "svn-log-command",
        short: None,
        section: Section::CommandLine,
        kind: Kind::Flag,
        metavar: None,
        default: "",
        range: None,
        group: "extra",
        help: "",
        man: "",
        extended: true,
        live: None,
    },
    SettingDesc {
        name: "hg-log-command",
        short: None,
        section: Section::CommandLine,
        kind: Kind::Flag,
        metavar: None,
        default: "",
        range: None,
        group: "extra",
        help: "",
        man: "",
        extended: true,
        live: None,
    },
    SettingDesc {
        name: "bzr-log-command",
        short: None,
        section: Section::CommandLine,
        kind: Kind::Flag,
        metavar: None,
        default: "",
        range: None,
        group: "extra",
        help: "",
        man: "",
        extended: true,
        live: None,
    },
    SettingDesc {
        name: "log-level",
        short: None,
        section: Section::CommandLine,
        kind: Kind::String,
        metavar: None,
        default: "",
        range: None,
        group: "extra",
        help: "",
        man: "",
        extended: true,
        live: None,
    },
];

/// Option aliases (short flags, legacy names).
pub static ALIASES: &[(&'static str, &'static str)] = &[
    ("p", "start-position"),
    ("a", "auto-skip-seconds"),
    ("s", "seconds-per-day"),
    ("t", "stop-at-time"),
    ("i", "file-idle-time"),
    ("e", "elasticity"),
    ("h", "help"),
    ("?", "help"),
    ("H", "extended-help"),
    ("b", "background-colour"),
    ("c", "time-scale"),
    ("background", "background-colour"),
    ("disable-bloom", "hide-bloom"),
    ("disable-progress", "hide-progress"),
    ("highlight-all-users", "highlight-users"),
    ("f", "fullscreen"),
    ("w", "windowed"),
    ("o", "output-ppm-stream"),
    ("r", "output-framerate"),
];

/// Find a setting descriptor by name across SETTINGS and EXTRA_SETTINGS.
pub fn find_setting(name: &str) -> Option<&'static SettingDesc> {
    SETTINGS
        .iter()
        .chain(EXTRA_SETTINGS.iter())
        .find(|s| s.name == name)
}

/// Resolve an option alias (e.g. "p" -> "start-position", "w" -> "windowed").
pub fn resolve_alias(opt: &str) -> &str {
    for &(alias, target) in ALIASES {
        if opt == alias {
            return target;
        }
    }
    opt
}

/// Look up the option type name ("bool", "int", "float", "string", "multi-value").
pub fn option_type(opt: &str) -> Option<&'static str> {
    let canonical = resolve_alias(opt);
    find_setting(canonical).map(|s| s.kind.type_name())
}

/// Look up the option section ("gource", "display", "command-line").
pub fn option_section(opt: &str) -> &'static str {
    let canonical = resolve_alias(opt);
    find_setting(canonical)
        .map(|s| s.section.as_str())
        .unwrap_or("gource")
}

/// Look up the CLI flag corresponding to a SettingId.
pub fn cli_flag_for_setting_id(id: SettingId) -> Option<&'static str> {
    match id {
        SettingId::BackgroundColour => Some("--background-colour"),
        SettingId::DirColour => Some("--dir-colour"),
        SettingId::TextColour => Some("--font-colour"),
        SettingId::HighlightColour => Some("--highlight-colour"),
        SettingId::SelectionColour => Some("--selection-colour"),
        SettingId::BloomMultiplier => Some("--bloom-multiplier"),
        SettingId::BloomIntensity => Some("--bloom-intensity"),
        SettingId::FontScale => Some("--font-scale"),
        SettingId::Title => Some("--title"),
        SettingId::CameraMode => Some("--camera-mode"),
        SettingId::HideFlags => Some("--hide"),
        SettingId::FilePulse => Some("--file-pulse"),
        SettingId::FileColourMode => Some("--file-colour-mode"),
        SettingId::HideDashboards => Some("--hide-dashboards"),
        SettingId::DashboardPeriod => Some("--dashboard-period"),
        SettingId::DashboardWindowDays => Some("--dashboard-window"),
        SettingId::Elasticity => Some("--elasticity"),
        SettingId::UserFriction => Some("--user-friction"),
        SettingId::UserSpeed => Some("--max-user-speed"),
        SettingId::UserScale => Some("--user-scale"),
        SettingId::FileIdleTime => Some("--file-idle-time"),
        SettingId::MaxFileLag => Some("--max-file-lag"),
        SettingId::FileSizeMetric => Some("--file-size-metric"),
        SettingId::TuningDirPadding
        | SettingId::TuningMinDirSize
        | SettingId::TuningFileDiameter
        | SettingId::TuningGravity
        | SettingId::TuningParentPull
        | SettingId::TuningSiblingPush
        | SettingId::TuningBeamLength
        | SettingId::TuningActionDistance
        | SettingId::TuningPersonalSpace
        | SettingId::TuningShadowStrength => None,
        SettingId::SecondsPerDay => Some("--seconds-per-day"),
        SettingId::AutoSkipSeconds => Some("--auto-skip-seconds"),
        SettingId::TimeScale => Some("--time-scale"),
        SettingId::Loop => Some("--loop"),
        SettingId::MaxFiles => Some("--max-files"),
        SettingId::FileFilterRegex => Some("--file-filter"),
        SettingId::FileShowFilterRegex => Some("--file-show-filter"),
        SettingId::UserFilterRegex => Some("--user-filter"),
        SettingId::UserShowFilterRegex => Some("--user-show-filter"),
    }
}

/// Render the help text pure function from the descriptor table.

pub const HELP_PATH_INFO: &str = r#"
PATH may be a supported version control directory, a log file, a gource config
file, or '-' to read STDIN. If omitted, gource will attempt to generate a log
from the current directory.
"#;

pub const HELP_SHORT_SUFFIX: &str = r#"
To see the full command line options use '-H'
"#;

pub fn generate_help_text(extended: bool) -> String {
    let mut out = format!(
        "Gource v{}\nUsage: gource [options] [path]\n\nOptions:\n",
        crate::GOURCE_VERSION
    );
    let mut last_group = "";

    // Base options
    for s in SETTINGS.iter().filter(|s| !s.extended) {
        if !last_group.is_empty() && s.group != last_group {
            out.push('\n');
        }
        last_group = s.group;
        out.push_str(s.help);
        out.push('\n');
    }

    if extended {
        out.push_str("\nExtended Options:\n\n");
        last_group = "";
        for s in SETTINGS.iter().filter(|s| s.extended) {
            if !last_group.is_empty() && s.group != last_group {
                out.push('\n');
            }
            last_group = s.group;
            out.push_str(s.help);
            out.push('\n');
        }
    }

    out.push_str(HELP_PATH_INFO);
    if !extended {
        out.push_str(HELP_SHORT_SUFFIX);
    }
    out.push('\n');
    out
}

/// Generate troff man page for data/gource.1 from descriptor table.
pub fn generate_man_page() -> String {
    let mut out = String::new();
    out.push_str(MAN_PAGE_HEADER);

    for s in SETTINGS {
        // Format the .TP entry
        if s.name == "help" {
            out.push_str(
                ".TP 8\n\\fB\\-h, \\-\\-help\\fR\nHelp ('\\fB-H\\fR' for extended help).\n",
            );
        } else if s.name == "viewport" {
            out.push_str(".TP\n\\fB\\-WIDTHxHEIGHT, \\-\\-viewport WIDTHxHEIGHT\\fR\n");
            out.push_str(s.man);
            out.push('\n');
        } else if s.name == "fullscreen" {
            out.push_str(".TP\n\\fB\\-f\\fR\nFullscreen.\n");
        } else if s.name == "path" {
            out.push_str(".TP\n\\fB\\-\\-path PATH\\fR\n.TP\n\\fBpath\\fR\n");
            out.push_str(s.man);
            out.push('\n');
        } else {
            out.push_str(".TP\n");
            // format flags:
            // e.g. short + long or just long
            let troff_name = s.name.replace('-', "\\-");
            let flag_str = match (s.short, s.metavar) {
                (Some(sh), Some(mv)) => {
                    let troff_mv = if mv.starts_with('"') && mv.ends_with('"') {
                        format!("\"{}\"", &mv[1..mv.len() - 1].replace('-', "\\-"))
                    } else {
                        mv.to_string()
                    };
                    format!("\\fB\\-{}, \\-\\-{} {}\\fR", sh, troff_name, troff_mv)
                }
                (Some(sh), None) => {
                    format!("\\fB\\-{}, \\-\\-{} \\fR", sh, troff_name)
                }
                (None, Some(mv)) => {
                    let troff_mv = if mv.starts_with('"') && mv.ends_with('"') {
                        format!("\"{}\"", &mv[1..mv.len() - 1].replace('-', "\\-"))
                    } else {
                        mv.to_string()
                    };
                    format!("\\fB\\-\\-{} {}\\fR", troff_name, troff_mv)
                }
                (None, None) => {
                    format!("\\fB\\-\\-{}\\fR", troff_name)
                }
            };
            out.push_str(&flag_str);
            out.push('\n');
            out.push_str(s.man);
            out.push('\n');
        }
    }

    out.push_str(MAN_PAGE_FOOTER);
    out
}

const MAN_PAGE_HEADER: &str = r#".TH Gource 1
.SH NAME
Gource - a software version control visualization
.SH SYNOPSIS
\fIgource\fR
[options] [path]
.SH DESCRIPTION
\fIgource\fR is an OpenGL-based 3D visualisation tool for source control repositories.

The repository is displayed as a tree where the root of the repository is the centre, directories are branches and files are leaves. Contributors to the source code appear and disappear as they contribute to specific files and directories.
.SH REQUIREMENTS
\fIgource\fR
requires a OpenGL capable video card to run.
.SH OPTIONS
"#;

const MAN_PAGE_FOOTER: &str = r#"
.SS Git, Bazaar, Mercurial and SVN Examples

View the log of the repository in the current path:

.ti 10
\fIgource\fR

View the log of a project in the specified directory:

.ti 10
\fIgource\fR my\-project\-dir

For large projects, generating a log of the project history may take a long time. For centralized VCS like SVN, generating the log will put load on the central VCS server.

In these cases, you may like to save a copy of the log for later use.

You can generate a log in the VCS specific log format using the \-\-\log\-command VCS option:

.ti 10
cd my\-svn\-project
.ti 10
\`\fIgource\fR \-\-\log\-command svn\` > my\-svn\-project.log
.ti 10
\fIgource\fR my\-svn\-project.log

You can also have Gource write a copy of the log file in its own format:

.ti 10
\fIgource\fR \-\-\output\-custom\-log my\-project\-custom.log

.SS Live and Multi-Repository Watching Examples

Watch a repository in real time with live polling:

.ti 10
\fIgource\fR \-\-\live my\-project\-dir

Watch multiple repositories simultaneously with combined commits:

.ti 10
\fIgource\fR \-\-\live \-\-\watch\-paths /path/to/repo1,/path/to/repo2

Watch all linked git worktrees for uncommitted in-flight changes:

.ti 10
\fIgource\fR \-\-\live \-\-\watch\-worktrees

Watch a remote GitHub repository directly:

.ti 10
\fIgource\fR \-\-\github torvalds/linux

.SS CVS Support

Use 'cvs2cl' to generate the log and then pass it to Gource:

.ti 10
cvs2cl \-\-\chrono \-\-\stdout \-\-\xml \-g\-q > my\-cvs\-project.log
.ti 10
gource my\-cvs\-project.log

.SS Custom Log Format

If you want to use Gource with something other than the supported systems, there is a pipe ('|') delimited custom log format:

.ti 10
timestamp - An ISO 8601 or unix timestamp of when the update occurred.
.ti 10
username  - The name of the user who made the update.
.ti 10
type      - Single character for the update type - (A)dded, (M)odified or (D)eleted.
.ti 10
file      - Path of the file updated.
.ti 10
colour    - A colour for the file in hex (FFFFFF) format. Optional.

.SS Caption Log Format

Gource can display captions along the timeline by specifying a caption file (using \-\-\caption\-file) in the pipe ('|') delimited format below:

.ti 10
timestamp - An ISO 8601 or unix timestamp of when to display the caption.
.ti 10
caption   - The caption

.SS Recording Videos

See the guide on the homepage for examples of recording videos with Gource:

.ti 10
https://github.com/acaudwell/Gource/wiki/Videos

.SS More Information

Visit the Gource homepage for guides and examples of using Gource with various version control systems:

.ti 10
http://gource.io

.SH INTERFACE
The time shown in the top left of the screen is set initially from the first log entry read and is incremented according to the simulation speed (\-\-\seconds\-per\-day).

Pressing SPACE at any time will pause/resume the simulation. While paused you may use the mouse to inspect the detail of individual files and users.

TAB cycles through selecting the current visible users.

The camera mode, either tracking activity or showing the entire code tree, can
be toggled using the Middle mouse button.

You can drag the left mouse button to manually control the camera. The right
mouse button rotates the view.

Interactive keyboard commands:
.sp
.ti 10
(V)   Toggle camera mode
.ti 10
(C)   Displays Gource logo
.ti 10
(K)   Toggle file extension key
.ti 10
(M)   Toggle mouse visibility
.ti 10
(N)   Jump forward in time to next log entry
.ti 10
(S)   Randomize colours
.ti 10
(D)   Toggle directory name display mode
.ti 10
(F)   Toggle file name display mode
.ti 10
(U)   Toggle user name display mode
.ti 10
(G)   Toggle display of users
.ti 10
(T)   Toggle display of directory tree edges
.ti 10
(R)   Toggle display of root directory edges
.ti 10
(<>)  Adjust time scale / user avatar movement speed
.ti 10
(+-)  Adjust simulation speed
.ti 10
(Keypad +-) Adjust camera zoom
.ti 10
(TAB) Cycle through visible users
.ti 10
(F12) Screenshot
.ti 10
(Alt+Enter) Fullscreen toggle
.ti 10
(ESC) Quit
.SH AUTHOR
.nf
 Written by Andrew Caudwell

 Project Homepage: http://gource.io
.SH COPYRIGHT
.nf
 Copyright (C) 2009 Andrew Caudwell (acaudwell@gmail.com)

 This program is free software; you can redistribute it and/or
 modify it under the terms of the GNU General Public License
 as published by the Free Software Foundation; either version
 3 of the License, or (at your option) any later version.

 This program is distributed in the hope that it will be useful,
 but WITHOUT ANY WARRANTY; without even the implied warranty of
 MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 GNU General Public License for more details.

 You should have received a copy of the GNU General Public License
 along with this program.  If not, see <http://www.gnu.org/licenses/>.
.fi
.SH ACKNOWLEDGEMENTS
.nf
 Catalyst IT (catalyst.net.nz)

 For supporting the development of Gource!
.fi
"#;

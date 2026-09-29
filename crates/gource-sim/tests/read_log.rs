//! `Gource::read_log`, a port of C++ `Gource::readLog`.

use gource_draw::DrawList;
use gource_settings::{CliAction, parse_command_line};
use gource_sim::app::{AppOptions, GourceApp};
use gource_sim::gource::Gource;
use gource_sim::platform::{PlatformRequest, Viewport};

/// 2021-11-01 00:00:00 UTC.
const T0: i64 = 1635724800;
const HOUR: i64 = 3600;
const DAY: i64 = 86400;

struct Run {
    app: GourceApp,
    requests: Vec<PlatformRequest>,
    _dir: tempfile::TempDir,
}

impl Run {
    fn start(log: &str, args: &[&str]) -> Run {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("log.txt");
        std::fs::write(&path, log).unwrap();
        let mut argv: Vec<String> = args.iter().map(|a| a.to_string()).collect();
        argv.push(path.to_str().unwrap().to_string());
        let CliAction::Run(config) = parse_command_line(&argv).unwrap() else {
            panic!("expected a run");
        };
        Run {
            app: GourceApp::new(config, AppOptions::default()).unwrap(),
            requests: Vec::new(),
            _dir: dir,
        }
    }

    fn gource(&self) -> &Gource {
        self.app.shell().gource.as_ref().expect("gource is running")
    }

    /// Once the log has loaded, advance up to `frames` frames of 1/60 s,
    /// stopping early once `done`.
    fn run(&mut self, frames: usize, done: impl Fn(&Gource) -> bool) {
        let viewport = Viewport::new(640, 480);
        let mut list = DrawList::new(glam::UVec2::new(640, 480));
        let mut frame = |run: &mut Run| {
            run.app.frame(1.0 / 60.0, viewport, &mut list);
            run.requests.extend(run.app.take_requests());
        };
        // The first frame starts Gource, which loads the log on a thread.
        if self.app.shell().gource.is_none() {
            frame(self);
        }
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        while !self
            .gource()
            .logmill
            .as_ref()
            .is_none_or(|m| m.is_finished())
        {
            assert!(std::time::Instant::now() < deadline, "the log did not load");
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        for _ in 0..frames {
            frame(self);
            if self.app.shell().gource.as_ref().is_some_and(&done) {
                return;
            }
        }
    }

    fn has_file(&self, path: &str) -> bool {
        self.gource().world.files_by_path.contains_key(path)
    }
}

fn custom_log(commits: &[(i64, &str, &str)]) -> String {
    commits
        .iter()
        .map(|(t, user, file)| format!("{t}|{user}|A|{file}\n"))
        .collect()
}

#[test]
fn stop_date_ends_a_log_file() {
    let log = custom_log(&[
        (T0, "Alice", "/a.txt"),
        (T0 + HOUR, "Bob", "/b.txt"),
        (T0 + 3 * HOUR, "Carol", "/c.txt"),
    ]);
    let mut run = Run::start(
        &log,
        &[
            "--seconds-per-day",
            "0.01",
            "--stop-date",
            "2021-11-01 02:00:00Z",
        ],
    );
    run.run(300, |g| g.stop_position_reached && g.commitqueue.is_empty());
    let gource = run.gource();
    assert!(gource.stop_position_reached);
    assert!(run.has_file("/a.txt") && run.has_file("/b.txt"));
    assert!(
        !run.has_file("/c.txt"),
        "a commit after --stop-date was shown"
    );
}

#[test]
fn reads_one_commit_ahead_of_the_current_time() {
    let commits: Vec<(i64, &str, &str)> =
        (0..10).map(|i| (T0 + i * DAY, "Alice", "/a.txt")).collect();
    let mut run = Run::start(&custom_log(&commits), &["--seconds-per-day", "10"]);
    // Well within the first day.
    run.run(20, |_| false);
    let gource = run.gource();
    assert!(gource.currtime > T0 && gource.currtime < T0 + DAY);
    // The queue holds the next commit, ahead of the current time, and the
    // slider shows how far the log has been read: to the line that ends the
    // queued commit, the third of ten lines of the same length.
    let queued: Vec<i64> = gource.commitqueue.iter().map(|c| c.timestamp).collect();
    assert_eq!(queued, [T0 + DAY]);
    assert!(
        (gource.last_percent - 0.3).abs() < 1e-6,
        "{}",
        gource.last_percent
    );
}

#[test]
fn no_commits_found_when_every_commit_is_filtered_out() {
    let log = custom_log(&[(T0, "Alice", "/a.txt"), (T0 + HOUR, "Bob", "/b.txt")]);
    let mut run = Run::start(&log, &["--user-filter", "^(Alice|Bob)$"]);
    run.run(30, |_| false);
    assert!(
        run.requests
            .iter()
            .any(|r| matches!(r, PlatformRequest::Fatal(msg) if msg == "no commits found")),
        "{:?}",
        run.requests
    );
}

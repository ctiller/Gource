#![allow(clippy::field_reassign_with_default)]

use gource_core::StringHasher;
use gource_vcs::{
    CommitFilters, CommitLog, FileAction, LogMill, LogMillStatus, VcsOptions, write_custom_log,
};
use std::path::Path;
use std::process::Command;
use tempfile::tempdir;

#[test]
fn test_log_commands() {
    assert_eq!(
        gource_vcs::log_command("git"),
        Some("git log --reverse --raw --encoding=UTF-8 --no-renames --no-show-signature --pretty=format:user:%aN%n%ct".to_string())
    );
    assert_eq!(
        gource_vcs::log_command("svn"),
        Some("svn log -r 1:HEAD --xml --verbose --quiet".to_string())
    );
    let resource_dir = gource_core::resources::resource_dir();
    assert!(Path::new(&format!("{resource_dir}gource.style")).is_file());
    assert_eq!(
        gource_vcs::log_command("hg"),
        Some(format!(
            "hg log  -r 0:tip --style '{resource_dir}gource.style'"
        ))
    );
    assert_eq!(
        gource_vcs::log_command("bzr"),
        Some("bzr log --verbose -r 1..-1 --short -n0 --forward".to_string())
    );
    assert_eq!(
        gource_vcs::log_command("cvs"),
        Some("gource: please use either 'cvs2cl' or 'cvs-exp'\nTry 'gource --help' for more information.\n".to_string())
    );
    assert_eq!(
        gource_vcs::log_command("cvs-exp"),
        Some("cvs-exp.pl -notree".to_string())
    );
    assert_eq!(
        gource_vcs::log_command("cvs2cl"),
        Some("cvs2cl --chrono --stdout --xml -g-q".to_string())
    );
    assert_eq!(
        gource_vcs::log_command("gitraw"),
        Some("git log --reverse --raw --pretty=raw".to_string())
    );
    assert_eq!(gource_vcs::log_command("unknown"), None);
}

#[test]
fn test_log_command_options() {
    let mut opts = VcsOptions::default();
    opts.author_time = true;
    opts.git_branch = "feature-branch".to_string();
    opts.start_timestamp = 1577836800; // 2020-01-01
    opts.stop_timestamp = 1609459200; // 2021-01-01

    let git_cmd = gource_vcs::formats::log_command_with_options("git", &opts).unwrap();
    assert!(git_cmd.contains("--pretty=format:user:%aN%n%at"));
    assert!(git_cmd.contains("--since "));
    assert!(git_cmd.contains("--until "));
    assert!(git_cmd.ends_with(" feature-branch"));

    let hg_cmd = gource_vcs::formats::log_command_with_options("hg", &opts).unwrap();
    assert!(hg_cmd.contains("--date '"));

    let bzr_cmd = gource_vcs::formats::log_command_with_options("bzr", &opts).unwrap();
    assert!(bzr_cmd.contains("date:"));

    let svn_cmd = gource_vcs::formats::log_command_with_options("svn", &opts).unwrap();
    assert!(svn_cmd.contains("-r {"));
}

#[test]
fn test_custom_parser_fixture() {
    let opts = VcsOptions::default();
    let mut log = CommitLog::open_file("tests/data/custom.log", "custom", &opts).unwrap();
    assert_eq!(log.format_name(), "custom");
    assert!(log.is_seekable());

    let c1 = log.next_commit().unwrap();
    assert_eq!(c1.username, "Alice");
    assert_eq!(c1.timestamp, 1577836800);
    // C++'s regex only accepts A/D/M, so the `R` line doesn't parse: it is
    // dropped and ends Alice's commit (checked with the C++ binary's
    // `--output-custom-log`).
    assert_eq!(c1.files.len(), 3);
    assert_eq!(c1.files[0].filename, "/src/main.rs");
    assert_eq!(c1.files[0].action, FileAction::Add);
    assert_eq!(c1.files[1].filename, "/src/lib.rs");
    assert_eq!(c1.files[1].action, FileAction::Modify);
    assert_eq!(c1.files[2].filename, "/README.md");
    assert_eq!(c1.files[2].action, FileAction::Delete);

    let c2 = log.next_commit().unwrap();
    assert_eq!(c2.username, "Bob");
    assert_eq!(c2.files.len(), 1);

    let c3 = log.next_commit().unwrap();
    assert_eq!(c3.username, "Charlie");
    assert_eq!(c3.files.len(), 2);

    assert!(log.next_commit().is_none());
    assert!(log.is_finished());
}

#[test]
fn test_git_parser_fixture() {
    let opts = VcsOptions::default();
    let mut log = CommitLog::open_file("tests/data/git.log", "git", &opts).unwrap();
    assert_eq!(log.format_name(), "git");

    let c1 = log.next_commit().unwrap();
    assert_eq!(c1.username, "Alice");
    assert_eq!(c1.timestamp, 1577836800);
    assert_eq!(c1.files.len(), 4);
    assert_eq!(c1.files[1].filename, "/src/quoted file.rs");

    let c2 = log.next_commit().unwrap();
    assert_eq!(c2.username, "Bob");
    assert_eq!(c2.files.len(), 1);

    assert!(log.next_commit().is_none());
    assert!(log.is_finished());
}

#[test]
fn test_gitraw_parser_fixture() {
    let opts = VcsOptions::default();
    let mut log = CommitLog::open_file("tests/data/gitraw.log", "gitraw", &opts).unwrap();
    let c1 = log.next_commit().unwrap();
    assert_eq!(c1.username, "Alice");
    assert_eq!(c1.timestamp, 1577836800);
    assert_eq!(c1.files.len(), 2);

    let c2 = log.next_commit().unwrap();
    assert_eq!(c2.username, "Bob");
    assert_eq!(c2.timestamp, 1577836900);
    assert_eq!(c2.files.len(), 1);

    assert!(log.next_commit().is_none());
}

#[test]
fn test_hg_parser_fixture() {
    let opts = VcsOptions::default();
    let mut log = CommitLog::open_file("tests/data/hg.log", "hg", &opts).unwrap();
    let c1 = log.next_commit().unwrap();
    assert_eq!(c1.username, "Alice");
    assert_eq!(c1.timestamp, 1577836800);
    assert_eq!(c1.files.len(), 3);

    let c2 = log.next_commit().unwrap();
    assert_eq!(c2.username, "Bob");
    assert_eq!(c2.timestamp, 1577836900);
    assert_eq!(c2.files.len(), 1);

    assert!(log.next_commit().is_none());
}

#[test]
fn test_bzr_parser_fixture() {
    let opts = VcsOptions::default();
    let mut log = CommitLog::open_file("tests/data/bzr.log", "bzr", &opts).unwrap();
    let c1 = log.next_commit().unwrap();
    assert_eq!(c1.username, "Alice");
    assert_eq!(c1.files.len(), 4);

    let c2 = log.next_commit().unwrap();
    assert_eq!(c2.username, "Bob");
    assert_eq!(c2.files.len(), 1);

    assert!(log.next_commit().is_none());
}

#[test]
fn test_svn_parser_fixture() {
    let opts = VcsOptions::default();
    let mut log = CommitLog::open_file("tests/data/svn.log", "svn", &opts).unwrap();
    let c1 = log.next_commit().unwrap();
    assert_eq!(c1.username, "Alice");
    // Directory deletes /trunk/old_dir/ should be included with trailing slash, /trunk/new_dir skipped because action is A
    assert_eq!(c1.files.len(), 3);
    assert_eq!(c1.files[2].filename, "/trunk/old_dir/");

    let c2 = log.next_commit().unwrap();
    assert_eq!(c2.username, "Bob");
    assert_eq!(c2.files.len(), 1);

    assert!(log.next_commit().is_none());
}

#[test]
fn test_cvs_exp_parser_fixture() {
    let opts = VcsOptions::default();
    let mut log = CommitLog::open_file("tests/data/cvs_exp.log", "cvs-exp", &opts).unwrap();
    let c1 = log.next_commit().unwrap();
    assert_eq!(c1.username, "Alice");
    // Attic/deleted.rs ignored
    assert_eq!(c1.files.len(), 2);

    let c2 = log.next_commit().unwrap();
    assert_eq!(c2.username, "Bob");
    assert_eq!(c2.files[0].action, FileAction::Delete);

    assert!(log.next_commit().is_none());
}

#[test]
fn test_cvs2cl_parser_fixture() {
    let opts = VcsOptions::default();
    let mut log = CommitLog::open_file("tests/data/cvs2cl.log", "cvs2cl", &opts).unwrap();
    let c1 = log.next_commit().unwrap();
    assert_eq!(c1.username, "Alice");
    assert_eq!(c1.files.len(), 3);
    assert_eq!(c1.files[2].action, FileAction::Delete);

    let c2 = log.next_commit().unwrap();
    assert_eq!(c2.username, "Bob");
    assert_eq!(c2.files.len(), 1);

    assert!(log.next_commit().is_none());
}

#[test]
fn test_apache_parser_fixture() {
    let opts = VcsOptions::default();
    let mut log = CommitLog::open_file("tests/data/apache.log", "apache", &opts).unwrap();
    let c1 = log.next_commit().unwrap();
    assert_eq!(c1.username, "127.0.0.1");
    assert_eq!(c1.files[0].filename, "/index.html");

    let c2 = log.next_commit().unwrap();
    assert_eq!(c2.username, "127.0.0.1");
    assert_eq!(c2.files[0].filename, "/images/logo.png");

    let c3 = log.next_commit().unwrap();
    assert_eq!(c3.username, "127.0.0.1");
    assert_eq!(c3.files[0].filename, "/docs/index.html");

    assert!(log.next_commit().is_none());
}

#[test]
fn test_seeking_and_percent() {
    let opts = VcsOptions::default();
    let mut log = CommitLog::open_file("tests/data/custom.log", "custom", &opts).unwrap();
    assert!(log.is_seekable());

    let pct_start = log.percent();
    assert_eq!(pct_start, 0.0);

    let _ = log.next_commit();
    let pct_after = log.percent();
    assert!(pct_after > 0.0);

    // Commit at position
    let c = log.commit_at(0.0).unwrap();
    assert_eq!(c.username, "Alice");
    assert_eq!(log.percent(), pct_after);

    // Seek to 0.0
    log.seek_to(0.0);
    assert_eq!(log.percent(), 0.0);
    let c_again = log.next_commit().unwrap();
    assert_eq!(c_again.username, "Alice");

    // Seek near end
    log.seek_to(0.99);
    let _ = log.next_commit();
    assert!(log.is_finished());
}

#[test]
fn test_buffer_commit() {
    let opts = VcsOptions::default();
    let mut log = CommitLog::open_file("tests/data/custom.log", "custom", &opts).unwrap();
    let c = log.next_commit().unwrap();
    assert_eq!(c.username, "Alice");
    assert!(!log.has_buffered_commit());

    log.buffer_commit(c.clone());
    assert!(log.has_buffered_commit());

    let c2 = log.next_commit().unwrap();
    assert_eq!(c2.username, "Alice");
    assert!(!log.has_buffered_commit());
}

#[test]
fn test_filters() {
    let mut filters = CommitFilters::default();
    filters
        .file_filters
        .push(fancy_regex::Regex::new("extra").unwrap());
    filters
        .user_filters
        .push(fancy_regex::Regex::new("Charlie").unwrap());

    let opts = VcsOptions {
        filters,
        ..Default::default()
    };

    let mut log = CommitLog::open_file("tests/data/custom.log", "custom", &opts).unwrap();
    let c1 = log.next_commit().unwrap();
    assert_eq!(c1.username, "Alice");

    let c2 = log.next_commit().unwrap();
    assert_eq!(c2.username, "Bob");

    // Charlie is filtered out
    assert!(log.next_commit().is_none());
}

#[test]
fn test_set_hash_seed() {
    let mut opts = VcsOptions::default();
    opts.hasher = StringHasher::new(10);
    let mut log = CommitLog::open_file("tests/data/custom.log", "custom", &opts).unwrap();
    log.set_hash_seed(42);
    // Hash seed updated without issue
}

#[test]
fn test_real_git_repository() {
    let dir = tempdir().unwrap();
    let repo_path = dir.path();

    // Run git init
    assert!(
        Command::new("git")
            .arg("init")
            .current_dir(repo_path)
            .status()
            .unwrap()
            .success()
    );

    assert!(
        Command::new("git")
            .args(["config", "user.name", "Test User"])
            .current_dir(repo_path)
            .status()
            .unwrap()
            .success()
    );
    assert!(
        Command::new("git")
            .args(["config", "user.email", "test@example.com"])
            .current_dir(repo_path)
            .status()
            .unwrap()
            .success()
    );

    let file1 = repo_path.join("file1.txt");
    std::fs::write(&file1, "hello").unwrap();

    assert!(
        Command::new("git")
            .args(["add", "file1.txt"])
            .current_dir(repo_path)
            .status()
            .unwrap()
            .success()
    );

    let mut commit_cmd = Command::new("git");
    commit_cmd
        .args(["commit", "-m", "First commit"])
        .current_dir(repo_path)
        .env("GIT_AUTHOR_DATE", "1577836800 +0000")
        .env("GIT_COMMITTER_DATE", "1577836800 +0000");
    assert!(commit_cmd.status().unwrap().success());

    // Test logmill auto-detection and blocking fetch on this repo
    let opts = VcsOptions::default();
    let mut log = LogMill::fetch_blocking(repo_path.to_str().unwrap(), &opts).unwrap();
    assert_eq!(log.format_name(), "git");
    let c = log.next_commit().unwrap();
    assert_eq!(c.username, "Test User");
    assert_eq!(c.timestamp, 1577836800);
    assert_eq!(c.files.len(), 1);
    assert_eq!(c.files[0].filename, "/file1.txt");
    assert_eq!(c.files[0].action, FileAction::Add);
}

#[test]
fn test_logmill_background_spawn() {
    let dir = tempdir().unwrap();
    let repo_path = dir.path();

    assert!(
        Command::new("git")
            .arg("init")
            .current_dir(repo_path)
            .status()
            .unwrap()
            .success()
    );
    assert!(
        Command::new("git")
            .args(["config", "user.name", "BG User"])
            .current_dir(repo_path)
            .status()
            .unwrap()
            .success()
    );
    assert!(
        Command::new("git")
            .args(["config", "user.email", "bg@example.com"])
            .current_dir(repo_path)
            .status()
            .unwrap()
            .success()
    );

    std::fs::write(repo_path.join("a.txt"), "content").unwrap();
    assert!(
        Command::new("git")
            .args(["add", "a.txt"])
            .current_dir(repo_path)
            .status()
            .unwrap()
            .success()
    );
    assert!(
        Command::new("git")
            .args(["commit", "-m", "Commit"])
            .current_dir(repo_path)
            .env("GIT_AUTHOR_DATE", "1577836800 +0000")
            .env("GIT_COMMITTER_DATE", "1577836800 +0000")
            .status()
            .unwrap()
            .success()
    );

    let mut mill = LogMill::spawn(repo_path.to_str().unwrap(), VcsOptions::default());
    while !mill.is_finished() {
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert_eq!(mill.status(), LogMillStatus::Success);
    let mut log = mill.take_result().unwrap().unwrap();
    assert_eq!(log.format_name(), "git");
    let c = log.next_commit().unwrap();
    assert_eq!(c.username, "BG User");
}

/// `tests/data/gource-custom.log` is the C++ `--output-custom-log` of this
/// repository at this revision (the last commit before the Rust port).
const GOLDEN_LOG_REVISION: &str = "70db547c6b8b40745fb41a8db7e8d28897be1a9e";

#[test]
fn test_golden_write_custom_log() {
    let repo_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let git = |args: &[&str]| {
        Command::new("git")
            .arg("-C")
            .arg(&repo_path)
            .args(args)
            .output()
            .ok()
            .filter(|out| out.status.success())
            .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string())
    };
    // Needs the full history up to the golden revision (not in shallow or
    // unrelated clones).
    let revision = format!("{GOLDEN_LOG_REVISION}^{{commit}}");
    let has_revision = git(&["cat-file", "-e", &revision]).is_some()
        && git(&["rev-parse", "--is-shallow-repository"]).as_deref() == Some("false");
    if !has_revision {
        eprintln!("Skipping golden test: revision {GOLDEN_LOG_REVISION} not available");
        return;
    }

    let out_dir = tempdir().unwrap();
    let out_file = out_dir.path().join("out.log");
    let out_file_str = out_file.to_str().unwrap();

    // `git log <revision>` produces the log as it was at that revision.
    let opts = VcsOptions {
        git_branch: GOLDEN_LOG_REVISION.to_string(),
        ..VcsOptions::default()
    };
    write_custom_log(repo_path.to_str().unwrap(), out_file_str, &opts).unwrap();

    let generated = std::fs::read_to_string(&out_file).unwrap();
    let expected = std::fs::read_to_string("tests/data/gource-custom.log").unwrap();

    assert_eq!(generated.trim(), expected.trim());
}

#[test]
fn test_write_custom_log_stdout() {
    let opts = VcsOptions::default();
    // Test writing to stdout ("-") from custom log file
    let res = write_custom_log("tests/data/custom.log", "-", &opts);
    assert!(res.is_ok());
}

#[test]
fn test_vcs_error_display() {
    let err_msg = gource_vcs::VcsError::Message("custom error".to_string());
    assert_eq!(err_msg.to_string(), "custom error");

    let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
    let err_io = gource_vcs::VcsError::Io(io_err);
    assert!(err_io.to_string().contains("file not found"));
}

#[test]
fn test_stream_log() {
    use std::io::Cursor;
    let data = "1577836800|Alice|A|file1.txt\r\n1577836800|Alice|M|file2.txt\n";
    let cursor = Cursor::new(data.as_bytes());
    let stream = gource_vcs::log::StreamLog::from_reader(cursor);
    let opts = VcsOptions::default();
    let mut clog = CommitLog::from_stream("custom", stream, opts);
    assert!(!clog.is_seekable());
    assert_eq!(clog.format_name(), "custom");
    assert_eq!(clog.percent(), 0.0);
    clog.seek_to(0.5);
    assert!(clog.commit_at(0.5).is_none());

    // Give the background thread time to read into channel
    std::thread::sleep(std::time::Duration::from_millis(50));
    let c = clog.next_commit().unwrap();
    assert_eq!(c.username, "Alice");
    assert_eq!(c.files.len(), 2);
    // After reading, stream finishes
    std::thread::sleep(std::time::Duration::from_millis(50));
    assert!(clog.next_commit().is_none());
}

#[test]
fn test_stream_log_empty_and_error() {
    use std::io::Cursor;
    let cursor = Cursor::new(b"");
    let stream = gource_vcs::log::StreamLog::from_reader(cursor);
    let opts = VcsOptions::default();
    let mut clog = CommitLog::from_stream("custom", stream, opts);
    assert!(clog.next_commit().is_none());
}

#[test]
fn test_check_format_stream() {
    use std::io::Cursor;
    let data = "1577836800|Alice|A|file1.txt\n";
    let cursor = Cursor::new(data.as_bytes());
    let stream = gource_vcs::log::StreamLog::from_reader(cursor);
    let opts = VcsOptions::default();
    let mut clog = CommitLog::from_stream("custom", stream, opts);
    std::thread::sleep(std::time::Duration::from_millis(50));
    assert!(clog.check_format());
    assert!(clog.has_buffered_commit());
    let c = clog.next_commit().unwrap();
    assert_eq!(c.username, "Alice");
}

#[test]
fn test_commit_log_next_commit_unvalidated() {
    let opts = VcsOptions::default();
    let mut clog = CommitLog::open_file("tests/data/custom.log", "custom", &opts).unwrap();
    let c = clog.next_commit_unvalidated().unwrap();
    assert_eq!(c.username, "Alice");
}

#[test]
fn test_open_file_validations_and_failures() {
    let opts = VcsOptions::default();
    // Nonexistent file
    assert!(CommitLog::open_file("tests/data/nonexistent.log", "custom", &opts).is_none());
    // Format check failure (wrong first char for git)
    assert!(CommitLog::open_file("tests/data/custom.log", "git", &opts).is_none());
    // Format check failure for svn
    assert!(CommitLog::open_file("tests/data/custom.log", "svn", &opts).is_none());
    // Stdin "-" with wrong format or empty
    // Empty file
    let temp = tempfile::NamedTempFile::new().unwrap();
    assert!(CommitLog::open_file(temp.path().to_str().unwrap(), "git", &opts).is_none());
    assert!(CommitLog::open_file(temp.path().to_str().unwrap(), "custom", &opts).is_none());
}

#[test]
fn test_custom_parser_dates() {
    use gource_vcs::formats::custom::parse_date_time;
    // ISO format with Z
    let ts_z = parse_date_time("2020-01-01T12:00:00Z");
    assert!(ts_z.is_some());
    // ISO format with positive offset
    let ts_pos = parse_date_time("2020-01-01 12:00:00 +05:30");
    assert!(ts_pos.is_some());
    // ISO format with negative offset
    let ts_neg = parse_date_time("2020-01-01 12:00:00 -0800");
    assert!(ts_neg.is_some());
    // ISO format without offset (local time)
    let ts_local = parse_date_time("2020-01-01 12:00:00");
    assert!(ts_local.is_some());
    // With milliseconds / microseconds
    let ts_millis = parse_date_time("2020-01-01 12:00:00.123456");
    assert!(ts_millis.is_some());
    let ts_millis_short = parse_date_time("2020-01-01 12:00:00.12");
    assert!(ts_millis_short.is_some());
    // Invalid dates
    assert!(parse_date_time("not-a-date").is_none());
    assert!(parse_date_time("2020-13-45 99:99:99").is_none());

    // Parse commit entry directly
    let mut commit = gource_vcs::Commit::default();
    let opts = VcsOptions::default();
    // Valid entry with empty username and action
    assert!(
        gource_vcs::formats::custom::CustomParser::parse_commit_entry(
            "1577836800|||file.txt",
            &mut commit,
            &opts
        )
        .is_ok()
    );
    assert_eq!(commit.username, "Unknown");
    assert_eq!(commit.files[0].action, FileAction::Add);

    // Invalid regex
    assert!(
        gource_vcs::formats::custom::CustomParser::parse_commit_entry(
            "invalid line",
            &mut commit,
            &opts
        )
        .is_err()
    );

    // Invalid timestamp
    assert!(
        gource_vcs::formats::custom::CustomParser::parse_commit_entry(
            "abc|||file.txt",
            &mut commit,
            &opts
        )
        .is_err()
    );
    assert!(
        gource_vcs::formats::custom::CustomParser::parse_commit_entry(
            "2020-99-99 99:99:99|||file.txt",
            &mut commit,
            &opts
        )
        .is_err()
    );
}

#[test]
fn test_apache_parser_edge_cases() {
    let opts = VcsOptions::default();
    let mut commit = gource_vcs::Commit::default();

    // Query string stripping and empty file -> /index.html
    let line1 =
        r#"127.0.0.1 - testuser [01/Jan/2020:12:00:00 +0000] "GET /?foo=bar HTTP/1.1" 200 1234"#;
    let mut get_line1 = {
        let mut done = false;
        move |l: &mut String| {
            if !done {
                *l = line1.to_string();
                done = true;
                true
            } else {
                false
            }
        }
    };
    assert!(gource_vcs::formats::apache::parse_commit(
        &mut get_line1,
        &mut commit,
        &opts
    ));
    assert_eq!(commit.files[0].filename, "/index.html");

    // Invalid start
    let mut get_line_bad = |l: &mut String| {
        *l = "bad apache line".to_string();
        true
    };
    assert!(!gource_vcs::formats::apache::parse_commit(
        &mut get_line_bad,
        &mut commit,
        &opts
    ));

    // Bad date format
    let line_baddate = r#"127.0.0.1 - testuser [invalid_date] "GET /page.html HTTP/1.1" 200 1234"#;
    let mut get_line_baddate = |l: &mut String| {
        *l = line_baddate.to_string();
        true
    };
    assert!(!gource_vcs::formats::apache::parse_commit(
        &mut get_line_baddate,
        &mut commit,
        &opts
    ));

    // Bad month name
    let line_badmonth =
        r#"127.0.0.1 - testuser [01/Foo/2020:12:00:00 +0000] "GET /page.html HTTP/1.1" 200 1234"#;
    let mut get_line_badmonth = |l: &mut String| {
        *l = line_badmonth.to_string();
        true
    };
    assert!(!gource_vcs::formats::apache::parse_commit(
        &mut get_line_badmonth,
        &mut commit,
        &opts
    ));

    // Bad request line
    let line_badreq = r#"127.0.0.1 - testuser [01/Jan/2020:12:00:00 +0000] "INVALID_REQ" 200 1234"#;
    let mut get_line_badreq = |l: &mut String| {
        *l = line_badreq.to_string();
        true
    };
    assert!(!gource_vcs::formats::apache::parse_commit(
        &mut get_line_badreq,
        &mut commit,
        &opts
    ));
}

#[test]
fn test_bzr_parser_errors_and_options() {
    let opts = VcsOptions::default();
    let mut commit = gource_vcs::Commit::default();

    // Empty bzr line
    let mut get_line_empty = |_: &mut String| false;
    assert!(!gource_vcs::formats::bzr::parse_commit(
        &mut get_line_empty,
        &mut commit,
        &opts
    ));

    // Malformed bzr header
    let mut get_line_bad = |l: &mut String| {
        *l = "malformed bzr line".to_string();
        true
    };
    assert!(!gource_vcs::formats::bzr::parse_commit(
        &mut get_line_bad,
        &mut commit,
        &opts
    ));

    // Bzr log command options with only start or stop
    let mut o1 = VcsOptions::default();
    o1.start_timestamp = 1577836800;
    assert!(gource_vcs::formats::bzr::log_command(&o1).contains("date:2020-01-01.."));
    let mut o2 = VcsOptions::default();
    o2.stop_timestamp = 1609459200;
    assert!(gource_vcs::formats::bzr::log_command(&o2).contains("1..date:2021-01-01"));
}

#[test]
fn test_hg_parser_errors_and_options() {
    let opts = VcsOptions::default();
    let mut commit = gource_vcs::Commit::default();

    assert!(
        gource_vcs::formats::hg::HgParser::parse_commit_entry(
            "not enough pipes",
            &mut commit,
            &opts
        )
        .is_err()
    );
    assert!(
        gource_vcs::formats::hg::HgParser::parse_commit_entry(
            "not_a_ts -0500|alice||file.txt",
            &mut commit,
            &opts
        )
        .is_err()
    );

    let mut o1 = VcsOptions::default();
    o1.start_timestamp = 1577836800;
    assert!(gource_vcs::formats::hg::log_command(&o1).contains("--date '>2020-01-01'"));

    let mut o2 = VcsOptions::default();
    o2.stop_timestamp = 1609459200;
    assert!(gource_vcs::formats::hg::log_command(&o2).contains("--date '<2021-01-01'"));
}

#[test]
fn test_gitraw_parser_errors() {
    let opts = VcsOptions::default();
    let mut commit = gource_vcs::Commit::default();

    let mut get_line_empty = |_: &mut String| false;
    assert!(!gource_vcs::formats::gitraw::parse_commit(
        &mut get_line_empty,
        &mut commit,
        &opts
    ));

    let mut get_line_bad = |l: &mut String| {
        *l = "tree 12345".to_string();
        true
    };
    assert!(!gource_vcs::formats::gitraw::parse_commit(
        &mut get_line_bad,
        &mut commit,
        &opts
    ));
}

#[test]
fn test_git_parser_errors() {
    let opts = VcsOptions::default();
    let mut commit = gource_vcs::Commit::default();

    let mut get_line_empty = |_: &mut String| false;
    assert!(!gource_vcs::formats::git::parse_commit(
        &mut get_line_empty,
        &mut commit,
        &opts
    ));

    let mut get_line_bad = |l: &mut String| {
        *l = "not_user:foo".to_string();
        true
    };
    assert!(!gource_vcs::formats::git::parse_commit(
        &mut get_line_bad,
        &mut commit,
        &opts
    ));
}

#[test]
fn test_cvs_exp_parser_branch_and_errors() {
    let opts = VcsOptions::default();
    let mut commit = gource_vcs::Commit::default();

    let mut get_line_empty = |_: &mut String| false;
    assert!(!gource_vcs::formats::cvs_exp::parse_commit(
        &mut get_line_empty,
        &mut commit,
        &opts
    ));

    // Branch line present
    let lines = [
        "000001:",
        "BRANCH [vendor-branch]",
        "",
        "(date: 2020/01/01 12:00:00;  author: alice;  state: Exp;  lines: +1 -1)",
        "| src/file.txt,v:1.1",
        "",
        "commit message",
        "=============================================================================",
    ];
    let mut idx = 0;
    let mut get_line = |l: &mut String| {
        if idx < lines.len() {
            *l = lines[idx].to_string();
            idx += 1;
            true
        } else {
            false
        }
    };
    assert!(gource_vcs::formats::cvs_exp::parse_commit(
        &mut get_line,
        &mut commit,
        &opts
    ));
    assert_eq!(commit.username, "alice");
}

#[test]
fn test_svn_parser_dir_and_errors() {
    let opts = VcsOptions::default();
    let mut commit = gource_vcs::Commit::default();

    let mut get_line_empty = |_: &mut String| false;
    assert!(!gource_vcs::formats::svn::parse_commit(
        &mut get_line_empty,
        &mut commit,
        &opts
    ));

    // Svn directory deletion
    let svn_xml = r#"<logentry revision="1">
<date>2020-01-01T12:00:00.000000Z</date>
<paths>
<path kind="dir" action="D">/my/dir</path>
<path kind="file" action="M">/my/file.txt</path>
</paths>
</logentry>
"#;
    let lines: Vec<&str> = svn_xml.lines().collect();
    let mut idx = 0;
    let mut get_line = |l: &mut String| {
        if idx < lines.len() {
            *l = lines[idx].to_string();
            idx += 1;
            true
        } else {
            false
        }
    };
    assert!(gource_vcs::formats::svn::parse_commit(
        &mut get_line,
        &mut commit,
        &opts
    ));
    assert_eq!(commit.username, "");
    assert_eq!(commit.files[0].filename, "/my/dir/");
    assert_eq!(commit.files[0].action, FileAction::Delete);
}

#[test]
fn test_cvs2cl_parser_empty_author_and_errors() {
    let opts = VcsOptions::default();
    let mut commit = gource_vcs::Commit::default();

    let mut get_line_empty = |_: &mut String| false;
    assert!(!gource_vcs::formats::cvs2cl::parse_commit(
        &mut get_line_empty,
        &mut commit,
        &opts
    ));

    let xml = r#"<entry>
<date>2020-01-01</date>
<time>12:00:00</time>
<isoDate>2020-01-01T12:00:00Z</isoDate>
<file>
<name>file.txt</name>
<cvsstate>Exp</cvsstate>
</file>
</entry>
"#;
    let lines: Vec<&str> = xml.lines().collect();
    let mut idx = 0;
    let mut get_line = |l: &mut String| {
        if idx < lines.len() {
            *l = lines[idx].to_string();
            idx += 1;
            true
        } else {
            false
        }
    };
    assert!(gource_vcs::formats::cvs2cl::parse_commit(
        &mut get_line,
        &mut commit,
        &opts
    ));
    assert_eq!(commit.username, "Unknown");
}

#[test]
fn test_logmill_errors_and_abort() {
    let dir = tempdir().unwrap();
    let opts = VcsOptions::default();

    // Directory without repo should return "directory not supported"
    let res = LogMill::fetch_blocking(dir.path().to_str().unwrap(), &opts);
    assert_eq!(res.err(), Some("directory not supported".to_string()));

    // Directory with forced unknown format
    let mut forced_opts = VcsOptions::default();
    forced_opts.log_format = "git".to_string();
    let res2 = LogMill::fetch_blocking(dir.path().to_str().unwrap(), &forced_opts);
    assert_eq!(res2.err(), Some("failed to generate log file".to_string()));

    // Directory with forced format and timestamps
    let mut forced_time_opts = VcsOptions::default();
    forced_time_opts.log_format = "git".to_string();
    forced_time_opts.start_timestamp = 1000;
    let res3 = LogMill::fetch_blocking(dir.path().to_str().unwrap(), &forced_time_opts);
    assert_eq!(
        res3.err(),
        Some("failed to generate log file for the specified time period".to_string())
    );

    // Non-repo unsupported file
    let temp_file = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(temp_file.path(), "unsupported junk content\n").unwrap();
    let res4 = LogMill::fetch_blocking(temp_file.path().to_str().unwrap(), &opts);
    assert_eq!(
        res4.err(),
        Some("unsupported log format (you may need to regenerate your log file)".to_string())
    );

    // Test aborting LogMill
    let mut mill = LogMill::spawn(dir.path().to_str().unwrap(), opts);
    mill.abort();
    // After abort, take_result returns None or Err
    assert!(mill.take_result().is_some());
}

#[test]
fn test_logmill_start_timestamp_filtering() {
    let repo_dir = tempdir().unwrap();
    let repo_path = repo_dir.path();

    assert!(
        Command::new("git")
            .arg("init")
            .current_dir(repo_path)
            .status()
            .unwrap()
            .success()
    );
    assert!(
        Command::new("git")
            .args(["config", "user.name", "Tester"])
            .current_dir(repo_path)
            .status()
            .unwrap()
            .success()
    );
    assert!(
        Command::new("git")
            .args(["config", "user.email", "test@example.com"])
            .current_dir(repo_path)
            .status()
            .unwrap()
            .success()
    );

    std::fs::write(repo_path.join("f1.txt"), "1").unwrap();
    assert!(
        Command::new("git")
            .args(["add", "f1.txt"])
            .current_dir(repo_path)
            .status()
            .unwrap()
            .success()
    );
    assert!(
        Command::new("git")
            .args(["commit", "-m", "1"])
            .current_dir(repo_path)
            .env("GIT_AUTHOR_DATE", "1500000000 +0000")
            .env("GIT_COMMITTER_DATE", "1500000000 +0000")
            .status()
            .unwrap()
            .success()
    );

    std::fs::write(repo_path.join("f2.txt"), "2").unwrap();
    assert!(
        Command::new("git")
            .args(["add", "f2.txt"])
            .current_dir(repo_path)
            .status()
            .unwrap()
            .success()
    );
    assert!(
        Command::new("git")
            .args(["commit", "-m", "2"])
            .current_dir(repo_path)
            .env("GIT_AUTHOR_DATE", "1600000000 +0000")
            .env("GIT_COMMITTER_DATE", "1600000000 +0000")
            .status()
            .unwrap()
            .success()
    );

    let mut opts = VcsOptions::default();
    opts.start_timestamp = 1550000000;
    let mut clog = LogMill::fetch_blocking(repo_path.to_str().unwrap(), &opts).unwrap();
    let c = clog.next_commit().unwrap();
    assert_eq!(c.timestamp, 1600000000);
}

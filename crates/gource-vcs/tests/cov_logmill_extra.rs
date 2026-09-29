use gource_vcs::options::VcsOptions;
use gource_vcs::{LogMill, LogMillStatus};
use std::fs::File;
use std::io::Write;

#[test]
fn test_logmill_wait_and_take_result_lifecycle() {
    let temp_dir = tempfile::tempdir().unwrap();
    let log_path = temp_dir.path().join("test.log");
    {
        let mut file = File::create(&log_path).unwrap();
        writeln!(file, "1577836800|Alice|A|/src/main.rs").unwrap();
        writeln!(file, "1577836860|Bob|M|/src/main.rs").unwrap();
    }

    let opts = VcsOptions {
        log_format: "custom".to_string(),
        ..VcsOptions::default()
    };

    let mut mill = LogMill::spawn(log_path.to_str().unwrap(), opts);

    // Call wait(), which blocks until the background fetch finishes
    mill.wait();

    // After wait(), is_finished() must be true
    assert!(mill.is_finished());
    assert_eq!(mill.status(), LogMillStatus::Success);

    // take_result() returns Ok(CommitLog)
    let res = mill.take_result();
    assert!(res.is_some());
    let mut commit_log = res.unwrap().expect("commit log should succeed");

    let first = commit_log.next_commit();
    assert!(first.is_some());
    assert_eq!(first.unwrap().username, "Alice");

    // Calling wait() again after take_result() returns immediately and cached_result is None
    mill.wait();
    assert!(mill.take_result().is_none());
}

#[test]
fn test_logmill_abort_with_active_child() {
    let temp_repo = tempfile::tempdir().unwrap();
    let status = std::process::Command::new("git")
        .args(["init"])
        .current_dir(temp_repo.path())
        .status();

    if let Ok(s) = status
        && s.success()
    {
        let opts = VcsOptions {
            log_format: "git".to_string(),
            ..VcsOptions::default()
        };
        let mut mill = LogMill::spawn(temp_repo.path().to_str().unwrap(), opts);
        // Abort right away, covering abort child killing and joining
        mill.abort();
        assert!(mill.is_finished());
    }
}

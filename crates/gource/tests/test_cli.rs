use std::process::Command;

#[test]
fn test_cli_help() {
    let bin = env!("CARGO_BIN_EXE_gource");
    let output = Command::new(bin)
        .arg("--help")
        .output()
        .expect("run binary");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Usage: gource [options] [path]"));
}

#[test]
fn test_cli_help_extended() {
    let bin = env!("CARGO_BIN_EXE_gource");
    let output = Command::new(bin).arg("-H").output().expect("run binary");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Usage: gource [options] [path]"));
}

#[test]
fn test_cli_log_command_git() {
    let bin = env!("CARGO_BIN_EXE_gource");
    let output = Command::new(bin)
        .args(["--log-command", "git"])
        .output()
        .expect("run binary");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.starts_with("git log"));
}

#[test]
fn test_cli_invalid_option() {
    let bin = env!("CARGO_BIN_EXE_gource");
    let output = Command::new(bin)
        .arg("--invalid-flag-that-does-not-exist")
        .output()
        .expect("run binary");
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.starts_with("gource: unknown option"));
    assert!(stderr.contains("Try 'gource --help'"));
}

#[test]
fn test_cli_bad_ppm_path() {
    let bin = env!("CARGO_BIN_EXE_gource");
    let output = Command::new(bin)
        .args(["--output-ppm-stream", "/nonexistent_dir_xyz/bad.ppm", "."])
        .output()
        .expect("run binary");
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("could not write to '/nonexistent_dir_xyz/bad.ppm'"));
}

#[test]
fn test_cli_custom_log_generation() {
    // The workspace root is this repository's git checkout.
    let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    if !repo.join(".git").exists() {
        eprintln!("Skipping: not running from a git checkout");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let log_path = dir.path().join("custom.log");
    let bin = env!("CARGO_BIN_EXE_gource");
    let output = Command::new(bin)
        .args([
            "--output-custom-log",
            log_path.to_str().unwrap(),
            repo.to_str().unwrap(),
        ])
        .output()
        .expect("run binary");
    assert!(output.status.success());
    assert!(log_path.exists());
}

#[test]
fn test_cli_run_and_exit_xvfb() {
    // Only run when DISPLAY is available (e.g. under xvfb-run)
    if std::env::var("DISPLAY").is_err() {
        return;
    }
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let tests_dir = format!("{manifest_dir}/tests");
    let bin = env!("CARGO_BIN_EXE_gource");
    let output = Command::new(bin)
        .args(["--stop-at-time", "0.001", "--stop-at-end", &tests_dir])
        .output()
        .expect("run binary");
    if !output.status.success() {
        panic!(
            "Failed with status {:?}:\nSTDOUT:\n{}\nSTDERR:\n{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn test_cli_recording_and_exit_xvfb() {
    // Only run when DISPLAY is available (e.g. under xvfb-run)
    if std::env::var("DISPLAY").is_err() {
        return;
    }
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let tests_dir = format!("{manifest_dir}/tests");
    let dir = tempfile::tempdir().unwrap();
    let ppm_path = dir.path().join("out.ppm");
    let bin = env!("CARGO_BIN_EXE_gource");
    let output = Command::new(bin)
        .args([
            "--output-ppm-stream",
            ppm_path.to_str().unwrap(),
            "--stop-at-time",
            "0.001",
            "--stop-at-end",
            &tests_dir,
        ])
        .output()
        .expect("run binary");
    if !output.status.success() {
        panic!(
            "Failed with status {:?}:\nSTDOUT:\n{}\nSTDERR:\n{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

//! A stdin log that ends with no usable commit fails, rather than waiting.

use std::io::Write;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

fn run(input: &str, extra: &[&str]) -> (Option<i32>, String) {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("e.ppm");
    let mut child = Command::new(env!("CARGO_BIN_EXE_gource-cli"))
        .args(["-160x120", "-o", out.to_str().unwrap()])
        .args(["--log-format", "custom"])
        .args(extra)
        .arg("-")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    stdin.write_all(input.as_bytes()).unwrap();
    drop(stdin);
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        if child.try_wait().unwrap().is_some() {
            break;
        }
        if Instant::now() > deadline {
            child.kill().unwrap();
            panic!("hangs after stdin ended");
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let o = child.wait_with_output().unwrap();
    (o.status.code(), String::from_utf8_lossy(&o.stderr).into())
}

#[test]
fn filtered_out_single_commit() {
    let (code, err) = run("1|a|A|/x\n", &["--user-filter", "a"]);
    assert_eq!(code, Some(1), "{err}");
    assert!(err.contains("no commits found"), "{err}");
}

#[test]
fn filtered_out_two_commits() {
    let (code, err) = run("1|a|A|/x\n2|b|A|/y\n", &["--user-filter", ".*"]);
    assert_eq!(code, Some(1), "{err}");
    assert!(err.contains("no commits found"), "{err}");
}

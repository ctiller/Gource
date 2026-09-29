//! Parity with the C++ binary. Every fixture under `tests/data/parity/` was
//! converted with the C++ `gource --output-custom-log` (see
//! `tests/tools/gen_parity.sh` and `tests/tools/gen_filter_cases.sh`); the Rust
//! conversion must produce the same output, and fail where C++ fails.

use gource_vcs::{VcsOptions, write_custom_log};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The goldens were generated with `TZ=America/New_York`, and formats with
/// local times (bzr, cvs, ...) depend on it. Re-run the calling test in a
/// child process with that time zone. Returns true in the parent, after
/// checking the child's result, and false in the child, which runs the test.
fn run_in_new_york(test_name: &str) -> bool {
    const CHILD: &str = "GOURCE_PARITY_CHILD";
    if std::env::var_os(CHILD).is_some() {
        return false;
    }
    let status = Command::new(std::env::current_exe().unwrap())
        .args([test_name, "--exact", "--nocapture"])
        .env("TZ", "America/New_York")
        .env(CHILD, "1")
        .status()
        .unwrap();
    assert!(
        status.success(),
        "{test_name} failed in the TZ=America/New_York child process"
    );
    true
}

fn parity_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/parity")
}

/// Convert `log` like `gource --output-custom-log`; `None` if that fails.
fn convert(log: &Path, opts: &VcsOptions) -> Option<String> {
    let out = tempfile::NamedTempFile::new().unwrap();
    write_custom_log(log.to_str().unwrap(), out.path().to_str().unwrap(), opts).ok()?;
    Some(fs::read_to_string(out.path()).unwrap())
}

#[test]
fn test_all_parity_fixtures() {
    if run_in_new_york("test_all_parity_fixtures") {
        return;
    }
    let formats = [
        "apache", "bzr", "custom", "cvs2cl", "cvs_exp", "git", "gitraw", "hg", "svn",
    ];
    let mut failures = Vec::new();
    let mut checked = 0;
    for format in formats {
        // As in gen_parity.sh: gource detects cvs-exp logs, and there is no
        // gitraw log-format value (git falls back to it).
        let log_format = match format {
            "cvs_exp" => "",
            "gitraw" => "git",
            _ => format,
        };
        let opts = VcsOptions {
            log_format: log_format.to_string(),
            ..VcsOptions::default()
        };
        let mut logs: Vec<PathBuf> = fs::read_dir(parity_dir().join(format))
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "log"))
            .collect();
        logs.sort();
        for log in logs {
            let exit: i32 = fs::read_to_string(log.with_extension("expected_exit"))
                .unwrap()
                .trim()
                .parse()
                .unwrap();
            let expected = fs::read_to_string(log.with_extension("expected")).ok();
            let actual = convert(&log, &opts);
            let ok = if exit == 0 {
                actual.is_some() && actual == expected
            } else {
                actual.is_none()
            };
            if !ok {
                failures.push(format!(
                    "{format}/{}: C++ exit {exit}, output {expected:?}; Rust output {actual:?}",
                    log.file_name().unwrap().to_string_lossy()
                ));
            }
            checked += 1;
        }
    }
    assert!(checked >= 80, "only {checked} fixtures found");
    assert!(
        failures.is_empty(),
        "{} of {checked} fixtures differ from C++:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn test_parity_filters() {
    if run_in_new_york("test_parity_filters") {
        return;
    }
    let dir = parity_dir().join("filters");
    let regex = |re: &str| vec![fancy_regex::Regex::new(re).unwrap()];
    let custom = VcsOptions {
        log_format: "custom".to_string(),
        ..VcsOptions::default()
    };
    // The cases of gen_filter_cases.sh.
    let mut cases = vec![
        ("file_filter", custom.clone()),
        ("file_show_filter", custom.clone()),
        ("user_filter", custom.clone()),
        ("user_show_filter", custom.clone()),
    ];
    cases[0].1.filters.file_filters = regex(r"\.md$");
    cases[1].1.filters.file_show_filters = regex(r"\.rs$");
    cases[2].1.filters.user_filters = regex(r"^Bob$");
    cases[3].1.filters.user_show_filters = regex(r"^Alice$");
    for (name, opts) in cases {
        let expected = fs::read_to_string(dir.join(format!("{name}.expected"))).unwrap();
        let actual = convert(&dir.join("input.log"), &opts);
        assert_eq!(actual, Some(expected), "{name}");
    }
}

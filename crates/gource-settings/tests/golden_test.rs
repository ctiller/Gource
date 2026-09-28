//! Golden test suite comparing Rust gource-settings against C++ Gource reference outputs.

use gource_settings::{CliAction, conffile::ConfFile, parse_command_line};
use std::fs;
use std::path::Path;

#[test]
fn test_golden_invalid_invocations() {
    let invalid_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/invalid");
    assert!(
        invalid_dir.exists(),
        "tests/data/invalid directory must exist"
    );

    let mut entries: Vec<_> = fs::read_dir(&invalid_dir)
        .expect("read_dir")
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "json"))
        .collect();
    entries.sort_by_key(|e| e.path());

    assert!(
        entries.len() >= 50,
        "Expected at least 50 invalid test cases, found {}",
        entries.len()
    );

    let mut tested_count = 0;
    for entry in entries {
        let content = fs::read_to_string(entry.path()).expect("read json");
        // Simple manual JSON parse for { "args": "...", "reason": "..." }
        let args_str = extract_json_field(&content, "args").expect("args field");
        let expected_reason = extract_json_field(&content, "reason").expect("reason field");

        let args: Vec<String> = if args_str.is_empty() {
            Vec::new()
        } else {
            args_str.split_whitespace().map(|s| s.to_string()).collect()
        };

        let result = parse_command_line(&args);
        assert!(
            result.is_err(),
            "Expected failure for case {:?} with args {:?}, but got {:?}",
            entry.file_name(),
            args,
            result
        );

        let actual_reason = result.unwrap_err().0;
        assert_eq!(
            actual_reason,
            expected_reason,
            "Mismatch in case {:?} with args {:?}",
            entry.file_name(),
            args
        );

        tested_count += 1;
    }

    println!("Verified {tested_count} invalid golden cases against C++ outputs.");
}

#[test]
fn test_golden_save_config() {
    let save_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/save_config");
    assert!(
        save_dir.exists(),
        "tests/data/save_config directory must exist"
    );

    let mut entries: Vec<_> = fs::read_dir(&save_dir)
        .expect("read_dir")
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "conf"))
        .collect();
    entries.sort_by_key(|e| e.path());

    assert!(
        entries.len() >= 20,
        "Expected at least 20 valid save_config test cases, found {}",
        entries.len()
    );

    let mut tested_count = 0;
    for entry in entries {
        let conf_path = entry.path();
        let args_path = conf_path.with_extension("args");
        assert!(
            args_path.exists(),
            "Args file must exist: {}",
            args_path.display()
        );

        let expected_conf_text = fs::read_to_string(&conf_path).expect("read conf");
        let args_content = fs::read_to_string(&args_path).expect("read args");
        let mut args: Vec<String> = args_content
            .split_whitespace()
            .map(|s| s.to_string())
            .collect();

        // Append --save-config dummy.conf
        args.push("--save-config".to_string());
        args.push("/tmp/dummy.conf".to_string());

        let action = parse_command_line(&args).unwrap_or_else(|e| {
            panic!(
                "Failed to parse valid case {:?} with args {:?}: {e}",
                entry.file_name(),
                args
            )
        });

        match action {
            CliAction::SaveConfig { config, .. } => {
                let actual_conf_text = config.conf.to_text();
                assert_eq!(
                    actual_conf_text,
                    expected_conf_text,
                    "Conf serialization mismatch in case {:?}\n--- Expected (C++) ---\n{}\n--- Actual (Rust) ---\n{}",
                    entry.file_name(),
                    expected_conf_text,
                    actual_conf_text
                );
            }
            other => {
                panic!(
                    "Expected CliAction::SaveConfig for {:?}, but got {:?}",
                    entry.file_name(),
                    other
                );
            }
        }

        tested_count += 1;
    }

    println!("Verified {tested_count} valid save_config golden cases against C++ outputs.");
}

#[test]
fn test_conf_file_roundtrip_golden() {
    let save_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/save_config");
    for entry in fs::read_dir(&save_dir).unwrap().filter_map(|e| e.ok()) {
        if entry.path().extension().is_some_and(|ext| ext == "conf") {
            let original = fs::read_to_string(entry.path()).unwrap();
            let parsed = ConfFile::parse(&original, "test.conf").unwrap();
            let serialized = parsed.to_text();
            assert_eq!(
                serialized,
                original,
                "Roundtrip failed for {}",
                entry.path().display()
            );
        }
    }
}

fn extract_json_field(json: &str, field: &str) -> Option<String> {
    let key = format!("\"{field}\":");
    let pos = json.find(&key)?;
    let after_key = json[pos + key.len()..].trim_start();
    if let Some(rest) = after_key.strip_prefix('"') {
        let mut result = String::new();
        let mut chars = rest.chars();
        while let Some(c) = chars.next() {
            if c == '\\' {
                if let Some(escaped) = chars.next() {
                    match escaped {
                        '"' => result.push('"'),
                        '\\' => result.push('\\'),
                        'n' => result.push('\n'),
                        'r' => result.push('\r'),
                        't' => result.push('\t'),
                        other => {
                            result.push('\\');
                            result.push(other);
                        }
                    }
                }
            } else if c == '"' {
                return Some(result);
            } else {
                result.push(c);
            }
        }
    }
    None
}

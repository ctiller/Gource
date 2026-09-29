//! Robustness: mutated logs must never panic a parser. The corpus is every
//! parity fixture line; mutations splice in multi-byte characters and format
//! delimiters at random character boundaries, truncate and shuffle lines.

use gource_vcs::{VcsOptions, write_custom_log};
use std::fs;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;

const FORMATS: [&str; 9] = [
    "git", "gitraw", "svn", "hg", "bzr", "cvs2cl", "cvs_exp", "apache", "custom",
];
/// Override with `GOURCE_FUZZ_CASES` for a longer local run.
const CASES_PER_FORMAT: usize = 300;
const TOKENS: [&str; 18] = [
    "\u{e9}",
    "\u{65e5}\u{672c}",
    "\u{1f600}",
    "\t",
    "|",
    ":",
    "\"",
    "<",
    ">",
    "&",
    "0",
    "9999",
    "\n",
    " ",
    "-",
    "/",
    ";",
    "\u{feff}",
];

/// xorshift64*: deterministic, so failures reproduce.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

fn char_boundary(rng: &mut Rng, s: &str) -> usize {
    let boundaries: Vec<usize> = s.char_indices().map(|(i, _)| i).chain([s.len()]).collect();
    boundaries[rng.below(boundaries.len())]
}

fn mutate(rng: &mut Rng, line: &str) -> String {
    let mut line = line.to_string();
    for _ in 0..=rng.below(3) {
        let at = char_boundary(rng, &line);
        match rng.below(4) {
            0 | 1 => line.insert_str(at, TOKENS[rng.below(TOKENS.len())]),
            2 => line.truncate(at),
            _ => {
                if let Some(c) = line[at..].chars().next() {
                    line.replace_range(at..at + c.len_utf8(), "");
                }
            }
        }
    }
    line
}

/// Fixture lines per format, in `FORMATS` order.
fn corpus(dir: &Path) -> Vec<Vec<String>> {
    FORMATS
        .iter()
        .map(|format| {
            let mut lines = Vec::new();
            for entry in fs::read_dir(dir.join(format)).unwrap() {
                let path = entry.unwrap().path();
                if path.extension().is_some_and(|e| e == "log") {
                    let bytes = fs::read(&path).unwrap();
                    lines.extend(String::from_utf8_lossy(&bytes).lines().map(str::to_string));
                }
            }
            lines
        })
        .collect()
}

#[test]
fn mutated_logs_do_not_panic() {
    let parity = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/parity");
    let corpus = corpus(&parity);
    let tmp = tempfile::tempdir().unwrap();
    let log = tmp.path().join("input.log");
    let out = tmp.path().join("output.log");
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    let cases = std::env::var("GOURCE_FUZZ_CASES")
        .ok()
        .and_then(|n| n.parse().ok())
        .unwrap_or(CASES_PER_FORMAT);
    let mut failures = Vec::new();
    for (index, format) in FORMATS.iter().copied().enumerate() {
        // There is no gitraw --log-format: git falls back to it.
        let log_format = match format {
            "gitraw" => "git",
            "cvs_exp" => "cvs-exp",
            other => other,
        };
        let options = VcsOptions {
            log_format: log_format.to_string(),
            ..VcsOptions::default()
        };
        for case in 0..cases {
            let mut text = String::new();
            for _ in 0..1 + rng.below(40) {
                // Mostly lines of this format, some from the others.
                let lines = if rng.below(5) == 0 {
                    &corpus[rng.below(corpus.len())]
                } else {
                    &corpus[index]
                };
                let line = &lines[rng.below(lines.len())];
                if rng.below(3) == 0 {
                    text.push_str(&mutate(&mut rng, line));
                } else {
                    text.push_str(line);
                }
                text.push('\n');
            }
            fs::write(&log, &text).unwrap();
            let result = catch_unwind(AssertUnwindSafe(|| {
                let _ = write_custom_log(log.to_str().unwrap(), out.to_str().unwrap(), &options);
            }));
            if result.is_err() {
                failures.push(format!("--- {format} case {case}:\n{text}"));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} inputs panicked:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

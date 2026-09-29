//! Reading log files line by line, checked against the C++ binary's
//! `--output-custom-log` output for the same inputs.

use gource_vcs::log::{CommitLog, StreamLog};
use gource_vcs::{VcsOptions, write_custom_log};
use std::io::{Cursor, Read};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc::{Receiver, Sender, channel};

/// Write `input` to a file, convert it with `--output-custom-log` semantics
/// and return the output.
fn custom_log_output(input: &[u8]) -> String {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("in.log");
    let out = dir.path().join("out.log");
    std::fs::write(&log, input).unwrap();
    let opts = VcsOptions {
        log_format: "custom".to_string(),
        ..VcsOptions::default()
    };
    write_custom_log(log.to_str().unwrap(), out.to_str().unwrap(), &opts).unwrap();
    std::fs::read_to_string(out).unwrap()
}

#[test]
fn last_commit_is_written() {
    // The log is only finished once a read reaches the end of the file, not
    // when the line that starts the last commit has been read.
    assert_eq!(
        custom_log_output(b"100|Alice|A|/a.txt\n100|Alice|M|/b.txt\n200|Bob|A|/c.txt\n"),
        "100|Alice|A|/a.txt\n100|Alice|M|/b.txt\n200|Bob|A|/c.txt\n"
    );
}

#[test]
fn last_line_without_newline_ends_the_log_like_cpp() {
    // C++ quirk: reading a final line without a newline sets eofbit, so the
    // commit that line starts is never processed.
    assert_eq!(
        custom_log_output(b"100|Alice|A|/a.txt\n100|Alice|M|/b.txt\n200|Bob|A|/c.txt"),
        "100|Alice|A|/a.txt\n100|Alice|M|/b.txt\n"
    );
    assert_eq!(
        custom_log_output(b"100|Alice|A|/a.txt\r\n200|Bob|A|/c.txt\r"),
        "100|Alice|A|/a.txt\n"
    );
}

#[test]
fn invalid_utf8_is_replaced_and_reading_continues() {
    assert_eq!(
        custom_log_output(b"100|Alice|A|/a.txt\n200|B\xe9b|A|/c\xff.txt\n300|Carol|A|/d.txt\n"),
        "100|Alice|A|/a.txt\n200|B?b|A|/c?.txt\n300|Carol|A|/d.txt\n"
    );
}

/// A stream whose input is written by the test, chunk by chunk.
struct ChunkReader {
    chunks: Receiver<Vec<u8>>,
    chunk: Cursor<Vec<u8>>,
}

impl Read for ChunkReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        loop {
            let n = self.chunk.read(buf)?;
            if n > 0 {
                return Ok(n);
            }
            match self.chunks.recv() {
                Ok(chunk) => self.chunk = Cursor::new(chunk),
                // The writer has gone: end of input.
                Err(_) => return Ok(0),
            }
        }
    }
}

fn chunked_custom_stream() -> (Sender<Vec<u8>>, CommitLog) {
    let (writer, chunks) = channel();
    let reader = ChunkReader {
        chunks,
        chunk: Cursor::new(Vec::new()),
    };
    let opts = VcsOptions {
        log_format: "custom".to_string(),
        ..VcsOptions::default()
    };
    let log = CommitLog::from_stream("custom", StreamLog::from_reader(reader), opts);
    (writer, log)
}

fn files(commit: &gource_vcs::Commit) -> Vec<&str> {
    commit.files.iter().map(|f| f.filename.as_str()).collect()
}

#[test]
fn stream_commits_are_not_split_by_slow_input() {
    let (writer, mut log) = chunked_custom_stream();
    writer
        .send(b"100|Alice|A|/a.txt\n100|Alice|A|/b.txt\n".to_vec())
        .unwrap();
    // Alice's commit may continue: nothing is returned until the next
    // commit starts or the input ends, however long the writer takes.
    for _ in 0..20 {
        assert!(log.next_commit().is_none());
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    writer
        .send(b"100|Alice|A|/c.txt\n200|Bob|A|/d.txt\n".to_vec())
        .unwrap();
    drop(writer);

    log.wait_for_input(true);
    let alice = log.next_commit().unwrap();
    assert_eq!(files(&alice), ["/a.txt", "/b.txt", "/c.txt"]);
    let bob = log.next_commit().unwrap();
    assert_eq!(files(&bob), ["/d.txt"]);
    assert!(log.next_commit().is_none());
    // Like C++ `RCommitLog::isFinished`, a stream is never finished.
    assert!(!log.is_finished());
}

#[test]
fn stream_format_check_waits_for_the_first_commit() {
    let (writer, mut log) = chunked_custom_stream();
    let delayed_writer = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(50));
        writer.send(b"100|Alice|A|/a.txt\n".to_vec()).unwrap();
    });
    // The first commit is complete when the input ends.
    assert!(log.check_format());
    delayed_writer.join().unwrap();
    assert_eq!(files(&log.next_commit().unwrap()), ["/a.txt"]);
}

#[test]
fn stream_last_line_without_newline_is_dropped_like_cpp() {
    // C++ `StreamLog::getNextLine` fails on a line that ends the input.
    let input = b"100|Alice|A|/a.txt\n200|Bob|A|/b.txt".to_vec();
    let opts = VcsOptions::default();
    let mut log =
        CommitLog::from_stream("custom", StreamLog::from_reader(Cursor::new(input)), opts);
    log.wait_for_input(true);
    assert_eq!(files(&log.next_commit().unwrap()), ["/a.txt"]);
    assert!(log.next_commit().is_none());
}

/// Run `write_custom_log("-", ...)` in a child process reading `log` on
/// stdin, and return its output.
fn convert_stdin(test_name: &str, log: &Path) -> String {
    let output = Command::new(std::env::current_exe().unwrap())
        .args([test_name, "--exact", "--nocapture"])
        .env("GOURCE_STDIN_CHILD", "1")
        .stdin(std::fs::File::open(log).unwrap())
        .stderr(Stdio::inherit())
        .output()
        .unwrap();
    assert!(output.status.success(), "{test_name} child failed");
    let stdout = String::from_utf8(output.stdout).unwrap();
    // libtest prints its own lines around the test's output.
    let start = stdout.find("<<<").expect("no output markers") + 3;
    let end = stdout.rfind(">>>").expect("no output markers");
    stdout[start..end].to_string()
}

/// In the child process: convert stdin and print the result between markers.
fn child_convert_stdin(log_format: &str) -> bool {
    if std::env::var_os("GOURCE_STDIN_CHILD").is_none() {
        return false;
    }
    let out = tempfile::NamedTempFile::new().unwrap();
    let opts = VcsOptions {
        log_format: log_format.to_string(),
        ..VcsOptions::default()
    };
    write_custom_log("-", out.path().to_str().unwrap(), &opts).unwrap();
    print!("<<<{}>>>", std::fs::read_to_string(out.path()).unwrap());
    true
}

fn parity_file(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data/parity")
        .join(name)
}

#[test]
fn stdin_custom_log_is_converted() {
    if child_convert_stdin("custom") {
        return;
    }
    assert_eq!(
        convert_stdin(
            "stdin_custom_log_is_converted",
            &parity_file("custom/standard.log")
        ),
        std::fs::read_to_string(parity_file("custom/standard.expected")).unwrap()
    );
}

#[test]
fn stdin_git_falls_back_to_the_raw_format() {
    // The git format check only peeks at stdin, which is left for gitraw.
    if child_convert_stdin("git") {
        return;
    }
    assert_eq!(
        convert_stdin(
            "stdin_git_falls_back_to_the_raw_format",
            &parity_file("gitraw/standard.log")
        ),
        std::fs::read_to_string(parity_file("gitraw/standard.expected")).unwrap()
    );
}

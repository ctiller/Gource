//! Incremental commit log reading (port of `RCommitLog` in
//! formats/commitlog.cpp and `SeekLog`/`StreamLog` in core/seeklog.cpp).

use crate::commit::Commit;
use crate::formats;
use crate::options::VcsOptions;
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::thread;
use tempfile::NamedTempFile;

enum LogSource {
    Seekable(SeekableLog),
    Stream(StreamLog),
}

pub(crate) struct SeekableLog {
    reader: BufReader<File>,
    file_size: u64,
    current_pos: u64,
    finished: bool,
    _temp_file: Option<NamedTempFile>,
}

impl SeekableLog {
    pub(crate) fn new(file: File, temp_file: Option<NamedTempFile>) -> std::io::Result<Self> {
        let file_size = file.metadata()?.len();
        let reader = BufReader::new(file);
        Ok(Self {
            reader,
            file_size,
            current_pos: 0,
            finished: false,
            _temp_file: temp_file,
        })
    }

    fn seek_to(&mut self, percent: f32) {
        let clamped = percent.clamp(0.0, 1.0);
        let offset = (clamped as f64 * self.file_size as f64) as u64;
        self.set_pointer(offset);
        if offset != 0 {
            // Discard the remainder of the line to sync to a newline
            let mut line = String::new();
            let _ = self.get_next_line(&mut line);
        }
    }

    fn get_pointer(&self) -> u64 {
        self.current_pos
    }

    fn set_pointer(&mut self, pointer: u64) {
        let actual = pointer.min(self.file_size);
        let _ = self.reader.seek(SeekFrom::Start(actual));
        self.current_pos = actual;
        self.finished = self.current_pos >= self.file_size;
    }

    pub(crate) fn get_percent(&self) -> f32 {
        if self.file_size == 0 {
            0.0
        } else {
            (self.current_pos as f32 / self.file_size as f32).clamp(0.0, 1.0)
        }
    }

    pub(crate) fn get_next_line(&mut self, line: &mut String) -> bool {
        line.clear();
        match self.reader.read_line(line) {
            Ok(0) => {
                self.finished = true;
                false
            }
            Ok(bytes) => {
                self.current_pos += bytes as u64;
                if line.ends_with('\n') {
                    line.pop();
                    if line.ends_with('\r') {
                        line.pop();
                    }
                }
                true
            }
            Err(_) => {
                self.finished = true;
                false
            }
        }
    }

    pub(crate) fn is_finished(&self) -> bool {
        self.finished || self.current_pos >= self.file_size
    }
}

pub struct StreamLog {
    receiver: Receiver<String>,
    finished: bool,
}

impl StreamLog {
    fn new_stdin() -> Self {
        Self::from_reader(std::io::stdin())
    }

    pub fn from_reader<R: Read + Send + 'static>(reader: R) -> Self {
        let (tx, rx) = channel::<String>();
        thread::Builder::new()
            .name("stream-log-reader".to_string())
            .spawn(move || {
                let mut buf_reader = BufReader::new(reader);
                let mut line = String::new();
                loop {
                    line.clear();
                    match buf_reader.read_line(&mut line) {
                        Ok(0) => break,
                        Ok(_) => {
                            if line.ends_with('\n') {
                                line.pop();
                                if line.ends_with('\r') {
                                    line.pop();
                                }
                            }
                            if tx.send(line.clone()).is_err() {
                                break;
                            }
                        }
                        Err(_) => break,
                    }
                }
            })
            .expect("failed to spawn stream reader thread");

        Self {
            receiver: rx,
            finished: false,
        }
    }

    fn get_next_line(&mut self, line: &mut String) -> bool {
        line.clear();
        if self.finished {
            return false;
        }
        match self.receiver.try_recv() {
            Ok(l) => {
                *line = l;
                true
            }
            Err(TryRecvError::Empty) => false,
            Err(TryRecvError::Disconnected) => {
                self.finished = true;
                false
            }
        }
    }

    pub(crate) fn is_finished(&self) -> bool {
        self.finished
    }
}

/// Reads commits one at a time from a log source using one log format.
pub struct CommitLog {
    format_name: String,
    log_command: Option<String>,
    source: LogSource,
    options: VcsOptions,
    last_line: Option<String>,
    buffered_commit: Option<Commit>,
}

impl CommitLog {
    /// Create a commit log from a SeekableLog or StreamLog.
    pub(crate) fn from_seekable(
        format_name: &str,
        log_command: Option<String>,
        seekable: SeekableLog,
        options: VcsOptions,
    ) -> Self {
        Self {
            format_name: format_name.to_string(),
            log_command,
            source: LogSource::Seekable(seekable),
            options,
            last_line: None,
            buffered_commit: None,
        }
    }

    pub fn from_stream(format_name: &str, stream: StreamLog, options: VcsOptions) -> Self {
        Self {
            format_name: format_name.to_string(),
            log_command: None,
            source: LogSource::Stream(stream),
            options,
            last_line: None,
            buffered_commit: None,
        }
    }

    /// Name of the log format ("git", "custom", ...).
    pub fn format_name(&self) -> &str {
        &self.format_name
    }

    /// The command used to generate the log, if it was generated from a
    /// repository.
    pub fn log_command(&self) -> Option<&str> {
        self.log_command.as_deref()
    }

    /// Low-level parse of the next commit according to `format_name`.
    fn parse_commit(&mut self, commit: &mut Commit) -> bool {
        let last_line = &mut self.last_line;
        let source = &mut self.source;
        let mut get_line = |line: &mut String| -> bool {
            if let Some(prev) = last_line.take() {
                *line = prev;
                return true;
            }
            match source {
                LogSource::Seekable(s) => s.get_next_line(line),
                LogSource::Stream(s) => s.get_next_line(line),
            }
        };

        match self.format_name.as_str() {
            "custom" => {
                let mut line = String::new();
                while get_line(&mut line) {
                    match formats::custom::CustomParser::parse_commit_entry(
                        &line,
                        commit,
                        &self.options,
                    ) {
                        Ok(true) => {}
                        Ok(false) => {
                            *last_line = Some(line);
                            break;
                        }
                        // C++ parseCommitEntry returns false: the bad line is
                        // dropped and the commit so far is kept.
                        Err(()) => break,
                    }
                }
                !commit.files.is_empty()
            }
            "hg" => {
                let mut line = String::new();
                while get_line(&mut line) {
                    match formats::hg::HgParser::parse_commit_entry(&line, commit, &self.options) {
                        Ok(true) => {}
                        Ok(false) => {
                            *last_line = Some(line);
                            break;
                        }
                        // C++ parseCommitEntry returns false: the bad line is
                        // dropped and the commit so far is kept.
                        Err(()) => break,
                    }
                }
                !commit.files.is_empty()
            }
            "git" => formats::git::parse_commit(&mut get_line, commit, &self.options),
            "gitraw" => formats::gitraw::parse_commit(&mut get_line, commit, &self.options),
            "bzr" => formats::bzr::parse_commit(&mut get_line, commit, &self.options),
            "svn" => formats::svn::parse_commit(&mut get_line, commit, &self.options),
            "cvs" | "cvs-exp" => {
                formats::cvs_exp::parse_commit(&mut get_line, commit, &self.options)
            }
            "cvs2cl" => formats::cvs2cl::parse_commit(&mut get_line, commit, &self.options),
            "apache" => formats::apache::parse_commit(&mut get_line, commit, &self.options),
            _ => false,
        }
    }

    /// `nextCommit(commit, validate = true)`: the next commit that passes the
    /// user filters and has at least one file (`RCommit::isValid`). Returns
    /// `None` if no commit is available right now (end of log, or no new data
    /// yet on a stream).
    pub fn next_commit(&mut self) -> Option<Commit> {
        self.next_commit_internal(true)
    }

    /// `nextCommit(commit, validate = false)`.
    pub fn next_commit_unvalidated(&mut self) -> Option<Commit> {
        self.next_commit_internal(false)
    }

    fn next_commit_internal(&mut self, validate: bool) -> Option<Commit> {
        if let Some(c) = self.buffered_commit.take()
            && (!validate || c.is_valid(&self.options))
        {
            return Some(c);
        }

        loop {
            let mut commit = Commit::default();
            if !self.parse_commit(&mut commit) {
                return None;
            }
            commit.postprocess();
            if !validate || commit.is_valid(&self.options) {
                return Some(commit);
            }
        }
    }

    /// Push a commit back so the next call to `next_commit` returns it
    /// (`bufferCommit`).
    pub fn buffer_commit(&mut self, commit: Commit) {
        self.buffered_commit = Some(commit);
    }

    pub fn has_buffered_commit(&self) -> bool {
        self.buffered_commit.is_some()
    }

    /// True once the whole log has been read (and nothing is buffered).
    pub fn is_finished(&self) -> bool {
        if self.buffered_commit.is_some() {
            return false;
        }
        match &self.source {
            LogSource::Seekable(s) => s.is_finished(),
            LogSource::Stream(s) => s.is_finished(),
        }
    }

    pub fn is_seekable(&self) -> bool {
        matches!(&self.source, LogSource::Seekable(_))
    }

    /// Read position as a fraction of the file size (`getPercent`).
    pub fn percent(&self) -> f32 {
        match &self.source {
            LogSource::Seekable(s) => s.get_percent(),
            LogSource::Stream(_) => 0.0,
        }
    }

    /// Seek to a fraction of the file (`seekTo`), then resynchronise to the
    /// next commit boundary as the C++ code does.
    pub fn seek_to(&mut self, percent: f32) {
        if let LogSource::Seekable(s) = &mut self.source {
            self.last_line = None;
            self.buffered_commit = None;
            s.seek_to(percent);
        }
    }

    /// `getCommitAt`: seek to `percent`, read the next commit, then restore the
    /// previous read position. Used for the slider hover date.
    pub fn commit_at(&mut self, percent: f32) -> Option<Commit> {
        let (saved_pointer, saved_last_line, saved_buffered) = match &mut self.source {
            LogSource::Seekable(s) => (
                s.get_pointer(),
                self.last_line.clone(),
                self.buffered_commit.clone(),
            ),
            LogSource::Stream(_) => return None,
        };

        self.seek_to(percent);

        // findNextCommit with attempts = 500
        let mut found = None;
        for _ in 0..500 {
            if let Some(c) = self.next_commit() {
                found = Some(c);
                break;
            }
        }

        // Restore
        if let LogSource::Seekable(s) = &mut self.source {
            s.set_pointer(saved_pointer);
            self.last_line = saved_last_line;
            self.buffered_commit = saved_buffered;
        }

        found
    }

    /// Change the seed used for extension colours of subsequently parsed
    /// commits (the `s` key recolours everything).
    pub fn set_hash_seed(&mut self, seed: i32) {
        self.options.hasher = gource_core::StringHasher::new(seed);
    }

    /// Check format implementation: read one commit without validation.
    /// If successful: seek back to 0.0 if seekable, or buffer the commit if stream.
    pub fn check_format(&mut self) -> bool {
        if let Some(commit) = self.next_commit_unvalidated() {
            match &mut self.source {
                LogSource::Seekable(s) => {
                    s.seek_to(0.0);
                    self.last_line = None;
                    self.buffered_commit = None;
                }
                LogSource::Stream(_) => {
                    self.buffered_commit = Some(commit);
                }
            }
            true
        } else {
            false
        }
    }

    /// Check first char of file or stdin if specified.
    fn check_first_char(path: &str, expected: Option<char>) -> bool {
        let exp = match expected {
            Some(c) => c,
            None => return true,
        };

        if path == "-" {
            // Can't easily peek stdin without consuming, but we can peek 1 byte if needed.
            // In C++, checkFirstChar peeks std::cin.
            return true;
        }

        let mut file = match File::open(path) {
            Ok(f) => f,
            Err(_) => return false,
        };
        let mut buf = [0u8; 1];
        if file.read_exact(&mut buf).is_ok() {
            buf[0] as char == exp
        } else {
            false
        }
    }

    /// Open a log file (or `-` for stdin) with a specific format parser, and
    /// check that the first commit parses (`checkFormat`). Returns `None` if
    /// the format does not match.
    pub fn open_file(path: &str, format: &str, options: &VcsOptions) -> Option<CommitLog> {
        let expected_char = match format {
            "git" => Some('u'),
            "gitraw" => Some('c'),
            "svn" | "cvs2cl" => Some('<'),
            _ => None,
        };

        if !Self::check_first_char(path, expected_char) {
            return None;
        }

        let mut clog = if path == "-" {
            let stream = StreamLog::new_stdin();
            CommitLog::from_stream(format, stream, options.clone())
        } else {
            let file = match File::open(path) {
                Ok(f) => f,
                Err(_) => return None,
            };
            let seekable = match SeekableLog::new(file, None) {
                Ok(s) => s,
                Err(_) => return None,
            };
            CommitLog::from_seekable(format, None, seekable, options.clone())
        };

        if clog.check_format() {
            Some(clog)
        } else {
            None
        }
    }
}

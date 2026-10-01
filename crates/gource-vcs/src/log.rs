//! Incremental commit log reading (port of `RCommitLog` in
//! formats/commitlog.cpp and `SeekLog`/`StreamLog` in core/seeklog.cpp).

use crate::commit::Commit;
use crate::commit::CommitExt;
use crate::formats;
use crate::options::VcsOptions;
use std::collections::VecDeque;
use std::fs::File;
use std::io::{BufRead, BufReader, ErrorKind, Read, Seek, SeekFrom};
use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use tempfile::NamedTempFile;

enum LogSource {
    Seekable(SeekableLog),
    Stream(StreamLog),
    /// Already-parsed commits pushed by a remote decoder.
    Feed(CommitFeed),
}

#[derive(Default)]
struct FeedState {
    queue: VecDeque<Commit>,
    ended: bool,
}

/// A queue of parsed commits shared between a producer (e.g. the wire
/// decoder of a remote stream) and a [`CommitLog`] reading from it.
#[derive(Clone, Default)]
pub struct CommitFeed {
    inner: Arc<Mutex<FeedState>>,
}

impl CommitFeed {
    pub fn new() -> Self {
        Self::default()
    }

    /// Queue a commit (already filtered and post-processed).
    pub fn push(&self, commit: Commit) {
        self.state().queue.push_back(commit);
    }

    /// No more commits will be pushed.
    pub fn end(&self) {
        self.state().ended = true;
    }

    /// Commits queued but not yet read.
    pub fn len(&self) -> usize {
        self.state().queue.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn pop(&self) -> Option<Commit> {
        self.state().queue.pop_front()
    }

    fn is_ended(&self) -> bool {
        let s = self.state();
        s.ended && s.queue.is_empty()
    }

    fn state(&self) -> std::sync::MutexGuard<'_, FeedState> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl LogSource {
    fn begin_parse(&mut self) {
        if let LogSource::Stream(s) = self {
            s.begin_parse();
        }
    }

    /// False if a stream parse ran out of input and was undone.
    fn end_parse(&mut self) -> bool {
        match self {
            LogSource::Seekable(_) | LogSource::Feed(_) => true,
            LogSource::Stream(s) => s.end_parse(),
        }
    }
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
        // `stream->clear()`: seeking resets the end-of-file state.
        self.finished = false;
    }

    pub(crate) fn get_percent(&self) -> f32 {
        if self.file_size == 0 {
            0.0
        } else {
            (self.current_pos as f32 / self.file_size as f32).clamp(0.0, 1.0)
        }
    }

    /// `SeekLog::getNextLine` (`std::getline`): read up to the next newline.
    /// A read that reaches the end of the file sets the finished state (C++
    /// `eofbit`), even when it returns a final line without a newline.
    pub(crate) fn get_next_line(&mut self, line: &mut String) -> bool {
        line.clear();
        let mut bytes = Vec::new();
        match self.reader.read_until(b'\n', &mut bytes) {
            Ok(0) | Err(_) => {
                self.finished = true;
                false
            }
            Ok(n) => {
                self.current_pos += n as u64;
                if bytes.last() == Some(&b'\n') {
                    bytes.pop();
                } else {
                    self.finished = true;
                }
                if bytes.last() == Some(&b'\r') {
                    bytes.pop();
                }
                *line = read_line_text(bytes);
                true
            }
        }
    }

    /// `SeekLog::isFinished`: a read has reached the end of the file.
    pub(crate) fn is_finished(&self) -> bool {
        self.finished
    }
}

/// Text of a log line. C++ reads raw bytes; invalid UTF-8 is replaced with
/// '?' as `RCommitLog::filter_utf8` does for names.
fn read_line_text(bytes: Vec<u8>) -> String {
    String::from_utf8(bytes).unwrap_or_else(|e| gource_core::utf8::filter_utf8(e.as_bytes()))
}

/// A byte of standard input read by `peek_stdin` and not yet consumed.
static STDIN_LOOKAHEAD: Mutex<Option<u8>> = Mutex::new(None);

/// `std::cin.peek()`: the next byte of standard input, without consuming it.
/// The C++ format checks peek at the first character, so a rejected format
/// leaves the whole stream for the next one.
fn peek_stdin() -> Option<u8> {
    let mut lookahead = STDIN_LOOKAHEAD
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    if lookahead.is_none() {
        let mut byte = [0u8; 1];
        loop {
            match std::io::stdin().read(&mut byte) {
                Ok(1) => *lookahead = Some(byte[0]),
                Err(e) if e.kind() == ErrorKind::Interrupted => continue,
                _ => {}
            }
            break;
        }
    }
    *lookahead
}

/// Standard input, starting with the byte read by `peek_stdin`, if any.
struct Stdin;

impl Read for Stdin {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        let peeked = STDIN_LOOKAHEAD
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        match peeked {
            Some(byte) => {
                buf[0] = byte;
                Ok(1)
            }
            None => std::io::stdin().read(buf),
        }
    }
}

/// A log read from standard input (or another stream). Lines are read on a
/// thread so the app never waits for input, as C++ makes stdin non-blocking.
///
/// A commit is only taken from the stream once all of its lines have
/// arrived: a parse that runs out of input keeps its lines for the next
/// attempt, so commits are not split by the timing of the writer.
pub struct StreamLog {
    receiver: Receiver<String>,
    /// Lines received and not yet consumed by a parse.
    pending: VecDeque<String>,
    /// Number of lines of `pending` read by the current parse.
    cursor: usize,
    /// The input has ended.
    ended: bool,
    /// The current parse needed a line that has not arrived yet.
    starved: bool,
    /// Wait for lines to arrive instead of starving.
    blocking: bool,
}

impl StreamLog {
    fn new_stdin() -> Self {
        Self::from_reader(Stdin)
    }

    pub fn from_reader<R: Read + Send + 'static>(reader: R) -> Self {
        let (tx, rx) = channel::<String>();
        thread::Builder::new()
            .name("stream-log-reader".to_string())
            .spawn(move || {
                let mut buf_reader = BufReader::new(reader);
                loop {
                    let mut bytes = Vec::new();
                    match buf_reader.read_until(b'\n', &mut bytes) {
                        // `StreamLog::getNextLine` does not return a last
                        // line without a newline (it sets eofbit).
                        Ok(_) if bytes.last() != Some(&b'\n') => break,
                        Ok(_) => {
                            bytes.pop();
                            if bytes.last() == Some(&b'\r') {
                                bytes.pop();
                            }
                            if tx.send(read_line_text(bytes)).is_err() {
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
            pending: VecDeque::new(),
            cursor: 0,
            ended: false,
            starved: false,
            blocking: false,
        }
    }

    /// Create a StreamLog reading lines directly from a channel receiver.
    pub fn from_channel(rx: Receiver<String>) -> Self {
        Self {
            receiver: rx,
            pending: VecDeque::new(),
            cursor: 0,
            ended: false,
            starved: false,
            blocking: false,
        }
    }

    fn get_next_line(&mut self, line: &mut String) -> bool {
        line.clear();
        if self.cursor == self.pending.len() {
            if self.ended {
                return false;
            }
            let received = if self.blocking {
                self.receiver.recv().ok()
            } else {
                match self.receiver.try_recv() {
                    Ok(l) => Some(l),
                    Err(TryRecvError::Empty) => {
                        self.starved = true;
                        return false;
                    }
                    Err(TryRecvError::Disconnected) => None,
                }
            };
            match received {
                Some(l) => self.pending.push_back(l),
                None => {
                    self.ended = true;
                    return false;
                }
            }
        }
        line.push_str(&self.pending[self.cursor]);
        self.cursor += 1;
        true
    }

    /// Start parsing a commit.
    fn begin_parse(&mut self) {
        self.cursor = 0;
        self.starved = false;
    }

    /// Finish parsing a commit. Returns false if the parse ran out of input:
    /// its lines are kept to parse again once more input has arrived.
    fn end_parse(&mut self) -> bool {
        let complete = !self.starved;
        if complete {
            self.pending.drain(..self.cursor);
        }
        self.cursor = 0;
        self.starved = false;
        complete
    }

    /// The input has ended and every line of it has been parsed.
    pub(crate) fn is_ended(&self) -> bool {
        self.ended && self.pending.is_empty()
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
    live: bool,
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
            live: false,
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
            live: false,
        }
    }

    /// Construct a ready-to-poll live CommitLog from a channel receiver without blocking on check_format.
    pub fn from_live_stream(
        format_name: &str,
        log_command: Option<String>,
        rx: Receiver<String>,
        options: VcsOptions,
    ) -> Self {
        let stream = StreamLog::from_channel(rx);
        Self {
            format_name: format_name.to_string(),
            log_command,
            source: LogSource::Stream(stream),
            options,
            last_line: None,
            buffered_commit: None,
            live: true,
        }
    }

    /// A log reading parsed commits from `feed` (a remote stream). It is
    /// finished once the feed has ended and been drained; until then it
    /// behaves like a stream that has no new input yet.
    pub fn from_feed(feed: CommitFeed, options: VcsOptions) -> Self {
        Self {
            format_name: "remote".to_string(),
            log_command: None,
            source: LogSource::Feed(feed),
            options,
            last_line: None,
            buffered_commit: None,
            live: false,
        }
    }

    pub fn with_live(mut self, live: bool) -> Self {
        self.live = live;
        self
    }

    pub fn is_live(&self) -> bool {
        self.live
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

    /// Parse the next commit. On a stream, a parse that runs out of input is
    /// undone, to be retried once more input has arrived.
    fn parse_commit(&mut self, commit: &mut Commit) -> bool {
        self.source.begin_parse();
        let last_line = self.last_line.clone();
        let parsed = self.parse_commit_lines(commit);
        if self.source.end_parse() {
            parsed
        } else {
            self.last_line = last_line;
            false
        }
    }

    /// Low-level parse of the next commit according to `format_name`.
    fn parse_commit_lines(&mut self, commit: &mut Commit) -> bool {
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
                LogSource::Feed(_) => false,
            }
        };

        match self.format_name.as_str() {
            "custom" => {
                let mut line = String::new();
                while get_line(&mut line) {
                    if line.is_empty() {
                        if !commit.files.is_empty() {
                            break;
                        }
                        continue;
                    }
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

        if let LogSource::Feed(feed) = &self.source {
            while let Some(commit) = feed.pop() {
                if !validate || commit.is_valid(&self.options) {
                    return Some(commit);
                }
            }
            return None;
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

    /// `isFinished`: the whole log file has been read and nothing is
    /// buffered. A stream is never finished, as more input may arrive.
    pub fn is_finished(&self) -> bool {
        self.buffered_commit.is_none()
            && match &self.source {
                LogSource::Seekable(s) => s.is_finished(),
                LogSource::Feed(f) => f.is_ended(),
                LogSource::Stream(_) => false,
            }
    }

    /// Everything has been read: the end of the file, or of a stream's input.
    pub(crate) fn at_end(&self) -> bool {
        self.buffered_commit.is_none()
            && match &self.source {
                LogSource::Seekable(s) => s.is_finished(),
                LogSource::Stream(s) => s.is_ended(),
                LogSource::Feed(f) => f.is_ended(),
            }
    }

    /// Make reading from a stream wait for input, instead of returning no
    /// commit until more input has arrived. Files are unaffected.
    pub fn wait_for_input(&mut self, wait: bool) {
        if let LogSource::Stream(s) = &mut self.source {
            s.blocking = wait;
        }
    }

    pub fn is_seekable(&self) -> bool {
        matches!(&self.source, LogSource::Seekable(_))
    }

    /// Read position as a fraction of the file size (`getPercent`).
    pub fn percent(&self) -> f32 {
        match &self.source {
            LogSource::Seekable(s) => s.get_percent(),
            LogSource::Stream(_) | LogSource::Feed(_) => 0.0,
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
            LogSource::Stream(_) | LogSource::Feed(_) => return None,
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
        if (self.format_name == "custom" && self.live) || matches!(self.source, LogSource::Feed(_))
        {
            return true;
        }

        // Wait for a stream's first commit (C++ waits for input on stdin).
        let was_waiting = match &mut self.source {
            LogSource::Stream(s) => std::mem::replace(&mut s.blocking, true),
            LogSource::Seekable(_) | LogSource::Feed(_) => false,
        };
        let first = self.next_commit_unvalidated();
        self.wait_for_input(was_waiting);
        if let Some(commit) = first {
            match &mut self.source {
                LogSource::Seekable(s) => {
                    s.seek_to(0.0);
                    self.last_line = None;
                    self.buffered_commit = None;
                }
                LogSource::Stream(_) | LogSource::Feed(_) => {
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
            // `checkFirstChar` peeks at `std::cin`.
            return peek_stdin() == Some(exp as u8);
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

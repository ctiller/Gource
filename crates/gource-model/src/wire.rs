//! The L1 wire format: the commit stream from a producer (`gource-serve`)
//! to a client, as length-prefixed binary frames.
//!
//! ```text
//! stream := frame*
//! frame  := varint(len) tag:u8 payload        (len counts tag + payload)
//! ```
//!
//! | tag | event | payload |
//! |---|---|---|
//! | 1 | [`Event::Hello`] | `version:v first:zz last:zz commits:v live:u8` |
//! | 2 | user intern | `id:v name:str` |
//! | 3 | path intern | `id:v path:str` |
//! | 4 | [`Event::Commit`] | `dts:zz user:v flags:u8 nfiles:v file*` |
//! | 5 | [`Event::EndOfHistory`] | (empty) |
//! | 6 | [`Event::Error`] | `message:str` |
//!
//! `v` is an unsigned LEB128 varint, `zz` a zigzag varint, `str` is
//! `v(len)` + UTF-8. A commit's timestamp is a delta from the previous
//! commit in the stream (the first is relative to 0). Users and paths are
//! interned: the first use is preceded by an intern frame, later uses send
//! the id only.
//!
//! ```text
//! file := path:v action:u8 [other:str] fflags:u8 [added:v] [removed:v] [rgb:3*f32le]
//! ```
//! `action`: 0 add, 1 modify, 2 delete, 3 other (followed by the code).
//! `fflags`: bit 0 lines added present, bit 1 lines removed present, bit 2
//! binary, bit 3 shadow, bit 4 explicit colour. Without bit 4 the colour is
//! whatever the client derives from the path (the extension colour), so
//! ordinary logs never ship colours.

use std::collections::HashMap;

use crate::commit::{Commit, CommitFile, FileAction};

/// Wire format version, sent in [`Event::Hello`].
pub const VERSION: u32 = 1;

const TAG_HELLO: u8 = 1;
const TAG_USER: u8 = 2;
const TAG_PATH: u8 = 3;
const TAG_COMMIT: u8 = 4;
const TAG_END: u8 = 5;
const TAG_ERROR: u8 = 6;

const F_ADDED: u8 = 1;
const F_REMOVED: u8 = 2;
const F_BINARY: u8 = 4;
const F_SHADOW: u8 = 8;
const F_COLOUR: u8 = 16;

/// Commit flag: the commit is in-flight worktree state.
const C_SHADOW: u8 = 1;

/// Largest frame a decoder accepts (guards against garbage lengths).
pub const MAX_FRAME: usize = 64 << 20;

/// Stream metadata, the first frame of every stream.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Hello {
    pub version: u32,
    /// Timestamp of the first and last commit of the (filtered) history.
    pub first_timestamp: i64,
    pub last_timestamp: i64,
    /// Number of commits that will follow before [`Event::EndOfHistory`].
    pub commits: u64,
    /// More commits may arrive after the history (a live repository).
    pub live: bool,
}

/// One decoded stream event.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    Hello(Hello),
    Commit(Commit),
    /// The recorded history has been sent; a live stream continues.
    EndOfHistory,
    /// The producer failed; the stream ends.
    Error(String),
}

/// Decoding failure. The stream is unusable afterwards.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WireError(pub String);

impl std::fmt::Display for WireError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "wire format error: {}", self.0)
    }
}

impl std::error::Error for WireError {}

fn err<T>(msg: impl Into<String>) -> Result<T, WireError> {
    Err(WireError(msg.into()))
}

// ---------------------------------------------------------------- primitives

fn put_v(out: &mut Vec<u8>, mut v: u64) {
    while v >= 0x80 {
        out.push((v as u8) | 0x80);
        v >>= 7;
    }
    out.push(v as u8);
}

fn put_zz(out: &mut Vec<u8>, v: i64) {
    put_v(out, ((v << 1) ^ (v >> 63)) as u64);
}

fn put_str(out: &mut Vec<u8>, s: &str) {
    put_v(out, s.len() as u64);
    out.extend_from_slice(s.as_bytes());
}

/// Reads primitives from one frame's payload.
struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn u8(&mut self) -> Result<u8, WireError> {
        let Some(&b) = self.buf.get(self.pos) else {
            return err("truncated frame");
        };
        self.pos += 1;
        Ok(b)
    }

    fn v(&mut self) -> Result<u64, WireError> {
        let mut v: u64 = 0;
        for shift in (0..64).step_by(7) {
            let b = self.u8()?;
            v |= u64::from(b & 0x7f) << shift;
            if b & 0x80 == 0 {
                return Ok(v);
            }
        }
        err("varint too long")
    }

    fn v32(&mut self) -> Result<u32, WireError> {
        u32::try_from(self.v()?).or_else(|_| err("value out of range"))
    }

    fn zz(&mut self) -> Result<i64, WireError> {
        let u = self.v()?;
        Ok(((u >> 1) as i64) ^ -((u & 1) as i64))
    }

    fn bytes(&mut self, n: usize) -> Result<&'a [u8], WireError> {
        let end = self.pos.checked_add(n).filter(|&e| e <= self.buf.len());
        let Some(end) = end else {
            return err("truncated frame");
        };
        let s = &self.buf[self.pos..end];
        self.pos = end;
        Ok(s)
    }

    fn str(&mut self) -> Result<String, WireError> {
        let n = self.v()? as usize;
        let b = self.bytes(n)?;
        String::from_utf8(b.to_vec()).or_else(|_| err("invalid UTF-8"))
    }

    fn f32(&mut self) -> Result<f32, WireError> {
        let b = self.bytes(4)?;
        Ok(f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn done(&self) -> Result<(), WireError> {
        if self.pos == self.buf.len() {
            Ok(())
        } else {
            err("trailing bytes in frame")
        }
    }
}

/// Append one frame (`tag` + `payload`) to `out`.
fn put_frame(out: &mut Vec<u8>, tag: u8, payload: &[u8]) {
    put_v(out, payload.len() as u64 + 1);
    out.push(tag);
    out.extend_from_slice(payload);
}

// ------------------------------------------------------------------ encoder

/// Encodes events into frames, interning users and paths. One encoder per
/// stream.
pub struct Encoder<D: Fn(&str) -> [f32; 3]> {
    users: HashMap<String, u32>,
    paths: HashMap<String, u32>,
    last_timestamp: i64,
    derive_colour: D,
    scratch: Vec<u8>,
}

impl<D: Fn(&str) -> [f32; 3]> Encoder<D> {
    /// `derive_colour` must match the client's: colours equal to it are not
    /// sent.
    pub fn new(derive_colour: D) -> Self {
        Self {
            users: HashMap::new(),
            paths: HashMap::new(),
            last_timestamp: 0,
            derive_colour,
            scratch: Vec::new(),
        }
    }

    pub fn hello(&mut self, hello: &Hello, out: &mut Vec<u8>) {
        let mut p = Vec::new();
        put_v(&mut p, u64::from(hello.version));
        put_zz(&mut p, hello.first_timestamp);
        put_zz(&mut p, hello.last_timestamp);
        put_v(&mut p, hello.commits);
        p.push(u8::from(hello.live));
        put_frame(out, TAG_HELLO, &p);
    }

    pub fn end_of_history(&mut self, out: &mut Vec<u8>) {
        put_frame(out, TAG_END, &[]);
    }

    pub fn error(&mut self, message: &str, out: &mut Vec<u8>) {
        let mut p = Vec::new();
        put_str(&mut p, message);
        put_frame(out, TAG_ERROR, &p);
    }

    fn intern(
        map: &mut HashMap<String, u32>,
        tag: u8,
        s: &str,
        out: &mut Vec<u8>,
        scratch: &mut Vec<u8>,
    ) -> u32 {
        if let Some(&id) = map.get(s) {
            return id;
        }
        let id = map.len() as u32;
        map.insert(s.to_owned(), id);
        scratch.clear();
        put_v(scratch, u64::from(id));
        put_str(scratch, s);
        put_frame(out, tag, scratch);
        id
    }

    pub fn commit(&mut self, commit: &Commit, out: &mut Vec<u8>) {
        let user = Self::intern(
            &mut self.users,
            TAG_USER,
            &commit.username,
            out,
            &mut self.scratch,
        );
        let path_ids: Vec<u32> = commit
            .files
            .iter()
            .map(|f| {
                Self::intern(
                    &mut self.paths,
                    TAG_PATH,
                    &f.filename,
                    out,
                    &mut self.scratch,
                )
            })
            .collect();

        let mut p = Vec::new();
        put_zz(&mut p, commit.timestamp.wrapping_sub(self.last_timestamp));
        self.last_timestamp = commit.timestamp;
        put_v(&mut p, u64::from(user));
        p.push(if commit.is_shadow { C_SHADOW } else { 0 });
        put_v(&mut p, commit.files.len() as u64);
        for (f, &pid) in commit.files.iter().zip(&path_ids) {
            put_v(&mut p, u64::from(pid));
            match &f.action {
                FileAction::Add => p.push(0),
                FileAction::Modify => p.push(1),
                FileAction::Delete => p.push(2),
                FileAction::Other(code) => {
                    p.push(3);
                    put_str(&mut p, code);
                }
            }
            let explicit =
                f.colour.map(f32::to_bits) != (self.derive_colour)(&f.filename).map(f32::to_bits);
            let mut flags = 0;
            if f.lines_added.is_some() {
                flags |= F_ADDED;
            }
            if f.lines_removed.is_some() {
                flags |= F_REMOVED;
            }
            if f.is_binary {
                flags |= F_BINARY;
            }
            if f.is_shadow {
                flags |= F_SHADOW;
            }
            if explicit {
                flags |= F_COLOUR;
            }
            p.push(flags);
            if let Some(a) = f.lines_added {
                put_v(&mut p, u64::from(a));
            }
            if let Some(r) = f.lines_removed {
                put_v(&mut p, u64::from(r));
            }
            if explicit {
                for c in f.colour {
                    p.extend_from_slice(&c.to_le_bytes());
                }
            }
        }
        put_frame(out, TAG_COMMIT, &p);
    }
}

// ------------------------------------------------------------------ decoder

/// Incremental decoder: feed bytes as they arrive with [`Decoder::push`],
/// then drain events with [`Decoder::next_event`].
pub struct Decoder<D: Fn(&str) -> [f32; 3]> {
    buf: Vec<u8>,
    pos: usize,
    users: Vec<String>,
    paths: Vec<String>,
    last_timestamp: i64,
    derive_colour: D,
    failed: bool,
}

impl<D: Fn(&str) -> [f32; 3]> Decoder<D> {
    pub fn new(derive_colour: D) -> Self {
        Self {
            buf: Vec::new(),
            pos: 0,
            users: Vec::new(),
            paths: Vec::new(),
            last_timestamp: 0,
            derive_colour,
            failed: false,
        }
    }

    /// Append received bytes.
    pub fn push(&mut self, bytes: &[u8]) {
        if self.pos > 0 && self.pos * 2 > self.buf.len() {
            self.buf.drain(..self.pos);
            self.pos = 0;
        }
        self.buf.extend_from_slice(bytes);
    }

    /// Bytes received but not yet decoded.
    pub fn pending(&self) -> usize {
        self.buf.len() - self.pos
    }

    /// The next complete event, `None` if more bytes are needed. Intern
    /// frames are consumed silently. After an error every call fails.
    pub fn next_event(&mut self) -> Option<Result<Event, WireError>> {
        if self.failed {
            return Some(err("stream already failed"));
        }
        loop {
            let frame = match self.take_frame() {
                Ok(Some(f)) => f,
                Ok(None) => return None,
                Err(e) => {
                    self.failed = true;
                    return Some(Err(e));
                }
            };
            match self.decode_frame(&frame) {
                Ok(Some(ev)) => return Some(Ok(ev)),
                Ok(None) => continue,
                Err(e) => {
                    self.failed = true;
                    return Some(Err(e));
                }
            }
        }
    }

    fn take_frame(&mut self) -> Result<Option<Vec<u8>>, WireError> {
        let mut r = Reader {
            buf: &self.buf[self.pos..],
            pos: 0,
        };
        let len = match r.v() {
            Ok(len) => len as usize,
            // A varint cut short by the end of the buffer: wait for more.
            Err(_) if self.buf.len() - self.pos < 10 => return Ok(None),
            Err(e) => return Err(e),
        };
        if len == 0 || len > MAX_FRAME {
            return err(format!("bad frame length {len}"));
        }
        let start = self.pos + r.pos;
        if self.buf.len() < start + len {
            return Ok(None);
        }
        let frame = self.buf[start..start + len].to_vec();
        self.pos = start + len;
        Ok(Some(frame))
    }

    fn decode_frame(&mut self, frame: &[u8]) -> Result<Option<Event>, WireError> {
        let mut r = Reader {
            buf: &frame[1..],
            pos: 0,
        };
        let ev = match frame[0] {
            TAG_HELLO => {
                let hello = Hello {
                    version: r.v32()?,
                    first_timestamp: r.zz()?,
                    last_timestamp: r.zz()?,
                    commits: r.v()?,
                    live: r.u8()? != 0,
                };
                if hello.version != VERSION {
                    return err(format!("unsupported version {}", hello.version));
                }
                Some(Event::Hello(hello))
            }
            TAG_USER | TAG_PATH => {
                let id = r.v()? as usize;
                let s = r.str()?;
                let table = if frame[0] == TAG_USER {
                    &mut self.users
                } else {
                    &mut self.paths
                };
                if id != table.len() {
                    return err(format!("intern id {id} out of order"));
                }
                table.push(s);
                None
            }
            TAG_COMMIT => Some(Event::Commit(self.decode_commit(&mut r)?)),
            TAG_END => Some(Event::EndOfHistory),
            TAG_ERROR => Some(Event::Error(r.str()?)),
            tag => return err(format!("unknown tag {tag}")),
        };
        r.done()?;
        Ok(ev)
    }

    fn decode_commit(&mut self, r: &mut Reader<'_>) -> Result<Commit, WireError> {
        let timestamp = self.last_timestamp.wrapping_add(r.zz()?);
        self.last_timestamp = timestamp;
        let user = r.v()? as usize;
        let Some(username) = self.users.get(user).cloned() else {
            return err(format!("unknown user id {user}"));
        };
        let is_shadow = r.u8()? & C_SHADOW != 0;
        let n = r.v()? as usize;
        let mut files = Vec::with_capacity(n.min(4096));
        for _ in 0..n {
            let pid = r.v()? as usize;
            let Some(filename) = self.paths.get(pid).cloned() else {
                return err(format!("unknown path id {pid}"));
            };
            let action = match r.u8()? {
                0 => FileAction::Add,
                1 => FileAction::Modify,
                2 => FileAction::Delete,
                3 => FileAction::Other(r.str()?),
                a => return err(format!("unknown action {a}")),
            };
            let flags = r.u8()?;
            let lines_added = if flags & F_ADDED != 0 {
                Some(r.v32()?)
            } else {
                None
            };
            let lines_removed = if flags & F_REMOVED != 0 {
                Some(r.v32()?)
            } else {
                None
            };
            let colour = if flags & F_COLOUR != 0 {
                [r.f32()?, r.f32()?, r.f32()?]
            } else {
                (self.derive_colour)(&filename)
            };
            files.push(CommitFile {
                filename,
                action,
                colour,
                lines_added,
                lines_removed,
                is_binary: flags & F_BINARY != 0,
                is_shadow: flags & F_SHADOW != 0,
            });
        }
        Ok(Commit {
            timestamp,
            username,
            files,
            is_shadow,
        })
    }
}

// --------------------------------------------------------------- filter spec

/// Server-side filters, sent as subscription parameters. Regexes use the
/// `--file-filter` etc. semantics (`*_filter` hides matches, `*_show_filter`
/// keeps only matches).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FilterSpec {
    pub file_filters: Vec<String>,
    pub file_show_filters: Vec<String>,
    pub user_filters: Vec<String>,
    pub user_show_filters: Vec<String>,
}

const FILTER_KEYS: [&str; 4] = ["ff", "fs", "uf", "us"];

fn pct_encode(s: &str, out: &mut String) {
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
}

fn pct_decode(s: &str) -> Option<String> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'%' => {
                let hex = s.get(i + 1..i + 3)?;
                out.push(u8::from_str_radix(hex, 16).ok()?);
                i += 3;
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8(out).ok()
}

impl FilterSpec {
    fn lists(&self) -> [&Vec<String>; 4] {
        [
            &self.file_filters,
            &self.file_show_filters,
            &self.user_filters,
            &self.user_show_filters,
        ]
    }

    pub fn is_empty(&self) -> bool {
        self.lists().iter().all(|l| l.is_empty())
    }

    /// URL query pairs (`ff=..&fs=..&uf=..&us=..`, repeated per regex),
    /// without a leading `?`.
    pub fn to_query(&self) -> String {
        let mut q = String::new();
        for (key, list) in FILTER_KEYS.iter().zip(self.lists()) {
            for v in list {
                if !q.is_empty() {
                    q.push('&');
                }
                q.push_str(key);
                q.push('=');
                pct_encode(v, &mut q);
            }
        }
        q
    }

    /// Parse the filter pairs out of a query string, ignoring other keys.
    pub fn from_query(query: &str) -> Result<Self, WireError> {
        let mut spec = FilterSpec::default();
        for pair in query.trim_start_matches('?').split('&') {
            let Some((k, v)) = pair.split_once('=') else {
                continue;
            };
            let list = match k {
                "ff" => &mut spec.file_filters,
                "fs" => &mut spec.file_show_filters,
                "uf" => &mut spec.user_filters,
                "us" => &mut spec.user_show_filters,
                _ => continue,
            };
            let Some(v) = pct_decode(v) else {
                return err(format!("bad escape in {k}"));
            };
            list.push(v);
        }
        Ok(spec)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn derive(path: &str) -> [f32; 3] {
        if path.ends_with(".rs") {
            [0.25, 0.5, 1.0]
        } else {
            [1.0; 3]
        }
    }

    fn file(name: &str, action: FileAction) -> CommitFile {
        CommitFile {
            filename: name.into(),
            action,
            colour: derive(name),
            ..Default::default()
        }
    }

    fn sample() -> Vec<Commit> {
        vec![
            Commit {
                timestamp: 1_000_000,
                username: "alice".into(),
                files: vec![
                    file("/src/main.rs", FileAction::Add),
                    CommitFile {
                        lines_added: Some(10),
                        lines_removed: Some(0),
                        is_binary: true,
                        ..file("/README", FileAction::Modify)
                    },
                ],
                is_shadow: false,
            },
            Commit {
                timestamp: 999_990, // goes backwards: negative delta
                username: "bob".into(),
                files: vec![
                    CommitFile {
                        colour: [0.1, 0.2, 0.3],
                        is_shadow: true,
                        ..file("/src/main.rs", FileAction::Other("R".into()))
                    },
                    file("/old.txt", FileAction::Delete),
                ],
                is_shadow: true,
            },
            Commit {
                timestamp: 2_000_000,
                username: "alice".into(),
                files: vec![file("/src/main.rs", FileAction::Modify)],
                is_shadow: false,
            },
        ]
    }

    fn encode_all(commits: &[Commit]) -> Vec<u8> {
        let mut enc = Encoder::new(derive);
        let mut out = Vec::new();
        enc.hello(
            &Hello {
                version: VERSION,
                first_timestamp: 999_990,
                last_timestamp: 2_000_000,
                commits: commits.len() as u64,
                live: true,
            },
            &mut out,
        );
        for c in commits {
            enc.commit(c, &mut out);
        }
        enc.end_of_history(&mut out);
        enc.error("boom", &mut out);
        out
    }

    fn decode_all(dec: &mut Decoder<fn(&str) -> [f32; 3]>) -> Vec<Event> {
        let mut evs = Vec::new();
        while let Some(ev) = dec.next_event() {
            evs.push(ev.unwrap());
        }
        evs
    }

    #[test]
    fn round_trip_in_one_push_and_byte_by_byte() {
        let commits = sample();
        let bytes = encode_all(&commits);
        let mut expect = vec![Event::Hello(Hello {
            version: VERSION,
            first_timestamp: 999_990,
            last_timestamp: 2_000_000,
            commits: 3,
            live: true,
        })];
        expect.extend(commits.iter().cloned().map(Event::Commit));
        expect.push(Event::EndOfHistory);
        expect.push(Event::Error("boom".into()));

        let mut dec: Decoder<fn(&str) -> [f32; 3]> = Decoder::new(derive);
        dec.push(&bytes);
        assert_eq!(decode_all(&mut dec), expect);
        assert_eq!(dec.pending(), 0);

        let mut dec: Decoder<fn(&str) -> [f32; 3]> = Decoder::new(derive);
        let mut got = Vec::new();
        for b in &bytes {
            dec.push(std::slice::from_ref(b));
            got.extend(decode_all(&mut dec));
        }
        assert_eq!(got, expect);
    }

    #[test]
    fn interning_and_derived_colours_keep_frames_small() {
        let commits = sample();
        let mut enc = Encoder::new(derive);
        let mut first = Vec::new();
        enc.commit(&commits[0], &mut first);
        let mut again = Vec::new();
        enc.commit(&commits[2], &mut again);
        // Repeat user + path: no intern frames, no colour: a few bytes.
        assert!(again.len() < 12, "{}", again.len());
        assert!(first.len() > again.len());
    }

    #[test]
    fn malformed_streams_fail_and_stay_failed() {
        let cases: Vec<(Vec<u8>, &str)> = vec![
            (vec![0], "bad frame length"),
            (vec![1, 99], "unknown tag"),
            (vec![2, TAG_HELLO, 9], "truncated"),
            (vec![3, TAG_USER, 0, 5], "truncated"),
            (vec![3, TAG_USER, 1, 0], "out of order"),
            (vec![2, TAG_END, 0], "trailing"),
            (vec![4, TAG_USER, 0, 1, 0xff], "UTF-8"),
            (vec![6, TAG_HELLO, 2, 0, 0, 0, 0], "version"),
            (vec![5, TAG_COMMIT, 0, 0, 0, 0], "unknown user"),
            ([vec![12, TAG_HELLO], vec![0xff; 11]].concat(), "varint"),
        ];
        for (bytes, want) in cases {
            let mut dec: Decoder<fn(&str) -> [f32; 3]> = Decoder::new(derive);
            dec.push(&bytes);
            let e = dec.next_event().unwrap().unwrap_err();
            assert!(e.to_string().contains(want), "{bytes:?}: {e}");
            assert!(dec.next_event().unwrap().is_err());
        }
        // Commit body errors: unknown path, unknown action, huge u32.
        let mut base = Vec::new();
        put_frame(&mut base, TAG_USER, &[0, 1, b'u']);
        for (body, want) in [
            (vec![0, 0, 0, 1, 7], "unknown path"),
            (vec![0, 0, 0, 0, 9], "trailing"),
        ] {
            let mut bytes = base.clone();
            put_frame(&mut bytes, TAG_COMMIT, &body);
            let mut dec: Decoder<fn(&str) -> [f32; 3]> = Decoder::new(derive);
            dec.push(&bytes);
            let e = dec.next_event().unwrap().unwrap_err();
            assert!(e.to_string().contains(want), "{e}");
        }
        let mut bytes = base.clone();
        put_frame(&mut bytes, TAG_PATH, &[0, 1, b'p']);
        put_frame(&mut bytes, TAG_COMMIT, &[0, 0, 0, 1, 0, 9]);
        let mut dec: Decoder<fn(&str) -> [f32; 3]> = Decoder::new(derive);
        dec.push(&bytes);
        assert!(
            dec.next_event()
                .unwrap()
                .unwrap_err()
                .0
                .contains("unknown action")
        );
        let mut bytes = base.clone();
        put_frame(&mut bytes, TAG_PATH, &[0, 1, b'p']);
        let mut body = vec![0, 0, 0, 1, 0, 0, F_ADDED];
        put_v(&mut body, u64::MAX >> 1);
        put_frame(&mut bytes, TAG_COMMIT, &body);
        let mut dec: Decoder<fn(&str) -> [f32; 3]> = Decoder::new(derive);
        dec.push(&bytes);
        assert!(dec.next_event().unwrap().unwrap_err().0.contains("range"));
        assert_eq!(WireError("x".into()).to_string(), "wire format error: x");
    }

    #[test]
    fn decoder_compacts_its_buffer() {
        let bytes = encode_all(&sample());
        let mut dec: Decoder<fn(&str) -> [f32; 3]> = Decoder::new(derive);
        let mut n = 0;
        for chunk in bytes.chunks(7).chain(bytes.chunks(7)) {
            dec.push(chunk);
            while let Some(ev) = dec.next_event() {
                if ev.is_err() {
                    break;
                }
                n += 1;
            }
        }
        assert!(n >= 6);
    }

    #[test]
    fn filter_spec_query_round_trip() {
        let spec = FilterSpec {
            file_filters: vec!["\\.png$".into(), "a b&c=d".into()],
            file_show_filters: vec!["^/src/".into()],
            user_filters: vec!["bot".into()],
            user_show_filters: vec!["ünïcode".into()],
        };
        let q = spec.to_query();
        assert!(!q.contains(' ') && !q.contains("&c="));
        assert_eq!(FilterSpec::from_query(&q).unwrap(), spec);
        assert_eq!(
            FilterSpec::from_query(&format!("?from=5&x&{q}")).unwrap(),
            spec
        );
        assert_eq!(
            FilterSpec::from_query("ff=a+b").unwrap().file_filters,
            vec!["a b"]
        );
        assert!(FilterSpec::from_query("ff=%zz").is_err());
        assert!(FilterSpec::from_query("ff=%ff").is_err());
        assert!(FilterSpec::default().is_empty());
        assert!(!spec.is_empty());
        assert_eq!(FilterSpec::default().to_query(), "");
    }
}

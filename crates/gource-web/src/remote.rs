//! Decoding a `gource-serve` stream into a commit feed.

use gource_core::StringHasher;
use gource_model::wire::{Decoder, Event, FilterSpec, Hello};
use gource_vcs::CommitFeed;
use gource_vcs::commit::file_colour;

type DeriveColour = Box<dyn Fn(&str) -> [f32; 3]>;

/// The `/stream` URL under `server` (no trailing slash needed).
pub fn stream_url(server: &str, from: Option<i64>, filter: &FilterSpec) -> String {
    let mut url = format!("{}/stream", server.trim_end_matches('/'));
    let mut sep = '?';
    if let Some(from) = from {
        url.push_str(&format!("{sep}from={from}"));
        sep = '&';
    }
    if !filter.is_empty() {
        url.push(sep);
        url.push_str(&filter.to_query());
    }
    url
}

/// Feeds the bytes of one `/stream` response into a [`CommitFeed`].
pub struct RemoteStream {
    decoder: Decoder<DeriveColour>,
    feed: CommitFeed,
    hello: Option<Hello>,
    commits: u64,
    history_done: bool,
}

impl RemoteStream {
    /// `hasher` must match the server's (it derives the colours the wire
    /// format omits).
    pub fn new(feed: CommitFeed, hasher: StringHasher) -> Self {
        Self {
            decoder: Decoder::new(Box::new(move |p| file_colour(p, &hasher))),
            feed,
            hello: None,
            commits: 0,
            history_done: false,
        }
    }

    /// The stream's metadata, once received.
    pub fn hello(&self) -> Option<&Hello> {
        self.hello.as_ref()
    }

    /// Commits received so far.
    pub fn commits(&self) -> u64 {
        self.commits
    }

    /// The recorded history has been received completely.
    pub fn history_done(&self) -> bool {
        self.history_done
    }

    /// Decode `bytes` (any split) and queue the commits. An error ends the
    /// feed; the stream is unusable afterwards.
    pub fn push(&mut self, bytes: &[u8]) -> Result<(), String> {
        self.decoder.push(bytes);
        while let Some(event) = self.decoder.next_event() {
            let event = event.map_err(|e| self.fail(e.to_string()))?;
            match event {
                Event::Hello(h) => self.hello = Some(h),
                Event::Commit(c) => {
                    self.commits += 1;
                    self.feed.push(c);
                }
                Event::EndOfHistory => {
                    self.history_done = true;
                    if !self.hello.as_ref().is_some_and(|h| h.live) {
                        self.feed.end();
                    }
                }
                Event::Error(msg) => return Err(self.fail(format!("server error: {msg}"))),
            }
        }
        Ok(())
    }

    /// The response body ended.
    pub fn finish(&mut self) -> Result<(), String> {
        if self.decoder.pending() > 0 {
            return Err(self.fail("stream truncated mid-frame".into()));
        }
        self.feed.end();
        if self.history_done {
            Ok(())
        } else {
            Err("stream ended before the end of the history".into())
        }
    }

    fn fail(&mut self, msg: String) -> String {
        self.feed.end();
        msg
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gource_serve::History;
    use gource_serve::stream::{Chunk, StreamWriter};
    use gource_vcs::{Commit, CommitFile, CommitLog, VcsOptions};
    use std::sync::Arc;

    fn commit(ts: i64, file: &str) -> Commit {
        Commit {
            timestamp: ts,
            username: "u".into(),
            files: vec![CommitFile {
                filename: file.into(),
                colour: file_colour(file, &StringHasher::default()),
                ..Default::default()
            }],
            is_shadow: false,
        }
    }

    fn served(history: &Arc<History>) -> Vec<u8> {
        let mut w = StreamWriter::new(history.clone(), Default::default(), i64::MIN);
        let mut bytes = Vec::new();
        while let Chunk::Data(b) = w.next_chunk(1 << 16) {
            bytes.extend(b);
        }
        bytes
    }

    #[test]
    fn urls() {
        assert_eq!(
            stream_url("http://h:1/", None, &FilterSpec::default()),
            "http://h:1/stream"
        );
        let f = FilterSpec {
            user_filters: vec!["a b".into()],
            ..Default::default()
        };
        assert_eq!(stream_url("", Some(5), &f), "/stream?from=5&uf=a%20b");
        assert_eq!(stream_url("", None, &f), "/stream?uf=a%20b");
    }

    #[test]
    fn server_stream_reaches_a_commit_log_byte_by_byte() {
        let h = History::new(
            vec![commit(1, "/a.rs"), commit(2, "/b/c.txt")],
            false,
            StringHasher::default(),
        );
        let bytes = served(&h);
        let feed = CommitFeed::new();
        let mut log = CommitLog::from_feed(feed.clone(), VcsOptions::default());
        let mut rs = RemoteStream::new(feed, StringHasher::default());
        for b in &bytes {
            rs.push(std::slice::from_ref(b)).unwrap();
        }
        assert_eq!(rs.hello().unwrap().commits, 2);
        assert_eq!((rs.commits(), rs.history_done()), (2, true));
        assert_eq!(log.next_commit(), Some(commit(1, "/a.rs")));
        assert_eq!(log.next_commit(), Some(commit(2, "/b/c.txt")));
        assert!(log.next_commit().is_none());
        assert!(log.is_finished());
        assert_eq!(rs.finish(), Ok(()));
    }

    #[test]
    fn live_streams_keep_the_feed_open() {
        let h = History::new(vec![commit(1, "/a")], true, StringHasher::default());
        let feed = CommitFeed::new();
        let log = CommitLog::from_feed(feed.clone(), VcsOptions::default());
        let mut rs = RemoteStream::new(feed, StringHasher::default());
        rs.push(&served(&h)).unwrap();
        assert!(rs.history_done());
        assert!(!log.is_finished());
        // The connection dropping ends the feed.
        assert_eq!(rs.finish(), Ok(()));
        assert!(!log.is_finished(), "one commit still queued");
    }

    #[test]
    fn errors_end_the_feed() {
        let h = History::new(vec![commit(1, "/a")], false, StringHasher::default());
        let bytes = served(&h);

        // Truncated mid-frame.
        let feed = CommitFeed::new();
        let mut rs = RemoteStream::new(feed.clone(), StringHasher::default());
        rs.push(&bytes[..bytes.len() - 3]).unwrap();
        assert!(rs.finish().unwrap_err().contains("truncated"));

        // Cut at a frame boundary before EndOfHistory.
        let mut rs = RemoteStream::new(CommitFeed::new(), StringHasher::default());
        rs.push(&bytes[..0]).unwrap();
        assert!(rs.finish().unwrap_err().contains("before the end"));

        // Garbage.
        let feed = CommitFeed::new();
        let log = CommitLog::from_feed(feed.clone(), VcsOptions::default());
        let mut rs = RemoteStream::new(feed, StringHasher::default());
        assert!(rs.push(&[0xff; 16]).is_err());
        assert!(log.is_finished());

        // A server-reported error.
        let mut enc = gource_model::wire::Encoder::new(|_: &str| [1.0; 3]);
        let mut out = Vec::new();
        enc.error("boom", &mut out);
        let mut rs = RemoteStream::new(CommitFeed::new(), StringHasher::default());
        assert_eq!(rs.push(&out), Err("server error: boom".into()));
    }
}

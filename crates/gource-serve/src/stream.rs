//! Encoding one subscription into wire frames.

use crate::filter::Filter;
use crate::store::History;
use gource_model::wire::{Encoder, Hello, VERSION};
use gource_vcs::commit::file_colour;
use std::sync::Arc;

type DeriveColour = Box<dyn Fn(&str) -> [f32; 3] + Send + Sync>;

/// What [`StreamWriter::next_chunk`] produced.
#[derive(Debug, PartialEq, Eq)]
pub enum Chunk {
    /// Bytes to send.
    Data(Vec<u8>),
    /// Nothing to send until the history grows (live repositories).
    Wait,
    /// The stream is complete.
    Done,
}

enum Phase {
    Hello,
    /// Sending `matches[next..]`, then `EndOfHistory`.
    History {
        matches: Vec<usize>,
        next: usize,
        end: usize,
    },
    /// Sending commits appended after index `cursor`.
    Live {
        cursor: usize,
    },
    Done,
}

/// The state of one `/stream` response: Hello, the filtered commits with
/// `timestamp >= from`, EndOfHistory, then live commits.
pub struct StreamWriter {
    history: Arc<History>,
    filter: Filter,
    from: i64,
    encoder: Encoder<DeriveColour>,
    phase: Phase,
}

impl StreamWriter {
    pub fn new(history: Arc<History>, filter: Filter, from: i64) -> Self {
        let hasher = history.hasher();
        let derive: DeriveColour = Box::new(move |p| file_colour(p, &hasher));
        Self {
            history,
            filter,
            from,
            encoder: Encoder::new(derive),
            phase: Phase::Hello,
        }
    }

    /// The next piece of the stream, about `max` bytes (at least one frame).
    pub fn next_chunk(&mut self, max: usize) -> Chunk {
        let mut out = Vec::new();
        loop {
            match &mut self.phase {
                Phase::Hello => self.hello(&mut out),
                Phase::History { matches, next, end } => {
                    let (matches, next, end) = (std::mem::take(matches), *next, *end);
                    let (sent, done) = self.history.read(|commits| {
                        let mut i = next;
                        while i < matches.len() && out.len() < max {
                            if let Some(c) = self.filter.apply(&commits[matches[i]]) {
                                self.encoder.commit(&c, &mut out);
                            }
                            i += 1;
                        }
                        (i, i == matches.len())
                    });
                    if done {
                        self.encoder.end_of_history(&mut out);
                        self.phase = if self.history.is_live() {
                            Phase::Live { cursor: end }
                        } else {
                            Phase::Done
                        };
                    } else {
                        self.phase = Phase::History {
                            matches,
                            next: sent,
                            end,
                        };
                    }
                }
                Phase::Live { cursor } => {
                    let start = *cursor;
                    let next = self.history.read(|commits| {
                        let mut i = start;
                        while i < commits.len() && out.len() < max {
                            if let Some(c) = self.filter.apply(&commits[i]) {
                                self.encoder.commit(&c, &mut out);
                            }
                            i += 1;
                        }
                        i
                    });
                    self.phase = Phase::Live { cursor: next };
                    return if out.is_empty() {
                        Chunk::Wait
                    } else {
                        Chunk::Data(out)
                    };
                }
                Phase::Done => {
                    return if out.is_empty() {
                        Chunk::Done
                    } else {
                        Chunk::Data(out)
                    };
                }
            }
            if out.len() >= max {
                return Chunk::Data(out);
            }
        }
    }

    /// Select the matching commits and send the Hello describing them.
    fn hello(&mut self, out: &mut Vec<u8>) {
        let (matches, end, first, last) = self.history.read(|commits| {
            let mut matches = Vec::new();
            let (mut first, mut last) = (0, 0);
            for (i, c) in commits.iter().enumerate() {
                if c.timestamp >= self.from && self.filter.apply(c).is_some() {
                    if matches.is_empty() {
                        first = c.timestamp;
                    }
                    last = c.timestamp;
                    matches.push(i);
                }
            }
            (matches, commits.len(), first, last)
        });
        let hello = Hello {
            version: VERSION,
            first_timestamp: first,
            last_timestamp: last,
            commits: matches.len() as u64,
            live: self.history.is_live(),
        };
        self.encoder.hello(&hello, out);
        self.phase = Phase::History {
            matches,
            next: 0,
            end,
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gource_core::StringHasher;
    use gource_model::wire::{Decoder, Event, FilterSpec};
    use gource_vcs::{Commit, CommitFile};

    fn commit(ts: i64, user: &str, file: &str) -> Commit {
        Commit {
            timestamp: ts,
            username: user.into(),
            files: vec![CommitFile {
                filename: file.into(),
                colour: file_colour(file, &StringHasher::default()),
                ..Default::default()
            }],
            is_shadow: false,
        }
    }

    fn decode(bytes: &[u8]) -> Vec<Event> {
        let hasher = StringHasher::default();
        let mut d = Decoder::new(move |p: &str| file_colour(p, &hasher));
        d.push(bytes);
        std::iter::from_fn(|| d.next_event().map(|e| e.unwrap())).collect()
    }

    fn drain(w: &mut StreamWriter, max: usize) -> (Vec<u8>, Chunk) {
        let mut bytes = Vec::new();
        loop {
            match w.next_chunk(max) {
                Chunk::Data(b) => {
                    assert!(!b.is_empty());
                    bytes.extend(b);
                }
                other => return (bytes, other),
            }
        }
    }

    fn history(live: bool) -> Arc<History> {
        History::new(
            vec![
                commit(10, "alice", "/a.rs"),
                commit(20, "bob", "/b.c"),
                commit(30, "alice", "/c.rs"),
            ],
            live,
            StringHasher::default(),
        )
    }

    #[test]
    fn full_history_round_trips_in_small_chunks() {
        let h = history(false);
        let mut w = StreamWriter::new(h.clone(), Filter::default(), i64::MIN);
        let (bytes, last) = drain(&mut w, 1);
        assert_eq!(last, Chunk::Done);
        let events = decode(&bytes);
        assert_eq!(
            events[0],
            Event::Hello(Hello {
                version: VERSION,
                first_timestamp: 10,
                last_timestamp: 30,
                commits: 3,
                live: false
            })
        );
        let commits: Vec<_> = h.read(|c| c.to_vec());
        for (e, c) in events[1..4].iter().zip(&commits) {
            assert_eq!(e, &Event::Commit(c.clone()));
        }
        assert_eq!(events[4], Event::EndOfHistory);
        assert_eq!(events.len(), 5);
        assert_eq!(w.next_chunk(1), Chunk::Done);
    }

    #[test]
    fn from_and_filter_select_commits() {
        let filter = Filter::compile(&FilterSpec {
            user_show_filters: vec!["alice".into()],
            ..Default::default()
        })
        .unwrap();
        let mut w = StreamWriter::new(history(false), filter, 15);
        let (bytes, _) = drain(&mut w, 1 << 16);
        let events = decode(&bytes);
        assert!(matches!(&events[0], Event::Hello(h) if h.commits == 1 && h.first_timestamp == 30));
        assert!(matches!(&events[1], Event::Commit(c) if c.timestamp == 30));
        assert_eq!(events[2], Event::EndOfHistory);
    }

    #[test]
    fn empty_selection_still_sends_hello_and_end() {
        let mut w = StreamWriter::new(history(false), Filter::default(), 1000);
        let (bytes, _) = drain(&mut w, 1 << 16);
        let events = decode(&bytes);
        assert_eq!(events.len(), 2);
        assert!(matches!(&events[0], Event::Hello(h) if h.commits == 0));
    }

    #[test]
    fn live_stream_waits_then_sends_appended_commits() {
        let h = history(true);
        let filter = Filter::compile(&FilterSpec {
            file_filters: vec!["\\.c$".into()],
            ..Default::default()
        })
        .unwrap();
        let mut w = StreamWriter::new(h.clone(), filter, i64::MIN);
        let (bytes, last) = drain(&mut w, 1 << 16);
        assert_eq!(last, Chunk::Wait);
        let events = decode(&bytes);
        assert!(matches!(&events[0], Event::Hello(h) if h.live && h.commits == 2));
        assert_eq!(events.last(), Some(&Event::EndOfHistory));

        h.append([commit(40, "carol", "/d.c")]);
        assert_eq!(w.next_chunk(1 << 16), Chunk::Wait, "filtered out");
        h.append([commit(50, "carol", "/d.rs")]);
        // Continue decoding with the same path table.
        let (more, last) = drain(&mut w, 1 << 16);
        assert_eq!(last, Chunk::Wait);
        let mut all = bytes;
        all.extend(more);
        let events = decode(&all);
        assert!(matches!(events.last(), Some(Event::Commit(c)) if c.timestamp == 50));
    }
}

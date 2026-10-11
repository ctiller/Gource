//! What the page must show when the app cannot continue.

use gource_app::PlatformRequest;

/// The messages of the fatal errors among `requests`, in order.
pub fn fatal_messages(requests: &[PlatformRequest]) -> Vec<String> {
    requests
        .iter()
        .filter_map(|r| match r {
            PlatformRequest::Fatal(msg) => Some(msg.clone()),
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use gource_app::{AppOptions, GourceApp, Viewport};
    use gource_core::UVec2;
    use gource_draw::DrawList;
    use gource_model::wire::{Encoder, Hello, VERSION};
    use gource_settings::{CliAction, parse_command_line};
    use gource_vcs::{Commit, CommitFeed, CommitFile};

    use crate::remote::RemoteStream;
    use gource_core::StringHasher;

    fn app(feed: &CommitFeed) -> GourceApp {
        let argv: Vec<String> = vec![];
        let CliAction::Run(config) = parse_command_line(&argv).unwrap() else {
            panic!("expected a run");
        };
        let options = AppOptions {
            feed: Some(feed.clone()),
            ..Default::default()
        };
        GourceApp::new(config, options).unwrap()
    }

    /// Run `frames` frames and collect the requests.
    fn run(app: &mut GourceApp, frames: usize) -> Vec<PlatformRequest> {
        let mut list = DrawList::new(UVec2::new(320, 240));
        let mut out = Vec::new();
        for _ in 0..frames {
            app.frame(1.0 / 60.0, Viewport::new(320, 240), &mut list);
            out.extend(app.take_requests());
        }
        out
    }

    fn wire(live: bool, commits: &[Commit]) -> Vec<u8> {
        let mut enc = Encoder::new(|_: &str| [1.0; 3]);
        let mut out = Vec::new();
        enc.hello(
            &Hello {
                version: VERSION,
                first_timestamp: 0,
                last_timestamp: 0,
                commits: commits.len() as u64,
                live,
            },
            &mut out,
        );
        for c in commits {
            enc.commit(c, &mut out);
        }
        enc.end_of_history(&mut out);
        out
    }

    fn commit(ts: i64) -> Commit {
        Commit {
            timestamp: ts,
            username: "u".into(),
            files: vec![CommitFile {
                filename: "/f".into(),
                ..Default::default()
            }],
            is_shadow: false,
        }
    }

    #[test]
    fn fatal_messages_are_picked_out() {
        let reqs = [
            PlatformRequest::Quit,
            PlatformRequest::Fatal("a".into()),
            PlatformRequest::Fatal("b".into()),
        ];
        assert_eq!(fatal_messages(&reqs), ["a", "b"]);
        assert!(fatal_messages(&[PlatformRequest::Quit]).is_empty());
    }

    #[test]
    fn waits_for_the_first_commits_of_an_empty_live_stream() {
        let feed = CommitFeed::new();
        let mut app = app(&feed);
        let mut rs = RemoteStream::new(feed.clone(), StringHasher::default());
        // Frames before anything has arrived: waiting, not failing.
        let reqs = run(&mut app, 30);
        assert!(fatal_messages(&reqs).is_empty(), "{reqs:?}");
        // An empty live history is still waiting.
        rs.push(&wire(true, &[])).unwrap();
        let reqs = run(&mut app, 30);
        assert!(fatal_messages(&reqs).is_empty(), "{reqs:?}");
        // The first commit starts the visualisation.
        let mut enc = Encoder::new(|_: &str| [1.0; 3]);
        let mut more = Vec::new();
        enc.commit(&commit(1_600_000_000), &mut more);
        rs.push(&more).unwrap();
        let reqs = run(&mut app, 60);
        assert!(fatal_messages(&reqs).is_empty(), "{reqs:?}");
        let gource = app.shell().gource.as_ref().unwrap();
        assert!(!gource.first_read && gource.commit_cursor > 0);
    }

    #[test]
    fn an_ended_empty_stream_is_a_surfaced_fatal() {
        let feed = CommitFeed::new();
        let mut app = app(&feed);
        let mut rs = RemoteStream::new(feed, StringHasher::default());
        rs.push(&wire(false, &[])).unwrap();
        let reqs = run(&mut app, 30);
        assert_eq!(fatal_messages(&reqs), ["no commits found"], "{reqs:?}");
    }
}

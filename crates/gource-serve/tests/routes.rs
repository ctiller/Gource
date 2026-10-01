//! The routes, end to end through the axum router.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use gource_core::StringHasher;
use gource_model::wire::{Decoder, Event, FilterSpec};
use gource_serve::{History, ServeOptions, router};
use gource_vcs::commit::file_colour;
use gource_vcs::{Commit, CommitFile};
use http_body_util::BodyExt;
use std::sync::Arc;
use tower::ServiceExt;

fn history() -> Arc<History> {
    let commit = |ts, user: &str, file: &str| Commit {
        timestamp: ts,
        username: user.into(),
        files: vec![CommitFile {
            filename: file.into(),
            colour: file_colour(file, &StringHasher::default()),
            ..Default::default()
        }],
        is_shadow: false,
    };
    History::new(
        vec![
            commit(1, "alice", "/src/a.rs"),
            commit(2, "bob", "/doc/b.md"),
        ],
        false,
        StringHasher::default(),
    )
}

async fn get(app: axum::Router, uri: &str) -> (StatusCode, Option<String>, Vec<u8>) {
    let resp = app
        .oneshot(Request::get(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = resp.status();
    let ctype = resp
        .headers()
        .get("content-type")
        .map(|v| v.to_str().unwrap().to_string());
    let body = resp
        .into_body()
        .collect()
        .await
        .unwrap()
        .to_bytes()
        .to_vec();
    (status, ctype, body)
}

fn decode(bytes: &[u8]) -> Vec<Event> {
    let hasher = StringHasher::default();
    let mut d = Decoder::new(move |p: &str| file_colour(p, &hasher));
    d.push(bytes);
    std::iter::from_fn(|| d.next_event().map(|e| e.unwrap())).collect()
}

#[tokio::test]
async fn stream_applies_query_filters() {
    let spec = FilterSpec {
        file_show_filters: vec!["^src/".into()],
        ..Default::default()
    };
    let uri = format!("/stream?from=0&{}", spec.to_query());
    let (status, ctype, body) = get(router(history(), &ServeOptions::default()), &uri).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(ctype.as_deref(), Some("application/x-gource-stream"));
    let events = decode(&body);
    assert_eq!(events.len(), 3, "{events:?}");
    assert!(matches!(&events[1], Event::Commit(c) if c.username == "alice"));
    assert_eq!(events[2], Event::EndOfHistory);
}

#[tokio::test]
async fn bad_queries_are_rejected() {
    let app = router(history(), &ServeOptions::default());
    let (status, _, body) = get(app.clone(), "/stream?from=x").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(String::from_utf8(body).unwrap().contains("invalid from"));
    let (status, _, _) = get(app.clone(), "/stream?us=%28").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _, _) = get(app, "/stream?us=%G").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn info_reports_the_history() {
    let (status, ctype, body) = get(router(history(), &ServeOptions::default()), "/info").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(ctype.as_deref(), Some("application/json"));
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["commits"], 2);
    assert_eq!(v["first_timestamp"], 1);
    assert_eq!(v["last_timestamp"], 2);
    assert_eq!(v["live"], false);
}

#[tokio::test]
async fn web_root_is_served() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("index.html"), "<p>hi</p>").unwrap();
    let opts = ServeOptions {
        web_root: Some(dir.path().into()),
    };
    let app = router(history(), &opts);
    let (status, _, body) = get(app.clone(), "/index.html").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, b"<p>hi</p>");
    let (status, _, _) = get(app, "/missing").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _, _) = get(router(history(), &ServeOptions::default()), "/x").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn live_stream_delivers_appended_commits() {
    let h = History::new(vec![], true, StringHasher::default());
    let app = router(h.clone(), &ServeOptions::default());
    let resp = app
        .oneshot(Request::get("/stream").body(Body::empty()).unwrap())
        .await
        .unwrap();
    let mut body = resp.into_body();
    let mut bytes = Vec::new();
    // Hello + EndOfHistory arrive before anything is appended.
    while decode(&bytes).len() < 2 {
        bytes.extend(body.frame().await.unwrap().unwrap().into_data().unwrap());
    }
    h.append([Commit {
        timestamp: 9,
        username: "c".into(),
        files: vec![CommitFile {
            filename: "/x".into(),
            colour: [1.0; 3],
            ..Default::default()
        }],
        is_shadow: false,
    }]);
    bytes.extend(body.frame().await.unwrap().unwrap().into_data().unwrap());
    let events = decode(&bytes);
    assert!(matches!(events.last(), Some(Event::Commit(c)) if c.timestamp == 9));
}

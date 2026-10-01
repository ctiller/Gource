//! HTTP routes.

use crate::filter::Filter;
use crate::store::History;
use crate::stream::{Chunk, StreamWriter};
use axum::Router;
use axum::body::{Body, Bytes};
use axum::extract::{RawQuery, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use gource_model::wire::{FilterSpec, VERSION};
use std::convert::Infallible;
use std::path::PathBuf;
use std::sync::Arc;

/// Target size of one body chunk.
const CHUNK_BYTES: usize = 64 << 10;

#[derive(Debug, Clone, Default)]
pub struct ServeOptions {
    /// Directory served at `/` (the web client), if any.
    pub web_root: Option<PathBuf>,
}

/// `GET /stream`, `GET /info`, and the web root.
pub fn router(history: Arc<History>, options: &ServeOptions) -> Router {
    let router = Router::new()
        .route("/stream", get(stream))
        .route("/info", get(info))
        .with_state(history);
    match &options.web_root {
        Some(root) => router.fallback_service(tower_http::services::ServeDir::new(root)),
        None => router,
    }
}

fn bad_request(msg: String) -> Response {
    (StatusCode::BAD_REQUEST, msg).into_response()
}

/// The `from` query parameter (default: everything).
fn parse_from(query: &str) -> Result<i64, String> {
    for pair in query.split('&') {
        if let Some(v) = pair.strip_prefix("from=") {
            return v.parse().map_err(|_| format!("invalid from '{v}'"));
        }
    }
    Ok(i64::MIN)
}

async fn stream(State(history): State<Arc<History>>, RawQuery(query): RawQuery) -> Response {
    let query = query.unwrap_or_default();
    let from = match parse_from(&query) {
        Ok(from) => from,
        Err(e) => return bad_request(e),
    };
    let filter = match FilterSpec::from_query(&query)
        .map_err(|e| e.to_string())
        .and_then(|spec| Filter::compile(&spec))
    {
        Ok(f) => f,
        Err(e) => return bad_request(e),
    };
    let rx = history.subscribe();
    let writer = StreamWriter::new(history, filter, from);
    let body = futures_util::stream::unfold((writer, rx), |(mut writer, mut rx)| async move {
        loop {
            match writer.next_chunk(CHUNK_BYTES) {
                Chunk::Data(bytes) => {
                    return Some((Ok::<_, Infallible>(Bytes::from(bytes)), (writer, rx)));
                }
                // The writer holds the History, so the sender outlives `rx`.
                Chunk::Wait => rx.changed().await.ok()?,
                Chunk::Done => return None,
            }
        }
    });
    (
        [
            (header::CONTENT_TYPE, "application/x-gource-stream"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        Body::from_stream(body),
    )
        .into_response()
}

async fn info(State(history): State<Arc<History>>) -> Response {
    let (first, last) = history.span().unwrap_or((0, 0));
    let json = serde_json::json!({
        "version": VERSION,
        "commits": history.len(),
        "first_timestamp": first,
        "last_timestamp": last,
        "live": history.is_live(),
    });
    (
        [(header::CONTENT_TYPE, "application/json")],
        json.to_string(),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_parameter() {
        assert_eq!(parse_from(""), Ok(i64::MIN));
        assert_eq!(parse_from("ff=x&from=-5"), Ok(-5));
        assert!(parse_from("from=abc").is_err());
    }
}

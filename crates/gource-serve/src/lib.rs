//! Serves a Gource commit history to remote clients.
//!
//! A [`History`] holds the full, unfiltered commit list of one repository
//! (loaded from a log or repository, and appended to while live). Clients
//! subscribe with `GET /stream?from=<ts>&ff=..&fs=..&uf=..&us=..`: the
//! server applies the [`FilterSpec`](gource_model::wire::FilterSpec), and
//! answers with a chunked body of length-prefixed wire frames
//! ([`gource_model::wire`]): `Hello`, the matching commits, `EndOfHistory`,
//! then (live repositories only) new commits as they arrive. Seeking to an
//! earlier point or changing a filter is a new request.

pub mod cli;
pub mod filter;
pub mod http;
pub mod store;
pub mod stream;

pub use filter::Filter;
pub use http::{ServeOptions, router};
pub use store::{History, load_history};
pub use stream::StreamWriter;

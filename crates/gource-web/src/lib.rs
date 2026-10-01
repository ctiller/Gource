//! Gource in the browser.
//!
//! The page fetches `GET /stream` from a `gource-serve`, decodes the wire
//! frames ([`remote::RemoteStream`]) into a [`gource_vcs::CommitFeed`], and
//! runs the ordinary [`gource_app::app::GourceApp`] on it, drawing each
//! frame's [`gource_draw::DrawList`] with the WebGL2 backend.
//!
//! [`remote`] and [`input`] are platform-independent (and tested natively);
//! the browser glue in `web` only exists on wasm32.

pub mod input;
pub mod remote;

#[cfg(target_arch = "wasm32")]
mod web;

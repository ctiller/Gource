//! Gource's domain types: the data that crosses crate and process boundaries
//! (producers -> data store -> scene, and the wire protocol). Plain data, no
//! glam, no parsing.

#![forbid(unsafe_code)]

pub mod commit;
pub mod wire;

pub use commit::{Commit, CommitFile, FileAction};

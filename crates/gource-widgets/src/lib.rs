//! Gource HUD widgets.
//!
//! These are independent of the settings and simulation crates: everything a
//! widget needs (font ids, scale factors, colours, positions) is passed in
//! explicitly, and drawing goes into a [`gource_draw::DrawList`] via
//! [`gource_draw::Gfx`].

pub mod caption;
pub mod cursor;
pub mod dashboard;
pub mod key;
pub mod slider;
pub mod textbox;
pub mod timeline_bar;
pub mod tuning_panel;

//! Simulation-determinism probe: run a log headlessly at the fixed 60 Hz tick
//! and print `World::state_hash()` once per simulated second.
//!
//! The scene is integer-only, so the output must be identical on every
//! platform, thread count and target. Compare native against wasm:
//!
//! ```text
//! cargo run --release -p gource-sim --example scene_hash -- LOG > native.txt
//! cargo build --release -p gource-sim --example scene_hash \
//!     --target wasm32-wasip1 --no-default-features
//! wasmtime run --dir . target/wasm32-wasip1/release/examples/scene_hash.wasm \
//!     LOG > wasm.txt
//! cmp native.txt wasm.txt
//! ```
//!
//! Arguments are gource arguments (defaults: `--seconds-per-day 0.05
//! --auto-skip-seconds 0.1` are prepended). `SCENE_HASH_TICKS` caps the run
//! (default 3600 ticks = 60 s).

use gource_draw::DrawList;
use gource_settings::{CliAction, parse_command_line};
use gource_sim::app::{AppOptions, GourceApp};
use gource_sim::platform::Viewport;

fn main() {
    let mut args: Vec<String> = ["-1280x720", "--seconds-per-day", "0.05"]
        .iter()
        .chain(["--auto-skip-seconds", "0.1"].iter())
        .map(|s| s.to_string())
        .collect();
    args.extend(std::env::args().skip(1));
    let config = match parse_command_line(&args) {
        Ok(CliAction::Run(config)) => config,
        other => {
            eprintln!("expected gource arguments to run: {other:?}");
            std::process::exit(2);
        }
    };
    let max_ticks: u64 = std::env::var("SCENE_HASH_TICKS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(3600);

    let viewport = Viewport::new(1280, 720);
    let mut list = DrawList::new(glam::UVec2::new(1280, 720));
    let mut app = GourceApp::new(config, AppOptions::default()).unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(1);
    });

    let dt = 1.0 / 60.0;
    let mut last_reported = u64::MAX;
    // Bounded by frames so a stalled sim cannot spin forever.
    for _ in 0..max_ticks * 4 + 600 {
        app.frame(dt, viewport, &mut list);
        let _ = app.take_requests();
        if app.is_finished() {
            break;
        }
        let Some(g) = app.shell().gource.as_ref() else {
            continue;
        };
        let w = &g.world;
        // Tick 0 is skipped: whether a frame passes before the log loads
        // depends on the loader (a thread natively, inline on wasm).
        if w.tick > 0 && w.tick % 60 == 0 && w.tick != last_reported {
            last_reported = w.tick;
            println!(
                "{} {:016x} dirs={} files={} users={}",
                w.tick,
                w.state_hash(),
                w.dirs.len(),
                w.files.len(),
                w.users.len()
            );
        }
        if w.tick >= max_ticks {
            break;
        }
    }
}

//! Print the simulation state of every frame, for frame-by-frame comparison
//! with a C++ gource patched to print the same `TRACE` lines (user positions
//! as `TRACEU` lines with `GOURCE_TRACE_USERS` set, directories as `TRACED`
//! lines with `GOURCE_TRACE_DIRS` set).
//!
//! ```text
//! cargo run -p gource-app --example trace -- -1280x720 -o - [gource args] LOG > rust.trace
//! ```
//!
//! Runs headless, with the timing of a recording (`-o`): every frame advances
//! the fixed tick rate. Nothing is written to the `-o` output.

use gource_app::app::{AppOptions, GourceApp};
use gource_app::platform::{PlatformRequest, Viewport};
use gource_draw::DrawList;
use gource_settings::{CliAction, parse_command_line};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let config = match parse_command_line(&args) {
        Ok(CliAction::Run(config)) => config,
        other => {
            eprintln!("expected gource arguments to run: {other:?}");
            std::process::exit(2);
        }
    };
    let recording = !config.display.output_ppm_filename.is_empty();
    let (width, height) = (
        config.display.display_width as u32,
        config.display.display_height as u32,
    );
    let viewport = Viewport::new(width, height);
    let mut list = DrawList::new(glam::UVec2::new(width, height));
    let mut app = GourceApp::new(
        config,
        AppOptions {
            recording,
            ..Default::default()
        },
    )
    .unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(1);
    });
    let trace_users = std::env::var_os("GOURCE_TRACE_USERS").is_some();
    let trace_dirs = std::env::var_os("GOURCE_TRACE_DIRS").is_some();
    let trace_exact = std::env::var_os("GOURCE_TRACE_EXACT").is_some();

    for frame in 0.. {
        app.frame(1.0 / 60.0, viewport, &mut list);
        let requests = app.take_requests();
        let dumped = requests
            .iter()
            .any(|r| matches!(r, PlatformRequest::CaptureFrame));
        if let Some(g) = app.shell().gource.as_ref() {
            let cam = g.camera.pos();
            let b = &g.world.dir_bounds;
            println!(
                "TRACE {frame} loaded={} dump={} rt={:.4} cur={} last={} q={} date=[{}] \
                 cam={:.2},{:.2},{:.2} users={} files={} dirb={:.1},{:.1},{:.1},{:.1}",
                u8::from(g.commitlog.is_some()),
                u8::from(dumped),
                g.runtime,
                g.currtime,
                g.lasttime,
                g.commitqueue.len(),
                g.display_date,
                cam.x,
                cam.y,
                cam.z,
                g.world.users.len(),
                g.world.files.len(),
                b.min.x,
                b.min.y,
                b.max.x,
                b.max.y,
            );
            if trace_dirs {
                for (path, &id) in &g.world.dir_map {
                    let d = &g.world.dirs[id];
                    let p = d.pos();
                    println!(
                        "TRACED {frame} {path} vis={} pos={:.3},{:.3} r={:.3} pr={:.3} files={} depth={} vc={}",
                        u8::from(d.is_visible(&g.world.dirs)),
                        p.x,
                        p.y,
                        d.radius(),
                        d.parent_radius(),
                        d.files.len(),
                        d.depth(),
                        d.visible_count
                    );
                }
            }
            if trace_users {
                for (name, &id) in &g.world.users_by_name {
                    let u = &g.world.users[id];
                    let p = u.pawn.pos();
                    let acc = gource_app::view::from_ivec(u.sim.accel);
                    println!(
                        "TRACEU {frame} {name} {:.2} {:.2} acts={}/{} acc={:.4},{:.4} la={:.4} el={:.4}",
                        p.x,
                        p.y,
                        u.pending_action_count(),
                        u.action_count(),
                        acc.x,
                        acc.y,
                        u.last_action,
                        u.pawn.elapsed
                    );
                }
            }
            if trace_exact {
                // Raw f32 bits, for finding the first bit-level divergence
                // (names last: they may contain spaces).
                let b = |f: f32| format!("{:08x}", f.to_bits());
                for (path, &id) in &g.world.dir_map {
                    let d = &g.world.dirs[id];
                    println!(
                        "TRACEXD {frame} {} {} {} {} {} {} {path}",
                        b(d.pos.x),
                        b(d.pos.y),
                        b(d.spos.x),
                        b(d.spos.y),
                        b(d.dir_radius),
                        b(d.parent_radius),
                    );
                }
                for (path, &id) in &g.world.files_by_path {
                    let f = &g.world.files[id];
                    let dest = gource_app::view::from_unit(f.sim.dest);
                    println!(
                        "TRACEXF {frame} {} {} {} {} {} {path}",
                        b(f.pawn.pos.x),
                        b(f.pawn.pos.y),
                        b(dest.x),
                        b(dest.y),
                        b(gource_app::view::from_fx(f.sim.distance)),
                    );
                }
                for (name, &id) in &g.world.users_by_name {
                    let u = &g.world.users[id];
                    let acc = gource_app::view::from_ivec(u.sim.accel);
                    println!(
                        "TRACEXU {frame} {} {} {} {} {name}",
                        b(u.pawn.pos.x),
                        b(u.pawn.pos.y),
                        b(acc.x),
                        b(acc.y),
                    );
                }
            }
        }
        let quit = requests
            .iter()
            .any(|r| matches!(r, PlatformRequest::Quit | PlatformRequest::Fatal(_)));
        if quit || app.is_finished() {
            break;
        }
        if app
            .shell()
            .gource
            .as_ref()
            .is_some_and(|g| g.commitlog.is_none())
        {
            // The log loads on a thread: don't race through frames meanwhile.
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    }
}

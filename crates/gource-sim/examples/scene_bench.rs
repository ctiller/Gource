//! Scene cost harness: how expensive is the scene per frame, and how many
//! bytes would scene state take if it crossed a process or network boundary?
//!
//! ```text
//! CARGO_TARGET_DIR=target/bench cargo run --release -p gource-sim \
//!     --example scene_bench -- -1280x720 --seconds-per-day 0.05 --stop-at-end LOG
//! ```
//!
//! Runs headless at a fixed tick (default 60 Hz). `logic` (simulation,
//! physics, camera) and `draw` (tessellation into a [`DrawList`]) are timed
//! separately, and `logic` is broken down by phase
//! ([`gource_sim::profile::LogicSpan`]). Every `SCENE_BENCH_EVERY` frames
//! (default 30) a CSV row goes to stdout; summaries go to stderr.
//!
//! Environment:
//! - `SCENE_BENCH_MAX_FRAMES`: stop after this many frames.
//! - `SCENE_BENCH_DT`: tick in seconds (default 1/60), for determinism checks.
//! - `SCENE_BENCH_RUNTIME`: stop once simulated runtime reaches this (seconds).
//! - `SCENE_BENCH_DUMP`: write `kind path x y` for every dir, file (absolute)
//!   and user at the end, to compare layouts between runs.
//! - `SCENE_BENCH_INTERP=0`: skip the interpolation/tier analysis.
//!
//! Byte estimates (hypothetical wire formats, not implemented):
//! - raw `SceneFrame`: file 20 B, dir 16 B, user 16 B;
//! - quantized: 7 B per entity (delta-varint id, pos 2xi16, 1 B attr);
//! - delta: quantized, only entities that moved more than 0.25 world units.
//!   Files are hierarchical (position relative to their directory), so a
//!   file only needs an update when it moves within its directory.
//!
//! Tier/interpolation analysis (the "coarse physics on a server" split):
//! positions are sampled at 20/10/5 Hz; between samples the client linearly
//! interpolates. The error against the 60 Hz ground truth is converted to
//! screen pixels with the camera's distance (fov 90: px = err * h / 2|z|).

use std::collections::{HashMap, VecDeque};
use std::hash::Hash;
use std::io::Write;
use std::time::Instant;

use glam::Vec2;
use gource_draw::DrawList;
use gource_settings::{CliAction, parse_command_line};
use gource_sim::app::{AppOptions, GourceApp};
use gource_sim::file::{DirId, FileId};
use gource_sim::platform::Viewport;
use gource_sim::profile::LogicSpan;
use gource_sim::user::UserId;
use gource_sim::world::World;

const RAW_FILE: usize = 20;
const RAW_DIR: usize = 16;
const RAW_USER: usize = 16;
const QUANT: usize = 7;
const MOVE_EPS: f32 = 0.25;
/// Sample strides at 60 Hz: 20, 10 and 5 Hz.
const STRIDES: [usize; 3] = [3, 6, 12];
const NSPAN: usize = LogicSpan::ALL.len();

struct Row {
    frame: usize,
    files: usize,
    dirs: usize,
    users: usize,
    logic_ms: f64,
    draw_ms: f64,
    spans: [f64; NSPAN],
    verts: usize,
    moved: usize,
}

impl Row {
    fn entities(&self) -> usize {
        self.files + self.dirs + self.users
    }
    fn raw_bytes(&self) -> usize {
        self.files * RAW_FILE + self.dirs * RAW_DIR + self.users * RAW_USER
    }
    fn quant_bytes(&self) -> usize {
        self.entities() * QUANT
    }
    fn delta_bytes(&self) -> usize {
        self.moved * QUANT
    }
}

/// One frame of visible entity positions.
#[derive(Default)]
struct Frame {
    /// Screen pixels per world unit.
    scale: f32,
    dirs: HashMap<DirId, Vec2>,
    users: HashMap<UserId, Vec2>,
    /// (absolute, relative to dir).
    files: HashMap<FileId, (Vec2, Vec2)>,
}

impl Frame {
    fn capture(w: &World, cam_z: f32, height: u32) -> Frame {
        let dirs: HashMap<_, _> = w
            .dirs
            .iter()
            .filter(|(_, d)| d.is_visible(&w.dirs))
            .map(|(k, d)| (k, d.pos()))
            .collect();
        let files = w
            .files
            .iter()
            .filter(|(_, f)| !f.pawn.is_hidden())
            .map(|(k, f)| {
                let dp = f
                    .dir
                    .and_then(|d| w.dirs.get(d))
                    .map(|d| d.pos())
                    .unwrap_or(Vec2::ZERO);
                (k, (f.absolute_pos(dp), f.pawn.pos()))
            })
            .collect();
        let users = w
            .users
            .iter()
            .filter(|(_, u)| !u.pawn.is_hidden())
            .map(|(k, u)| (k, u.pawn.pos()))
            .collect();
        Frame {
            scale: height as f32 / (2.0 * cam_z.abs().max(1.0)),
            dirs,
            users,
            files,
        }
    }
}

/// Entities that moved (or appeared/disappeared) between two position maps.
fn moved<K: Hash + Eq, V>(a: &HashMap<K, V>, b: &HashMap<K, V>, pos: impl Fn(&V) -> Vec2) -> usize {
    let changed = b
        .iter()
        .filter(|(k, v)| a.get(k).is_none_or(|u| pos(u).distance(pos(v)) > MOVE_EPS))
        .count();
    changed + a.keys().filter(|k| !b.contains_key(k)).count()
}

/// Histogram of pixel errors in 0.05 px buckets up to 64 px.
#[derive(Clone)]
struct Hist {
    buckets: Vec<u64>,
    n: u64,
    max: f32,
}

impl Hist {
    const STEP: f32 = 0.05;
    fn new() -> Hist {
        Hist {
            buckets: vec![0; (64.0 / Self::STEP) as usize + 1],
            n: 0,
            max: 0.0,
        }
    }
    fn add(&mut self, v: f32) {
        let i = ((v / Self::STEP) as usize).min(self.buckets.len() - 1);
        self.buckets[i] += 1;
        self.n += 1;
        self.max = self.max.max(v);
    }
    fn pct(&self, p: f64) -> f32 {
        let target = (self.n as f64 * p).ceil() as u64;
        let mut acc = 0;
        for (i, &c) in self.buckets.iter().enumerate() {
            acc += c;
            if acc >= target.max(1) {
                return (i as f32 + 1.0) * Self::STEP;
            }
        }
        self.max
    }
}

/// Per-stride interpolation error and tier bandwidth accumulators.
struct TierStats {
    stride: usize,
    err: [Hist; 3], // dirs, users, files (absolute)
    samples: usize,
    moved: [usize; 3], // dirs, users, files (relative)
    present: [usize; 3],
}

impl TierStats {
    fn new(stride: usize) -> TierStats {
        TierStats {
            stride,
            err: [Hist::new(), Hist::new(), Hist::new()],
            samples: 0,
            moved: [0; 3],
            present: [0; 3],
        }
    }

    /// `hist` holds recent frames, newest last; the newest is a sample frame.
    fn sample(&mut self, hist: &VecDeque<Frame>) {
        let s = self.stride;
        if hist.len() <= s {
            return;
        }
        let b = &hist[hist.len() - 1];
        let a = &hist[hist.len() - 1 - s];
        self.samples += 1;
        self.moved[0] += moved(&a.dirs, &b.dirs, |p| *p);
        self.moved[1] += moved(&a.users, &b.users, |p| *p);
        self.moved[2] += moved(&a.files, &b.files, |p| p.1);
        self.present[0] += b.dirs.len();
        self.present[1] += b.users.len();
        self.present[2] += b.files.len();
        for j in 1..s {
            let t = &hist[hist.len() - 1 - s + j];
            let f = j as f32 / s as f32;
            let lerp = |pa: Vec2, pb: Vec2| pa + (pb - pa) * f;
            for (k, &pt) in &t.dirs {
                if let (Some(&pa), Some(&pb)) = (a.dirs.get(k), b.dirs.get(k)) {
                    self.err[0].add(lerp(pa, pb).distance(pt) * t.scale);
                }
            }
            for (k, &pt) in &t.users {
                if let (Some(&pa), Some(&pb)) = (a.users.get(k), b.users.get(k)) {
                    self.err[1].add(lerp(pa, pb).distance(pt) * t.scale);
                }
            }
            for (k, &(pt, _)) in &t.files {
                if let (Some(&(pa, _)), Some(&(pb, _))) = (a.files.get(k), b.files.get(k)) {
                    self.err[2].add(lerp(pa, pb).distance(pt) * t.scale);
                }
            }
        }
    }
}

fn pct(v: &mut [f64], p: f64) -> f64 {
    if v.is_empty() {
        return 0.0;
    }
    v.sort_by(|a, b| a.total_cmp(b));
    v[((v.len() - 1) as f64 * p).round() as usize]
}

fn env<T: std::str::FromStr>(name: &str) -> Option<T> {
    std::env::var(name).ok().and_then(|s| s.parse().ok())
}

fn dump(path: &str, w: &World) {
    let mut out = std::io::BufWriter::new(std::fs::File::create(path).expect("dump file"));
    for (p, &id) in &w.dir_map {
        let v = w.dirs[id].pos();
        writeln!(out, "D {} {} {p}", v.x, v.y).unwrap();
    }
    for (p, &id) in &w.files_by_path {
        let f = &w.files[id];
        let dp = f
            .dir
            .and_then(|d| w.dirs.get(d))
            .map(|d| d.pos())
            .unwrap_or(Vec2::ZERO);
        let v = f.absolute_pos(dp);
        writeln!(out, "F {} {} {p}", v.x, v.y).unwrap();
    }
    for (n, &id) in &w.users_by_name {
        let v = w.users[id].pawn.pos();
        writeln!(out, "U {} {} {n}", v.x, v.y).unwrap();
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let config = match parse_command_line(&args) {
        Ok(CliAction::Run(config)) => config,
        other => {
            eprintln!("expected gource arguments to run: {other:?}");
            std::process::exit(2);
        }
    };
    let every: usize = env("SCENE_BENCH_EVERY").unwrap_or(30);
    let max_frames: usize = env("SCENE_BENCH_MAX_FRAMES").unwrap_or(usize::MAX);
    let dt: f32 = env("SCENE_BENCH_DT").unwrap_or(1.0 / 60.0);
    let max_runtime: f32 = env("SCENE_BENCH_RUNTIME").unwrap_or(f32::INFINITY);
    let interp = std::env::var("SCENE_BENCH_INTERP").map_or(true, |v| v != "0");
    let dump_path = std::env::var("SCENE_BENCH_DUMP").ok();
    let (width, height) = (
        config.display.display_width as u32,
        config.display.display_height as u32,
    );
    let viewport = Viewport::new(width, height);
    let mut list = DrawList::new(glam::UVec2::new(width, height));
    let mut app = GourceApp::new(config, AppOptions::default()).unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(1);
    });

    // Let the shell create the Gource and load the log on its thread.
    let load_start = Instant::now();
    loop {
        app.frame(dt, viewport, &mut list);
        let _ = app.take_requests();
        if app.is_finished() {
            eprintln!("finished before the log loaded");
            return;
        }
        if app
            .shell()
            .gource
            .as_ref()
            .is_some_and(|g| g.commitlog.is_some())
        {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    eprintln!("log loaded in {:.2}s", load_start.elapsed().as_secs_f64());

    let mut rows: Vec<Row> = Vec::new();
    let mut hist: VecDeque<Frame> = VecDeque::new();
    let mut tiers: Vec<TierStats> = STRIDES.iter().map(|&s| TierStats::new(s)).collect();
    let wall = Instant::now();
    println!("frame,files,dirs,users,logic_ms,draw_ms,verts,raw_B,quant_B,delta_B,moved_frac");

    for frame in 0..max_frames {
        let shell = app.shell_mut();
        let gfx = &mut shell.gfx;
        let Some(g) = shell.gource.as_mut() else {
            break;
        };
        if g.is_finished || g.runtime >= max_runtime {
            break;
        }
        // Mirrors `Gource::update`, with logic and draw timed separately.
        let scaled_dt = dt.min(g.max_tick_rate) * g.settings.time_scale;
        // `logic` advances `runtime` per fixed tick (and checks stop_at_time).
        let t0 = Instant::now();
        if let Err(e) = g.logic(scaled_dt, viewport, gfx) {
            eprintln!("logic error: {}", e.0);
            break;
        }
        let t1 = Instant::now();
        g.draw(scaled_dt, viewport, gfx, &mut list);
        let t2 = Instant::now();
        g.framecount += 1;
        g.pending_requests.clear();

        let cur = Frame::capture(&g.world, g.camera.pos().z, height);
        let moved_now =
            hist.back()
                .map_or(cur.dirs.len() + cur.users.len() + cur.files.len(), |p| {
                    moved(&p.dirs, &cur.dirs, |v| *v)
                        + moved(&p.users, &cur.users, |v| *v)
                        + moved(&p.files, &cur.files, |v| v.1)
                });
        let row = Row {
            frame,
            files: cur.files.len(),
            dirs: cur.dirs.len(),
            users: cur.users.len(),
            logic_ms: (t1 - t0).as_secs_f64() * 1e3,
            draw_ms: (t2 - t1).as_secs_f64() * 1e3,
            spans: g.logic_profile.ms,
            verts: list.vertex_count(),
            moved: moved_now,
        };
        hist.push_back(cur);
        if hist.len() > STRIDES[STRIDES.len() - 1] + 1 {
            hist.pop_front();
        }
        if interp {
            for t in &mut tiers {
                if frame % t.stride == 0 {
                    t.sample(&hist);
                }
            }
        }
        if frame % every == 0 {
            println!(
                "{},{},{},{},{:.3},{:.3},{},{},{},{},{:.3}",
                row.frame,
                row.files,
                row.dirs,
                row.users,
                row.logic_ms,
                row.draw_ms,
                row.verts,
                row.raw_bytes(),
                row.quant_bytes(),
                row.delta_bytes(),
                row.moved as f64 / row.entities().max(1) as f64
            );
        }
        rows.push(row);
    }

    if let (Some(path), Some(g)) = (&dump_path, app.shell().gource.as_ref()) {
        dump(path, &g.world);
        eprintln!("dumped layout at runtime {:.3}s to {path}", g.runtime);
    }

    let n = rows.len().max(1) as f64;
    eprintln!(
        "{} frames in {:.1}s wall; mean logic {:.3} ms, draw {:.3} ms",
        rows.len(),
        wall.elapsed().as_secs_f64(),
        rows.iter().map(|r| r.logic_ms).sum::<f64>() / n,
        rows.iter().map(|r| r.draw_ms).sum::<f64>() / n
    );
    eprintln!(
        "{:>12} {:>6} {:>9} {:>9} {:>9} {:>9} {:>9} {:>9} {:>9} {:>9} {:>6}",
        "entities",
        "frames",
        "logic50",
        "logic95",
        "draw50",
        "draw95",
        "verts",
        "raw_Mb/s",
        "q_Mb/s",
        "dq_Mb/s",
        "moved"
    );
    let bins = [
        0, 100, 500, 1_000, 2_500, 5_000, 10_000, 25_000, 50_000, 100_000,
    ];
    let binned = |lo: usize, i: usize| -> Vec<&Row> {
        let hi = bins.get(i + 1).copied().unwrap_or(usize::MAX);
        rows.iter()
            .filter(|r| (lo..hi).contains(&r.entities()))
            .collect()
    };
    let label = |lo: usize, i: usize| {
        bins.get(i + 1)
            .map_or(format!("{lo}-inf"), |hi| format!("{lo}-{hi}"))
    };
    for (i, &lo) in bins.iter().enumerate() {
        let sel = binned(lo, i);
        if sel.is_empty() {
            continue;
        }
        let k = sel.len() as f64;
        let mut logic: Vec<f64> = sel.iter().map(|r| r.logic_ms).collect();
        let mut draw: Vec<f64> = sel.iter().map(|r| r.draw_ms).collect();
        let mean = |f: &dyn Fn(&Row) -> f64| sel.iter().map(|r| f(r)).sum::<f64>() / k;
        // Mbit/s at 60 frames per second.
        let mbps = |b: f64| b * 8.0 * 60.0 / 1e6;
        eprintln!(
            "{:>12} {:>6} {:>9.3} {:>9.3} {:>9.3} {:>9.3} {:>9.0} {:>9.2} {:>9.2} {:>9.2} {:>6.3}",
            label(lo, i),
            sel.len(),
            pct(&mut logic, 0.5),
            pct(&mut logic, 0.95),
            pct(&mut draw, 0.5),
            pct(&mut draw, 0.95),
            mean(&|r| r.verts as f64),
            mbps(mean(&|r| r.raw_bytes() as f64)),
            mbps(mean(&|r| r.quant_bytes() as f64)),
            mbps(mean(&|r| r.delta_bytes() as f64)),
            mean(&|r| r.moved as f64 / r.entities().max(1) as f64),
        );
    }

    // Logic breakdown: mean ms per span, per entity bin.
    eprintln!("\nlogic breakdown (mean ms per frame):");
    let mut header = format!("{:>12}", "entities");
    for s in LogicSpan::ALL {
        header += &format!(" {:>9}", s.name());
    }
    eprintln!("{header}");
    for (i, &lo) in bins.iter().enumerate() {
        let sel = binned(lo, i);
        if sel.is_empty() {
            continue;
        }
        let mut line = format!("{:>12}", label(lo, i));
        for s in LogicSpan::ALL {
            let m = sel.iter().map(|r| r.spans[s as usize]).sum::<f64>() / sel.len() as f64;
            line += &format!(" {m:>9.3}");
        }
        eprintln!("{line}");
    }

    if interp {
        eprintln!("\ntiers: linear interpolation error (px p50/p95/p99/max) and delta bandwidth");
        eprintln!(
            "{:>6} {:>28} {:>28} {:>28} {:>10} {:>10} {:>10}",
            "rate", "dirs err", "users err", "files err", "dirs_kb/s", "users_kb/s", "files_kb/s"
        );
        for t in &tiers {
            let hz = 60 / t.stride;
            let e = |h: &Hist| {
                format!(
                    "{:.2}/{:.2}/{:.2}/{:.1}",
                    h.pct(0.5),
                    h.pct(0.95),
                    h.pct(0.99),
                    h.max
                )
            };
            // Quantized bytes per second for each tier at this rate.
            let kbps = |m: usize| {
                if t.samples == 0 {
                    0.0
                } else {
                    m as f64 / t.samples as f64 * QUANT as f64 * hz as f64 * 8.0 / 1e3
                }
            };
            eprintln!(
                "{:>4}Hz {:>28} {:>28} {:>28} {:>10.1} {:>10.1} {:>10.1}",
                hz,
                e(&t.err[0]),
                e(&t.err[1]),
                e(&t.err[2]),
                kbps(t.moved[0]),
                kbps(t.moved[1]),
                kbps(t.moved[2]),
            );
        }
        if let Some(t) = tiers.first()
            && t.samples > 0
        {
            eprintln!(
                "mean present per sample: dirs {:.0}, users {:.0}, files {:.0}",
                t.present[0] as f64 / t.samples as f64,
                t.present[1] as f64 / t.samples as f64,
                t.present[2] as f64 / t.samples as f64
            );
        }
    }
}

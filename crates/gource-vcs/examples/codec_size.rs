//! Event codec sizing: how many bytes does a commit history take on the wire
//! (data store -> scene, the "L1" interface), and how big is a seek snapshot?
//!
//! ```text
//! CARGO_TARGET_DIR=target/bench cargo run --release -p gource-vcs \
//!     --example codec_size -- CUSTOM_LOG [OUT_DIR]
//! ```
//!
//! Input is a Gource custom log (`timestamp|user|A/M/D|/path`). Encodings:
//! - `json`: one NDJSON object per file change (`{"t":..,"u":..,"a":..,"p":..}`);
//! - `bin`: commits grouped by (timestamp, user); varint/zigzag timestamp
//!   deltas; users and paths interned. A path is interned as a tree: each
//!   new directory or file node is sent once as (parent node id, name), and a
//!   change is a single varint `node << 2 | action`.
//!
//! Snapshots at 10/50/90% of the changes hold the live file set (each file:
//! node id, last user, last-modified time) plus the node and user tables, in
//! JSON and binary. Every output is written to OUT_DIR (default a temp dir)
//! and compressed with the `gzip` and `zstd` command-line tools if present.

use std::collections::{BTreeMap, HashMap};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::Command;

struct Change {
    ts: i64,
    user: String,
    action: u8,
    path: String,
}

fn varint(out: &mut Vec<u8>, mut v: u64) {
    while v >= 0x80 {
        out.push((v as u8) | 0x80);
        v >>= 7;
    }
    out.push(v as u8);
}

fn zigzag(v: i64) -> u64 {
    ((v << 1) ^ (v >> 63)) as u64
}

fn bytes(out: &mut Vec<u8>, s: &str) {
    varint(out, s.len() as u64);
    out.extend_from_slice(s.as_bytes());
}

/// Interns path components as a tree of nodes; emits new nodes into `table`.
#[derive(Default)]
struct PathTree {
    nodes: HashMap<(u32, String), u32>,
    names: Vec<(u32, String)>,
}

impl PathTree {
    fn intern(&mut self, path: &str, table: &mut Vec<u8>) -> u32 {
        let mut parent = 0u32; // 0 = root
        for comp in path.split('/').filter(|c| !c.is_empty()) {
            let key = (parent, comp.to_string());
            parent = match self.nodes.get(&key) {
                Some(&id) => id,
                None => {
                    let id = self.names.len() as u32 + 1;
                    varint(table, u64::from(key.0));
                    bytes(table, comp);
                    self.names.push(key.clone());
                    self.nodes.insert(key, id);
                    id
                }
            };
        }
        parent
    }
}

#[derive(Default)]
struct Interner {
    ids: HashMap<String, u32>,
    names: Vec<String>,
}

impl Interner {
    /// Returns (id, is_new).
    fn intern(&mut self, s: &str) -> (u32, bool) {
        if let Some(&id) = self.ids.get(s) {
            return (id, false);
        }
        let id = self.names.len() as u32;
        self.ids.insert(s.to_string(), id);
        self.names.push(s.to_string());
        (id, true)
    }
}

fn parse(path: &Path) -> Vec<Change> {
    let f = std::fs::File::open(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    BufReader::new(f)
        .lines()
        .map_while(Result::ok)
        .filter_map(|line| {
            let mut it = line.splitn(4, '|');
            let ts = it.next()?.trim().parse().ok()?;
            let user = it.next()?.to_string();
            let action = it.next()?.bytes().next()?;
            let path = it.next()?.to_string();
            Some(Change {
                ts,
                user,
                action,
                path,
            })
        })
        .collect()
}

fn action_code(a: u8) -> u64 {
    match a {
        b'A' => 0,
        b'M' => 1,
        b'D' => 2,
        _ => 3,
    }
}

struct Encoded {
    json: Vec<u8>,
    bin: Vec<u8>,
    path_table: usize,
    user_table: usize,
    commits: usize,
}

fn encode_stream(changes: &[Change]) -> Encoded {
    let mut json = Vec::new();
    for c in changes {
        let v = serde_json::json!({
            "t": c.ts,
            "u": c.user,
            "a": (c.action as char).to_string(),
            "p": c.path,
        });
        serde_json::to_writer(&mut json, &v).unwrap();
        json.push(b'\n');
    }

    let mut tree = PathTree::default();
    let mut users = Interner::default();
    let mut bin = Vec::new();
    let (mut path_table, mut user_table, mut commits) = (0, 0, 0);
    let mut last_ts = 0i64;
    let mut i = 0;
    while i < changes.len() {
        let (ts, user) = (changes[i].ts, &changes[i].user);
        let mut j = i;
        while j < changes.len() && changes[j].ts == ts && &changes[j].user == user {
            j += 1;
        }
        commits += 1;
        // Commit header: ts delta, user (new users inline: id == count).
        varint(&mut bin, zigzag(ts - last_ts));
        last_ts = ts;
        let (uid, new_user) = users.intern(user);
        varint(&mut bin, u64::from(uid));
        if new_user {
            let before = bin.len();
            bytes(&mut bin, user);
            user_table += bin.len() - before;
        }
        // New path nodes for this commit, then the changes.
        let mut table = Vec::new();
        let ids: Vec<u32> = changes[i..j]
            .iter()
            .map(|c| tree.intern(&c.path, &mut table))
            .collect();
        varint(&mut bin, table.len() as u64);
        path_table += table.len();
        bin.extend_from_slice(&table);
        varint(&mut bin, (j - i) as u64);
        for (c, id) in changes[i..j].iter().zip(ids) {
            varint(&mut bin, (u64::from(id) << 2) | action_code(c.action));
        }
        i = j;
    }
    Encoded {
        json,
        bin,
        path_table,
        user_table,
        commits,
    }
}

struct Live {
    user: String,
    ts: i64,
}

/// Snapshot after applying `changes[..n]`: (json, bin).
fn snapshot(changes: &[Change], n: usize) -> (Vec<u8>, Vec<u8>, usize) {
    let mut live: BTreeMap<&str, Live> = BTreeMap::new();
    for c in &changes[..n] {
        if c.action == b'D' {
            live.remove(c.path.as_str());
        } else {
            live.insert(
                &c.path,
                Live {
                    user: c.user.clone(),
                    ts: c.ts,
                },
            );
        }
    }
    let files: Vec<serde_json::Value> = live
        .iter()
        .map(|(p, l)| serde_json::json!({"p": p, "u": l.user, "t": l.ts}))
        .collect();
    let json = serde_json::to_vec(&serde_json::json!({ "files": files })).unwrap();

    // Binary: node table (sorted paths share prefixes), user table, then
    // per file (node id delta, user id, ts delta from the snapshot time).
    let mut tree = PathTree::default();
    let mut users = Interner::default();
    let mut table = Vec::new();
    let mut recs = Vec::new();
    let now = changes[n.saturating_sub(1)].ts;
    let mut last_id = 0i64;
    for (p, l) in &live {
        let id = i64::from(tree.intern(p, &mut table));
        varint(&mut recs, zigzag(id - last_id));
        last_id = id;
        let (uid, _) = users.intern(&l.user);
        varint(&mut recs, u64::from(uid));
        varint(&mut recs, zigzag(now - l.ts));
    }
    let mut bin = Vec::new();
    varint(&mut bin, users.names.len() as u64);
    for u in &users.names {
        bytes(&mut bin, u);
    }
    varint(&mut bin, table.len() as u64);
    bin.extend_from_slice(&table);
    varint(&mut bin, live.len() as u64);
    bin.extend_from_slice(&recs);
    (json, bin, live.len())
}

/// Compressed sizes via CLI tools: (gzip -6, zstd -3, zstd -19).
fn compressed(path: &Path) -> [Option<u64>; 3] {
    let run = |cmd: &str, args: &[&str]| -> Option<u64> {
        let out = Command::new(cmd).args(args).arg(path).output().ok()?;
        out.status.success().then_some(out.stdout.len() as u64)
    };
    [
        run("gzip", &["-6", "-c"]),
        run("zstd", &["-3", "-c", "-q"]),
        run("zstd", &["-19", "-c", "-q", "--long=27"]),
    ]
}

fn report(dir: &Path, name: &str, data: &[u8], per: Option<(usize, &str)>) {
    let path = dir.join(name);
    std::fs::write(&path, data).unwrap();
    let c = compressed(&path);
    let fmt = |v: Option<u64>| v.map_or("-".to_string(), |b| b.to_string());
    let per_s = |b: Option<u64>| match (per, b) {
        (Some((n, _)), Some(b)) if n > 0 => format!("{:.2}", b as f64 / n as f64),
        _ => "-".into(),
    };
    println!(
        "{:<22} {:>12} {:>12} {:>12} {:>12}   per {}: {} / {} / {} / {}",
        name,
        data.len(),
        fmt(c[0]),
        fmt(c[1]),
        fmt(c[2]),
        per.map_or("-", |p| p.1),
        per_s(Some(data.len() as u64)),
        per_s(c[0]),
        per_s(c[1]),
        per_s(c[2]),
    );
}

fn main() {
    let mut args = std::env::args().skip(1);
    let Some(log) = args.next().map(PathBuf::from) else {
        eprintln!("usage: codec_size CUSTOM_LOG [OUT_DIR]");
        std::process::exit(2);
    };
    let tmp;
    let dir = match args.next() {
        Some(d) => PathBuf::from(d),
        None => {
            tmp = tempfile::tempdir().unwrap();
            tmp.path().to_path_buf()
        }
    };
    std::fs::create_dir_all(&dir).unwrap();

    let changes = parse(&log);
    if changes.is_empty() {
        eprintln!("no changes parsed");
        std::process::exit(1);
    }
    let e = encode_stream(&changes);
    let n = changes.len();
    let raw_log = std::fs::metadata(&log).map(|m| m.len()).unwrap_or(0);
    let span_days = (changes[n - 1].ts - changes[0].ts) as f64 / 86400.0;
    let mut users: Vec<&str> = changes.iter().map(|c| c.user.as_str()).collect();
    users.sort_unstable();
    users.dedup();
    println!(
        "{}: {} changes, {} commits, {} users, raw log {} B, span {:.0} days",
        log.display(),
        n,
        e.commits,
        users.len(),
        raw_log,
        span_days
    );
    println!(
        "bin: path table {} B ({:.1}% of bin), user table {} B, change records {:.2} B/change",
        e.path_table,
        100.0 * e.path_table as f64 / e.bin.len() as f64,
        e.user_table,
        (e.bin.len() - e.path_table - e.user_table) as f64 / n as f64
    );
    println!(
        "{:<22} {:>12} {:>12} {:>12} {:>12}   (raw / gzip-6 / zstd-3 / zstd-19)",
        "output", "raw_B", "gzip6_B", "zstd3_B", "zstd19_B"
    );
    report(&dir, "stream.json", &e.json, Some((n, "change")));
    report(&dir, "stream.bin", &e.bin, Some((n, "change")));
    for pct in [10, 50, 90, 100] {
        let k = (n * pct / 100).max(1);
        let (json, bin, files) = snapshot(&changes, k);
        println!("snapshot @{pct}%: {files} live files");
        report(
            &dir,
            &format!("snap{pct}.json"),
            &json,
            Some((files, "file")),
        );
        report(&dir, &format!("snap{pct}.bin"), &bin, Some((files, "file")));
    }
}

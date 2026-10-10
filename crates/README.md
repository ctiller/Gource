# Gource in Rust

This directory contains the Rust port of Gource. The C++ sources in `src/`
remain the behavioural reference until the port is complete.

## Architecture

```
gource (bin, Bevy)        windowed desktop frontend (Bevy + winit)
gource-cli (bin, wgpu)    Bevy-free headless CLI & offscreen exporter (--output-video / -o)
  └─ gource-render        plain-wgpu DrawList renderer (offscreen & surface, 4x MSAA)
gource-web (cdylib, wasm) browser client streaming from gource-serve, Canvas2D glyph rasteriser
  └─ gource-webgl         WebGL2 DrawList renderer over web-sys
gource-serve (bin, axum)  HTTP server streaming the L1 binary wire format (GET /stream, GET /info)
  └─ gource-app           the simulation & presenters: tree, files, users, actions, camera,
     │                    Gource state machine, multi-repo shell, MVC presenters.
     ├─ gource-scene      integer-only simulation (Q24.8 fixed point, World, step, view):
     │                    directory forces, file rings/packing, user movement
     ├─ gource-widgets    HUD widgets consuming gource-vm ViewModels into DrawList
     ├─ gource-vm         serialisable ViewModel structs for HUD & scene presenters
     ├─ gource-history    analytics, Git-of-Theseus cohorts, timeline scrubber
     ├─ gource-vcs        log parsers, repository detection, CommitFeed
     ├─ gource-model      canonical Commit / FileChange model & L1 binary wire codec
     ├─ gource-settings   single-descriptor-table CLI, config files, help text, man page
     ├─ gource-draw       DrawList, Projection, TextureStore, pluggable FontStore, PPM/Video
     └─ gource-core       Vec2/Vec3/Vec4/IVec2/UVec2, Bounds2D, QuadTree, StringHasher, utf8
```

Design rules:

* **Engine-agnostic core.** Everything except the `gource` binary is plain
  Rust with no Bevy dependency, deterministic and unit-testable. The
  simulation emits a backend-neutral `DrawList` (screen-pixel triangles with
  two materials, like egui's tessellation); the Bevy frontend only uploads
  textures and draws batches. Other frontends (headless video, web) can reuse
  everything below the binary.
* **No global mutable state.** The C++ globals (`gGourceSettings`,
  `gGourceDirMap`, `gStringHashSeed`, force constants, ...) become fields of
  the owning struct and are passed explicitly.
* **Arenas instead of pointers.** Directories, files and users live in
  `slotmap` arenas and refer to each other by id.
* **Preserve behaviour, not bits.** Same command line, config files and
  look as C++ Gource, but bit-exact layout parity is no longer a goal.
  Doc comments name the C++ function a piece derives from.
* **Integer simulation, float view.** Layout runs in `gource-scene` integers
  at a fixed 60 Hz tick (`gource-app/src/step.rs`), so it is bit-identical
  for any thread count and on wasm, and a tick can be undone exactly
  (reverse playback). Each frame `World::sync_view(alpha)` interpolates the
  integer state into the float fields the draw code reads; rotation is a
  view transform only. `--elasticity` is accepted but has no effect.
  `examples/scene_hash.rs` prints `World::state_hash()` per simulated
  second; native and `wasm32-wasip1` (wasmtime, `--no-default-features` to
  drop gix) output must be identical.
* **Rendering in gamma space.** Colours and texels are used as-is, and
  blending happens on gamma-encoded values like the original OpenGL renderer
  (Bevy's `CompositingSpace::Srgb`, non-sRGB textures).

## gource-serve

`gource-serve [OPTIONS] PATH` serves the commit history of one log file or
git repository (a working tree or a bare repository). PATH is read once at
start-up; with `--live` it is polled afterwards.

Routes:

* `GET /stream?from=<ts>&ff=..&fs=..&uf=..&us=..` is the commit stream: a
  chunked `application/x-gource-stream` body of L1 wire frames
  (`gource_model::wire`): `Hello`, the matching commits, `EndOfHistory`, and
  on a live server new commits as they arrive (the response then stays open).
  `from` is a unix timestamp (default: everything); `ff`/`fs`/`uf`/`us` are
  the file-hide, file-show, user-hide and user-show regular expressions
  (repeatable). Bad values answer 400.
* Wire version 2 (added file sizes). A client and server of different
  versions fail with `unsupported version N` instead of misreading frames, so
  deploy the server and the web client together.
* `GET /info` is JSON: `version` (the wire version), `commits` (the current count, growing while
  live), `first_timestamp`, `last_timestamp`, `live`.
* Anything else is served from `--web-root DIR` when given (the web client
  built by `crates/gource-web/build.sh`), otherwise 404.

Options (`--help` prints them):

| option | meaning |
|---|---|
| `--bind ADDR`, `--port N` | listen address (default `127.0.0.1:8080`) |
| `--web-root DIR` | directory served at `/` |
| `--log-format FORMAT` | force a log format instead of auto-detecting |
| `--live`, `--follow` | git repositories only. The history is marked live and polled; commits made later are appended and reach open streams without a restart |
| `--poll-interval SECS` | seconds between polls with `--live` (default 5) |
| `--all-branches` | git repositories only. The history is the union of the commits reachable from every local branch (`refs/heads`), each once, in time order. The default is `HEAD` only |
| `--no-stats` | skip the per-file line counts and sizes (faster on huge histories); without them clients have no data for `--file-size-metric lines/diff/churn/size` |

Polling failures are logged to stderr and retried at the next poll; the log
level is `GOURCE_SERVE_LOG=error|warn|info|debug|off` (default `info`).

With `--live` the server compares the branch tips (`HEAD`, or all local
branches with `--all-branches`) on every poll and appends exactly the commits
not reachable from the tips already served (a bounded set: tips git can no
longer find, e.g. after a force-push and `gc --prune=now`, are dropped, and
tips reachable from others are folded away), so a bare repository works (push to it)
and so does a clone that is `git fetch`ed by something else. Appended commits
are not re-sorted into earlier history; they arrive in the order found, oldest
first. Remote-tracking branches are not followed.

The stream carries, for git, per-file `lines_added`/`lines_removed`
(`--numstat`) and the size in bytes of the file after the commit (`byte_size`,
from the blob; `fflags` bit 5, wire version 2), which is what the web
client's `--file-size-metric` and `--file-colour-mode churn` use.

## Working on a crate (parallel development rules)

Several people/agents work on different crates at the same time:

* Only edit files inside the crate you own. Never edit another crate, the
  workspace `Cargo.toml` or `Cargo.lock`. Do not add dependencies; if one is
  truly missing, ask. Available workspace dependencies: `log`,
  `thiserror`, `anyhow`, `regex`, `fancy-regex`, `chrono`, `roxmltree`,
  `tempfile`, `image`, `ab_glyph`, `slotmap`, `fastrand` (only those listed in
  your crate's `Cargo.toml` are usable without asking).
* Public items already declared in the skeleton are an interface other crates
  code against: implement them, don't rename or remove them, don't change
  signatures. You may add items.
* Use your own target directory to avoid lock contention:
  `export CARGO_TARGET_DIR=target/<crate-name>`.
* Keep your crate compiling: work in small steps and run
  `cargo check -p <crate>` after each change. If a *dependency* crate fails to
  compile, it's someone else's work in progress: wait a minute and retry.
* Before finishing: `cargo fmt -p <crate>`,
  `cargo clippy -p <crate> --all-targets -- -D warnings`, `cargo test -p <crate>`.

## Testing

* Unit tests live next to the code (`#[cfg(test)] mod tests`), integration
  tests in `crates/<crate>/tests/`. Test data goes in
  `crates/<crate>/tests/data/`.
* Line coverage target: **95%+ per crate**, measured with
  `cargo llvm-cov -p <crate> --summary-only`, excluding other workspace crates
  from the report with `--ignore-filename-regex`, e.g. for gource-vcs:
  `cargo llvm-cov -p gource-vcs --summary-only --ignore-filename-regex 'crates/gource-(core|draw|settings|sim|widgets)/'`.
* Prefer tests that pin down C++ behaviour (golden values computed from the
  C++ code by hand or with the reference build) over tests that merely execute
  code.
* Golden outputs (help text, log commands, custom logs, saved configs,
  parser results) come from a reference build of the C++ Gource in `src/`.
  The generator scripts in `crates/*/tests/tools/` read the path of the
  reference binary from `GOURCE_REF`, e.g.
  `GOURCE_REF=/path/to/gource crates/gource-vcs/tests/tools/gen_parity.sh`.

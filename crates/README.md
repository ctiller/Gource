# Gource in Rust

This directory contains the Rust port of Gource. The C++ sources in `src/`
remain the behavioural reference until the port is complete.

## Architecture

```
gource (bin, Bevy)        window, input, GPU upload of draw lists, PPM/PNG capture
  └─ gource-sim           the simulation: tree, files, users, actions, camera,
     │                    Gource state machine, multi-repo shell. Tessellates
     │                    each frame into a DrawList.
     ├─ gource-widgets    HUD widgets: file key, captions, slider, tooltip, cursor
     ├─ gource-vcs        commit model, log parsers, repository detection
     ├─ gource-settings   command line, config files, defaults, help text
     ├─ gource-draw       DrawList, Projection, TextureStore, FontStore, PPM writer
     └─ gource-core       Bounds2D, QuadTree, StringHasher, datetime, utf8, math
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
* **Preserve behaviour.** The goal of the first milestone is a faithful port:
  same command line, same config files, same layout physics, same look.
  Keep C++ arithmetic (f32, integer truncation, iteration order of
  `std::map` → `BTreeMap`) where it affects output. Doc comments name the C++
  function being ported.
* **Rendering in gamma space.** Colours and texels are used as-is, and
  blending happens on gamma-encoded values like the original OpenGL renderer
  (Bevy's `CompositingSpace::Srgb`, non-sRGB textures).

## Working on a crate (parallel development rules)

Several people/agents work on different crates at the same time:

* Only edit files inside the crate you own. Never edit another crate, the
  workspace `Cargo.toml` or `Cargo.lock`. Do not add dependencies; if one is
  truly missing, ask. Available workspace dependencies: `glam`, `log`,
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

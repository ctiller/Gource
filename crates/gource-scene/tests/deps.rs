//! Architectural dependency lint for `gource-scene`.
//!
//! Asserts that `gource-scene` does not depend on UI, rendering, VCS, or history crates.

use std::fs;
use std::path::Path;

#[test]
fn scene_does_not_depend_on_unwanted_crates() {
    let manifest_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
    let manifest = fs::read_to_string(&manifest_path)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", manifest_path.display()));

    let forbidden = [
        "gource-draw",
        "gource-widgets",
        "gource-vcs",
        "gource-history",
        "gource-app",
        "gource-sim",
    ];

    for crate_name in forbidden {
        assert!(
            !manifest.contains(crate_name),
            "gource-scene Cargo.toml must not depend on `{crate_name}`"
        );
    }
}

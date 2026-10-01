#!/bin/sh
# Build the web client into crates/gource-web/www/pkg, then serve it with:
#   gource-serve --web-root crates/gource-web/www LOG_OR_REPO
# Needs: rustup target add wasm32-unknown-unknown; cargo install
# wasm-bindgen-cli --version <the wasm-bindgen version in Cargo.lock>.
set -eu
cd "$(dirname "$0")/../.."
target="${CARGO_TARGET_DIR:-target}"
cargo build --release -p gource-web --target wasm32-unknown-unknown
wasm-bindgen --target web --no-typescript \
    --out-dir crates/gource-web/www/pkg \
    "$target/wasm32-unknown-unknown/release/gource_web.wasm"
ls -la crates/gource-web/www/pkg

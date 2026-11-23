#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
cargo build --locked -p simile-wasm --target wasm32-unknown-unknown
wasm-bindgen --target web --out-dir crates/simile-wasm/pkg target/wasm32-unknown-unknown/debug/simile_wasm.wasm
npm --prefix crates/simile-wasm ci --ignore-scripts --no-audit --no-fund
npm --prefix crates/simile-wasm run build
npm --prefix crates/simile-wasm test

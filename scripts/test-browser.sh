#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

# Install wasm-bindgen-cli 0.2.100 and a matching Chrome/Chromedriver first.
# The explicit runner keeps the harness independent of a UI bundler or wasm-pack.
export CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUNNER=wasm-bindgen-test-runner
export CHROMEDRIVER="${CHROMEDRIVER:-chromedriver}"
export WASM_BINDGEN_TEST_TIMEOUT="${WASM_BINDGEN_TEST_TIMEOUT:-120}"
cargo test --locked -p simile-wasm --target wasm32-unknown-unknown --test browser

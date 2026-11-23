#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

# Inspect the target's resolved normal dependencies, not all workspace features.
graph=$(cargo tree --locked -p simile-wasm --target wasm32-unknown-unknown -e normal,build --prefix none)
if printf '%s\n' "$graph" | grep -E '^(axum|tokio|redb|fs2|ureq|tree-sitter)( |-)'; then
    echo "Native dependency reached the browser host" >&2
    exit 1
fi
cargo check --locked -p simile-wasm --target wasm32-unknown-unknown

# Simile browser host

This crate builds with Rust 1.85 for `wasm32-unknown-unknown`. It enables browser
entropy at the host boundary; native crates keep their default behavior. Smithy
source compilation is native-only, but compatible compiled artifacts are portable.

Install the target, the pinned test runner, and Chrome with a matching Chromedriver:

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.100 --locked
bash scripts/check-portable.sh
bash scripts/test-browser.sh
```

The smoke tests run inside a real headless browser. They compile OpenAPI with HCL
and multiple TypeScript declarations, decode artifact bytes, execute a seeded
operation, and exercise manual and realtime clocks. The dependency check rejects
native listeners, storage, networking, and Tree-sitter in the browser graph.

Shared application entry points are in `simile-core::workspace`, `ops`, and
`session`. Virtual projects use the same source discovery and Simfile composition
as native adapters. Baseline execution uses a fresh runtime; stateful sessions use
the shared HTTP adapter and default to manual time at the Unix epoch.

The parity fixtures run unchanged in native Rust and Chromium. Native integration
tests also compare artifact bytes, reports, setup profiles, and actual listener
responses with their in-memory counterparts.

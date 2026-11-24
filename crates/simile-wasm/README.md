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

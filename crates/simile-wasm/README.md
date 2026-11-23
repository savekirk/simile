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
bash scripts/test-worker.sh
```

The smoke tests run inside a real headless browser. They compile OpenAPI with HCL
and multiple TypeScript declarations, decode artifact bytes, execute a seeded
operation, and exercise manual and realtime clocks. The dependency check rejects
native listeners, storage, networking, and Tree-sitter in the browser graph.

Shared application entry points are in `simile-core::workspace`, `ops`, and
`session`. Virtual projects use the same source discovery and Simfile composition
as native adapters. Baseline execution uses a fresh runtime; stateful sessions use
the shared HTTP adapter and default to manual time at the Unix epoch.
Realtime sessions use the browser wall time unless an initial time is supplied.

The parity fixtures run unchanged in native Rust and Chromium. Native integration
tests also compare artifact bytes, reports, setup profiles, and actual listener
responses with their in-memory counterparts.

The Worker test also needs Node.js and npm. It builds the WASM bindings, type-checks
the TypeScript API, and runs the exported functions through an actual module Worker
in Chromium. Generated `pkg/` and `dist/` directories are ignored.

Use `web/client.ts` with a module Worker built from `web/worker.ts`. The client
serializes calls and tracks the Worker generation. `web/contract.ts` defines the
request and response types; invalid message envelopes are rejected before dispatch.
Routes include parameter names, locations, required flags, and serialization rules.
Request synthesis produces body fixtures; parameter values remain caller inputs.
For example, compile a workspace, then explicitly start its immutable revision:

```ts
const artifact = await client.call("compileWorkspace", {
  files: { "api.yaml": source }, contractEntry: "api.yaml",
});
await client.call("startSession", { revision: artifact.revision });
const response = await client.call("handleRequest", {
  method: "GET", uri: "/orders", responseSeed: "18446744073709551615",
});
```

All 64-bit controls and metadata use range-checked decimal strings. Payload JSON
uses `requestDocJson`, `bodyJson`, `profileJson`, or `reportJson`; keep it as text
when reusing or downloading it. An omitted body differs from JSON text `null`.
HTTP bodies, artifact files, bundle files, and checkpoints are `Uint8Array` values.
Headers are ordered `[name, Uint8Array]` pairs, retaining repeated values and raw
header bytes. The HTTP adapter normalizes names and groups values for each name.
Maps are plain objects; absent optional fields are omitted, while explicit metadata
nulls remain null. No payload passes through JavaScript JSON parsing.

Compilation and loading create candidates. Only `startSession` replaces the active
session. Failed compilation, loading, construction, or checkpoint validation leaves
it usable. Baseline `runOperation` calls use fresh runtimes and never modify it.

Reset creates a fresh runtime and empty Stack with the original clock configuration;
compiled fixtures remain available. Checkpoints contain the complete Stack plus
version, artifact digest/format, service identity, Stack protocol, clock, and captured
revision. Restore requires an exact artifact match and validates before replacement.
It restores resources, profiles, outbox, journal, and clock, but starts new HTTP replay
caches and fault RNG progress. Queue leases keep their visibility deadlines. Reset,
restore, and replacement invalidate work handles; they are never persisted.

`stepWork` leases only causally ready work. Execute async work or prepare webhook
bytes and explicitly acknowledge/retry the simulated delivery. Inspection uses a
snapshot and never leases work. Exporting a checkpoint captures bytes; it does not
claim persistence. There is no UI, IndexedDB integration, or webhook network sender.
The Worker retains candidates until termination and transfers only response-owned
buffers; inputs remain reusable by the caller.

The checked-in Smithy fixture was produced by the native CLI (artifact format 24):

```sh
cargo run -p simile-cli -- compile crates/simile-core/tests/fixtures/orders.smithy \
  --artifact /tmp/orders.smithy.similec
cmp /tmp/orders.smithy.similec crates/simile-wasm/tests/fixtures/orders.smithy.similec
```

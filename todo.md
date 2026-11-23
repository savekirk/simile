# Browser execution foundations

Build and verify the portable Rust engine, shared application workflows, and a typed
Worker API in three dependent pull requests. Browser UI and persistence integration
remain outside this stack.

## Phase 1 — Portable core

- [x] Verify fork destination and stack root. `origin` is `savekirk/simile`;
  `upstream` is fetch-only. The fetched upstream master is an ancestor of
  `origin/wasm` (`c0a73b3`), so no rebase rehearsal is needed.
- [x] Separate native dependencies and preserve native defaults.
- [x] Pin CI and release checkout steps to v7.0.1; regenerate the release workflow
  with the configured `dist` 0.32.0. Keep immutable commit pins.
- [x] Support browser entropy and clocks on Rust 1.85.
- [x] Run OpenAPI/HCL, multi-file TypeScript, artifact decode, and seeded execution
  in a real browser; assert the portable dependency exclusions.
- [x] Pass formatting, Clippy, workspace tests, rustdoc, and cargo-deny.
  Chromium: three smoke tests pass; portable dependency check passes.
- [x] Publish [the portable-core PR](https://github.com/savekirk/simile/pull/1) against `wasm`.
  All CI jobs pass, including Chromium and native Linux, macOS, and Windows.

## Phase 2 — Shared workflows and HTTP

- [x] Share workspace composition, audit, description, baseline, replay,
  conformance, state, and session operations with native adapters.
- [x] Share listener-independent HTTP handling and webhook preparation.
- [x] Reject invalid request heads before collecting native bodies. A stalled-body
  regression reproduces the old delay and passes 14 fixed-length/chunked cases
  after the fix; native/browser fixtures retain body-limit error precedence.
- [x] Verify parity with controlled sources, artifacts, seeds, clocks, and signing
  timestamps, including HTTP error and wire behavior.
- [x] Pass formatting, Clippy, workspace tests, rustdoc, cargo-deny, and the
  portable dependency check. Chromium passes five tests.
- [x] Publish [the shared-workflows PR](https://github.com/savekirk/simile/pull/2)
  against `browser/portable-core`. All CI jobs pass. A macOS job was cancelled
  before receiving a runner; inspection found no failing test, and the rerun passed.

## Phase 3 — WASM and Worker API

- [x] Add thin bindings and typed Worker messages with lossless payload boundaries.
- [x] Implement atomic imports, reset, and compatible checkpoint restoration with
  fresh runtime state and invalidated work handles.
- [x] Verify boundary values, CLI Smithy artifacts, and session lifecycle in a browser.
  Chromium passes five shared Rust tests and 104 Worker assertions; native lifecycle
  tests cover snapshot equality, lease visibility, and invalid imports.
- [x] Pass formatting, Clippy, workspace tests, rustdoc, cargo-deny, and the
  portable dependency check under Rust 1.85. TypeScript compiles in strict mode.
- [x] Publish [the WASM API PR](https://github.com/savekirk/simile/pull/4) against
  `browser/shared-workflows`; verify all three bases and keep the PRs open.
- [ ] Inspect the final fork CI results.

## Decisions and tradeoffs

- Preserve default native crate behavior; portable consumers explicitly disable
  native features. The CLI explicitly enables its native feature set.
- Browser clocks and entropy are gated to `wasm32-unknown-unknown`, excluding WASI.
  The correction was verified in the portable-core branch, then the dependent
  branches were rebased bottom-up without conflicts and their gates rerun.
- Smithy source compilation stays native because its Tree-sitter parser requires
  a separate port. Portable builds retain a clear diagnostic and can load artifacts.
- Use a small WASM test host now and grow it into the public bindings in the third
  PR. This proves browser execution before committing to the API surface.
- The existing filesystem-oriented core operations remain native in the first PR;
  the second PR extracts their shared in-memory entry points.
- Use the portable `http` request/response types to retain validated header bytes
  and repeated fields without carrying Axum into the browser graph.
- HEAD now strips fallback error bodies too; this corrects native wire behavior
  while retaining status, headers, and CLI output schemas.
- Profile timestamps follow the service clock. Native defaults still use realtime;
  seeded setup is available for deterministic fixtures.
- The GitHub CLI initially resolved PR creation to the upstream repository despite
  the explicit push remote; no PR was created there. Setting its local default to
  `savekirk/simile` fixed publication. All branch pushes have used `origin`.
- Keep payload JSON in Rust-produced text and binary values in `Uint8Array`;
  controls and 64-bit metadata use decimal strings. Header pairs preserve raw
  bytes and repeated values; the shared HTTP map normalizes names and groups them.
- Checkpoints restore the complete Stack into a fresh runtime and require an exact
  artifact match. Replay caches and fault RNG progress restart; leases keep their
  visibility deadlines. Reset, restore, and replacement invalidate work handles.
- Keep candidate artifacts until Worker termination for this API foundation;
  eviction and compilation cancellation belong with the later UI lifecycle.
  Inputs are cloned; only response-owned buffers are transferred.
- Keep workspace unsafe-code lints intact: the pinned binding macro needs no
  exception. Pin TypeScript and the binding toolchain for reproducible harnesses.
- No user decisions are currently blocking work.

## What was achieved

- Phase 1: portable feature boundaries, browser entropy and clocks, and a real
  Chromium harness are implemented. Native checks pass under Rust 1.85.
- Phase 2: shared workspace and application operations, HTTP exchanges, webhook
  bytes, seeded setup, and explicit session clocks are implemented. Native CLI
  regression tests pass.
- Phase 3: typed WASM/Worker APIs expose compilation, artifact inspection, baseline
  and conformance runs, profiles, HTTP sessions, work, reset, and checkpoints.
  Browser UI, IndexedDB, outbound delivery, and release tooling remain deferred.

## Exit gate

- Phase 1: passed. Chromium compiled OpenAPI/HCL and multi-file TypeScript, decoded
  `.similec` bytes, ran a seeded operation, and exercised both clock modes. The
  resolved browser graph excludes all six prohibited dependencies.
- Phase 2: passed. Named native/browser fixtures cover compilation, reports,
  replay, conformance, state, repeated fields, HEAD, 405, error bodies, body limits,
  and signed webhook bytes. Native listener bytes match the portable adapter.
- Phase 3: passed locally. The module Worker covers decimal integer boundaries,
  nested payload integers, absent/null bodies, typed bytes/maps, repeated headers,
  native Smithy artifacts, invalid imports, checkpoint compatibility, dynamic
  profiles, stale handles/generations, lease recovery, and fresh replay/fault state.

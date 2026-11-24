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
- [ ] Publish the first PR against `wasm`.

## Phase 2 — Shared workflows and HTTP

- [ ] Share workspace composition, audit, description, baseline, replay,
  conformance, state, and session operations with native adapters.
- [ ] Share listener-independent HTTP handling and webhook preparation.
- [ ] Verify parity with controlled sources, artifacts, seeds, clocks, and signing
  timestamps, including HTTP error and wire behavior.
- [ ] Pass repository checks and publish the dependent PR.

## Phase 3 — WASM and Worker API

- [ ] Add thin bindings and typed Worker messages with lossless payload boundaries.
- [ ] Implement atomic imports, reset, and compatible checkpoint restoration with
  fresh runtime state and invalidated work handles.
- [ ] Verify boundary values, CLI Smithy artifacts, and session lifecycle in a browser.
- [ ] Pass repository checks, inspect CI, and verify all three published PR bases.

## Decisions and tradeoffs

- Preserve default native crate behavior; portable consumers explicitly disable
  native features. The CLI explicitly enables its native feature set.
- Smithy source compilation stays native because its Tree-sitter parser requires
  a separate port. Portable builds retain a clear diagnostic and can load artifacts.
- Use a small WASM test host now and grow it into the public bindings in the third
  PR. This proves browser execution before committing to the API surface.
- The existing filesystem-oriented core operations remain native in the first PR;
  the second PR extracts their shared in-memory entry points.
- No user decisions are currently blocking work.

## What was achieved

- Phase 1: portable feature boundaries, browser entropy and clocks, and a real
  Chromium harness are implemented. Native checks pass under Rust 1.85.
- Phase 2: pending.
- Phase 3: pending.

## Exit gate

- Phase 1: passed. Chromium compiled OpenAPI/HCL and multi-file TypeScript, decoded
  `.similec` bytes, ran a seeded operation, and exercised both clock modes. The
  resolved browser graph excludes all six prohibited dependencies.
- Phase 2: pending shared workflow and HTTP parity checks.
- Phase 3: pending binding and session lifecycle checks.

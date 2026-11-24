# Browser execution foundations

Build and verify the portable Rust engine, shared application workflows, and a typed
Worker API, then the browser playground and explicit browser persistence.
The first three implementation PRs are merged; the playground builds on `origin/wasm`.

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
- [x] Reject expired or replaced work receipts before side effects. The regression
  failed before the fix; native and Worker tests now preserve checkpoint bytes
  after rejected execution, acknowledgement, retry, and webhook preparation.
  Tests cover the 29,999/30,000 ms boundary, re-leasing, duplicate completion,
  retry without duplication, and rollback after a failed save.
- [x] Verify boundary values, CLI Smithy artifacts, and session lifecycle in a browser.
  Chromium passes five shared Rust tests and 146 Worker assertions; native lifecycle
  tests cover snapshot equality, lease visibility, and invalid imports.
- [x] Pass formatting, Clippy, workspace tests, rustdoc, cargo-deny, and the
  portable dependency check under Rust 1.85. TypeScript compiles in strict mode.
- [x] Publish [the WASM API PR](https://github.com/savekirk/simile/pull/4) against
  `browser/shared-workflows`; verify all three bases. All three PRs have since
  been merged into `wasm`.
- [x] Inspect the final fork CI results. All six jobs pass on each implementation
  revision: [portable core](https://github.com/savekirk/simile/actions/runs/45288474136),
  [shared workflows](https://github.com/savekirk/simile/actions/runs/45288472817), and
  [WASM API](https://github.com/savekirk/simile/actions/runs/45288997179).

## Phase 4 — Browser playground

- [x] Start from merged `origin/wasm` (`afe36cb`) and verify the fork destination.
- [x] Rehearse an upstream update in an isolated checkout, repeated against the
  latest fetched upstream `12bf17b`. The first portability
  commit conflicts in `Cargo.toml`, `Cargo.lock`, `crates/simile-service/src/lib.rs`,
  and `crates/simile-stack/Cargo.toml`
  with upstream's newer console/persistence changes. Abort the rehearsal and
  preserve the requested fork base; no upstream changes are published.
- [x] Build the playground with shadcn 4.23.0, React, TypeScript, and CodeMirror.
- [x] Separate compiler and runtime Workers, coalesce recompiles, reject stale
  responses, and require explicit runtime revision replacement.
- [x] Support file/folder/ZIP imports, artifact mode, runnable workspace exports,
  decimal seeds, exact payload text/bytes, profiles, and simulated work outcomes.
- [x] Save projects and explicit checkpoints through completed IndexedDB
  transactions. Reject stale save acknowledgements and preserve state on failure.
- [x] Verify native profile credentials and non-mutating work inspection; check
  retries and dead-letter redelivery through shared service logic.
- [x] Verify the complete Orders proof loop, artifact/bundle imports, revision
  switching, storage failure/retry, and mobile layout in six Chromium tests.
- [x] Verify the actual browser ZIP downloads natively: register, provision,
  authenticated create/read over TCP, and reproduction replay all pass.
- [x] Pass all five repository checks, portable dependency checks, five browser
  Rust tests, 146 Worker assertions, and frontend typecheck/lint/build/unit checks.
- [x] Publish [the playground PR](https://github.com/savekirk/simile/pull/5) on
  origin with base `wasm`; leave the PR open and `origin/wasm` untouched.
- [x] Inspect [fork CI](https://github.com/savekirk/simile/actions/runs/45388521715):
  all six jobs pass, including Chromium and native Linux, macOS, and Windows.
- [x] Keep Workbench tabs inside narrow screens. The browser regression now checks
  all six views and every Workbench panel at 390 px width.

- [x] Keep definition and execution on one screen, with resizable panes, file
  tabs, a collapsible tools panel, and compact Run/HTTP controls. Seven Chromium
  tests pass, including simultaneous editor/result visibility, keyboard resizing,
  theme switching, mobile navigation, and the complete native handoff.
- [x] Compare rendered desktop, mobile, light/dark, and HTTP layouts against the
  supplied playground references. See `playground/design-qa.md`. All five Rust
  checks and frontend typecheck, lint, build, and 13 unit tests pass again.
- [x] Make the editor cursor visible in both themes and strengthen light-theme
  text and field borders. Chromium asserts text contrast of at least 4.5:1 and
  cursor/field-border contrast of at least 3:1 on the reviewed surfaces.
- [x] Open Problems automatically for action, compilation, formatting, and storage
  errors, including repeated errors after collapsing the panel.
- [x] Keep separate operation drafts and retain fixture selection. Switching from
  POST to GET synthesizes a body-free request; a manually supplied invalid body
  still reports the contract error. Existing drafts survive operation switches.
- [x] Add file-type formatting and a Guide with contract examples, actions, and
  verified documentation section links. Browser tests add and run new OpenAPI and
  TypeScript operations. Sixteen unit tests and ten Chromium tests pass, along
  with typecheck, lint, build, all five Rust checks, and native export handoff.
- [x] Prevent loading a body-bearing fixture into a body-free operation. Expose
  the compiled body's presence in route metadata, explain mismatches inline, and
  offer lossless removal of top-level body fields. Both themes reproduce and
  recover from the reported GET /orders mismatch. Eighteen unit tests, ten browser
  tests, 149 Worker assertions, portable dependencies, native handoff, and all five
  repository checks pass.
- [x] Address the compiler recovery review: closed Workers are replaced for the
  next compile or artifact load, while source edits and the active runtime survive.
  Unit tests cover failed imports and queued edits; browser tests inject both a
  Worker crash and a timeout, then verify successful compilation and preserved time.
- [x] Add the bundled Swagger Petstore sample with all 20 operations, a request
  fixture, instructions, original snapshot, conversion provenance, and license.
  Native compilation and browser baseline/HTTP/reload pass without remote API calls.
- [x] Diagnose the cancelled macOS check: GitHub reported hosted arm64 capacity
  exhaustion before any steps ran. Rerunning that job passed; all six jobs on
  `07d1849` are green. Keep the existing runner matrix and ARM coverage.
- [x] Verify 21 frontend unit tests and 13 Chromium tests, typecheck/lint/build,
  native downloaded-workspace handoff, and all five repository checks.
- [x] Document WASM and playground architecture beside their source. The guides
  explain ownership, messages, compilation, sessions, and persistence, with seven
  rendered Mermaid diagrams and an ASCII screen map. Local links and all five
  repository checks pass.
- [x] Deploy the playground to `https://similator.dela.dev` with the `cf` CLI.
  The static-assets Worker owns the custom domain; HTTPS and the WASM MIME type
  are verified. Production uses the existing Rust release profile (8,408,136-byte
  WASM) to fit Cloudflare's 25 MiB limit. Development defaults remain unchanged.
- [x] Verify the deployed release with all 13 Chromium tests, including both-theme
  contrast, Petstore, profiles, checkpoints, and recovery. Its exported workspace
  and bundle pass native handoff. Local release tests, 21 unit tests, typecheck,
  lint, build, and all five repository checks pass. Cloudflare version:
  `3368f291-ba84-4816-9807-06f76353fa6e`.

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
- Validate receipts with a read-only Stack lookup so rejection cannot change the
  journal or counters. Session async execution and acknowledgement share one
  transaction so a failed acknowledgement cannot leave committed effects. Keep
  the separate native execution interface; validate all service lease mutations,
  including defer and dead-letter redelivery, under the Stack lock.
- Keep candidate artifacts until Worker termination for this API foundation;
  eviction and compilation cancellation belong with the later UI lifecycle.
  Inputs are cloned; only response-owned buffers are transferred.
- Keep workspace unsafe-code lints intact: the pinned binding macro needs no
  exception. Pin TypeScript and the binding toolchain for reproducible harnesses.
- Keep compile cancellation isolated from the runtime by using two Workers.
  Runtime restart is explicit and discards unsaved runtime state. This costs a
  second WASM instance; measure memory and optimize in the later tooling work.
- Retain payload JSON as text instead of parsing and re-serializing it in React.
  Structured catalogue and bounded counters are safe UI metadata.
- Keep debug WASM for local development and the default build. Use the existing
  Rust release profile for Cloudflare deployment because debug WASM exceeds the
  static-asset size limit. Further optimization and broader browser CI remain
  separate work. Serve only static assets on Cloudflare; the Simile runtime and
  persistence stay in the browser. Configuration and `pnpm run deploy` make
  deployment repeatable without adding a server or changing native defaults.
- Templates keep principal setup in the Simfile and contract-defined webhooks in
  OpenAPI. Explicit database effects accompany async effects so create/read works.
- Saved source files and notes survive an incompatible cached artifact. Storage
  retries reopen failed database connections; only committed saves show as saved.
- Keep the editor and runner visible while inspection tools open below the
  editor. On narrow screens the panes stack; each scrolls independently. This
  prioritizes editing and execution over displaying every advanced control.
- Keep exact payload text in results; retained baseline results identify an older
  revision after recompilation. Tabs preserve forms, while workspace replacement
  resets the baseline form without replacing the active HTTP service.
- Added the current shadcn Resizable component using `react-resizable-panels`
  rather than custom drag/keyboard handling.
- Lazy-load the existing Prettier parsers for YAML, JSON, TypeScript, and Markdown.
  JSON formatting does not round-trip numbers through JavaScript values. HCL uses
  conservative indentation rather than canonical rewriting, preserving comments,
  inline blocks, and heredoc contents without adding another parser dependency.
- New operations are authored in the contract. The Add operation action opens
  examples and the contract, keeping the source as the single definition instead
  of maintaining a separate form-generated representation.
- Fixture filtering checks whether a body is declared by the compiled contract,
  not the HTTP method or filename. Full schema validation remains in Rust. Body
  removal is explicit and edits syntax ranges so other payload values stay exact.
- Bundle a pinned Swagger 2.0 snapshot converted to OpenAPI 3.0.3 with
  swagger2openapi 7.0.8. This uses the existing importer without adding a runtime
  dependency or broadening ingestion scope. Petstore is contract-only: no HCL
  persistence effects or live upstream requests are added.
- Replace a closed compiler engine without automatically repeating the failed
  request. Users retain its diagnostic, and the next compile/edit/import can
  proceed without risking a crash loop or replacing the active runtime.
- No user decisions are currently blocking work.

## What was achieved

- Phase 1: portable feature boundaries, browser entropy and clocks, and a real
  Chromium harness are implemented. Native checks pass under Rust 1.85.
- Phase 2: shared workspace and application operations, HTTP exchanges, webhook
  bytes, seeded setup, and explicit session clocks are implemented. Native CLI
  regression tests pass.
- Phase 3: typed WASM/Worker APIs expose compilation, artifact inspection, baseline
  and conformance runs, profiles, HTTP sessions, work, reset, and checkpoints.
  Outbound delivery and release tooling remain deferred.

- Phase 4: the playground implements authoring, inspection, audit, baseline runs,
  stateful HTTP, profiles, work, reproduction suites, and explicit persistence.
  Definition and execution now share a resizable workspace inspired by the
  supplied playground screenshots. Per-operation requests, visible diagnostics,
  file-type formatting, contextual help, and both-theme readability are verified.
  Petstore is available as a bundled sample, and compiler failures are recoverable.
  Linked architecture guides explain the WASM host and playground workflows.
  The release playground is hosted on Cloudflare at `similator.dela.dev`.

## Exit gate

- Phase 1: passed. Chromium compiled OpenAPI/HCL and multi-file TypeScript, decoded
  `.similec` bytes, ran a seeded operation, and exercised both clock modes. The
  resolved browser graph excludes all six prohibited dependencies.
- Phase 2: passed. Named native/browser fixtures cover compilation, reports,
  replay, conformance, state, repeated fields, HEAD, 405, error bodies, body limits,
  and signed webhook bytes. Native listener bytes match the portable adapter.
- Phase 3: passed locally and in CI. The module Worker covers decimal integer boundaries,
  nested payload integers, absent/null bodies, typed bytes/maps, repeated headers,
  native Smithy artifacts, invalid imports, checkpoint compatibility, dynamic
  profiles, stale handles/generations, lease recovery, and fresh replay/fault state.
  Request synthesis feeds baseline execution; causal work completion and delayed
  retry run through the exported API. Expired and replaced work handles reject
  without changing state; renewed handles still complete or retry exactly once.

- Phase 4: passed locally. Thirteen production-browser tests verify authoring, audit,
  profile setup, authenticated create/read, time/work, checkpoint reload, baseline
  capture, replay/conformance, imports, revision isolation, storage recovery,
  new operation authoring, per-operation drafts, formatting, visible errors, and
  light/dark contrast, Petstore, and compiler failure recovery. Twenty-one frontend
  unit tests cover persistence races,
  incompatible cached artifacts, paths, lossless values, formatting, and request
  body detection/removal. The updated Worker harness passes 149 assertions.
  Native tests verify credential attachment
  and read-only inspection; exported workspaces and bundles run with the CLI.
  The PR targets `wasm`. The implementation revision `487d33d` passed all six fork
  CI jobs, including compiler recovery and the Petstore sample. Architecture
  documentation has validated diagrams and source links; repository checks pass.
  The Cloudflare deployment passes the complete browser suite over HTTPS and the
  native checks using files exported from the public site.

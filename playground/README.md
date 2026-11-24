# Simile playground

Author a service, drop a `.similec`, and exercise it entirely in a browser. The
Vite/React app uses shadcn 4.23.0 (Base UI), CodeMirror, and the shared Rust Worker
API. The Orders starter follows `docs/define-a-service.md`; its Simfile references
the exact exported paths. A multi-file TypeScript starter and blank OpenAPI are
also available.

## Run locally

Use Node 26.13.1, pnpm 12.23.0, Rust 1.85.0, and the matching binding tool:

```sh
rustup target add --toolchain 1.85.0 wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.100 --locked
pnpm --dir playground install --frozen-lockfile
pnpm --dir playground dev
```

`build:wasm` generates ignored bindings in `src/wasm/` and the standalone binding
harness's `pkg/` directory. `build` produces a static site in `dist/`; serve it over
HTTP with the WASM MIME type. `preview` serves the production build locally.
The current build uses the existing debug WASM profile. Release-size tuning,
performance budgets, and the browser CI matrix belong to the later tooling work.

## Workflow

- **Definition editor:** edit/create/rename/delete files while the runner stays
  alongside the source. Open **Composition** below the editor to select a Simfile
  or quick-mode contract, attach HCL modules, and record project notes. Directory
  uploads and ZIPs preserve relative paths. Choose an entry when imports are
  ambiguous. Renaming does not rewrite imports or Simfile paths.
- **Describe and Audit:** inspect the public catalogue and artifact counters;
  filter fidelity findings and apply the shared strict acceptance rule.
- **Run:** synthesize or load a request, expand **Run settings** for a decimal seed and optional
  outcome/scenario, then capture a native reproduction bundle. Payload JSON remains
  Rust-produced text; binary bodies and files remain bytes.
- **HTTP session:** explicitly start/restart a compiled revision. Provision caller
  profiles, opt in to applying their credentials, send HTTP requests, inspect
  backend state and the journal, advance manual time, and step work. Repeated
  headers remain separate. Absent bodies differ from JSON `null`. Authentication,
  wire options, and session controls expand independently.
- **Reproduction:** save/import/export named cases, replay them, or run a suite.
  ZIPs contain native bundle files; they do not represent arbitrary session state.

Drag the divider, or focus it and use arrow keys, to resize the definition and
execution panes. Narrow screens stack them vertically. Problems, Composition,
Describe, Audit, and Reproduction open in a collapsible panel below the editor.
Switching execution tabs preserves request inputs and results; new workspaces
and successful imports start a fresh baseline form. Results from an earlier
compiled revision are labelled.

Compilation uses a separate Worker, debounces edits, coalesces pending builds,
and discards stale responses. It cannot replace the running revision. Cancel
compilation to restart its Worker without losing the session. Runtime recovery is
explicit and discards unsaved runtime state while retaining source edits.

Artifact imports validate before switching modes. Artifacts do not recover source
files. Failed compilation/import/checkpoint validation keeps the active session.
The engine reports artifact-format compatibility; the UI does not hard-code it.

## Saving and checkpoints

IndexedDB autosaves source files, notes, the last compiled artifact, and saved
reproduction cases independently from runtime state. Only a completed transaction
marks a save successful. A failed save leaves edits and the live session usable.

**Save checkpoint** captures the running artifact, options, snapshot, and revision.
A delayed save cannot mark newer mutations saved. **Restore saved** validates in a
spare Worker before replacing the live session. Reload restores source work;
restoring runtime state requires an explicit action. File imports validate against
the running artifact. Restoring preserves Stack state, profiles, outbox, and lease
visibility deadlines, but creates fresh replay caches and fault progress and
invalidates old work handles. This is not exact runtime continuation.

Profile credentials are attached inside Rust, and issued values stay hidden until
revealed in the Profiles panel. State inspection excludes internal credential
records from its resource picker. Saved artifacts and checkpoints can contain
synthetic credentials: browser storage belongs to this origin, not a secret vault.

The workbench opens no TCP listener. Delays and disconnects display transport
plans; webhook success/failure simulates a recipient without network delivery.
Read-only outbox inspection does not lease work. Step leases causally ready items;
expired handles reject without side effects. Redelivery processes the next
available dead letter through the shared service logic.

## Native handoff

Export a runnable workspace ZIP. Quick mode adds and validates a Simfile first.
The ZIP includes notes, exact artifact bytes, and `HANDOFF.sh` with inspect/audit,
describe, register, required setup, and start commands using the service's name.
Registration compiles the complete configuration, including inline setup. Native
profiles are created separately; browser credentials/checkpoints are not copied
into the native registry. A real native listener create/read flow has been verified
using the exported Orders workspace.

## Verify

```sh
pnpm --dir playground typecheck
pnpm --dir playground lint
pnpm --dir playground test
pnpm --dir playground build
pnpm --dir playground exec playwright install chromium
pnpm --dir playground test:e2e
cargo build -p simile-cli --locked
python3 playground/scripts/check-handoff.py
```

Set `CHROMIUM_PATH` to use an installed Chromium instead. Browser tests exercise
the production bundle, real WASM, module Workers, IndexedDB reload, artifact
imports, explicit revision switching, and the Orders proof loop. Unit tests cover
stale compilations, delayed/failed saves, archive paths and exact values. Native
coverage: `cargo test -p simile-core --test playground --locked`. Keep running all
repository checks from `AGENTS.md` and both existing browser harnesses.
The handoff check consumes the actual browser downloads and needs port 4242 free.
It registers and provisions the exported workspace in a temporary native home,
verifies authenticated create/read over TCP, and replays the exported bundle.

Components are checked-in shadcn source. To add components, use
`pnpm --dir playground dlx shadcn@latest add ...` with the global shadcn skill.

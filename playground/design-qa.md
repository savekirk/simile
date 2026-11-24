# Playground layout review

Final result: passed locally, 2026-11-24.

The supplied DartPad screenshot is the primary layout reference: compact toolbar,
file explorer, large editor, adjacent execution, and a lower tools panel. The
Svelte, Rust, and Kotlin screenshots reinforce keeping editing and results on
one screen. These are inspiration, so Simile retains its own controls, typography,
and light/dark tokens rather than copying another product's content or branding.

## Evidence and comparison

Chromium rendered the running playground at 1440×900 and 1920×1080 in dark mode,
1440×900 in light mode, 1024×768, and 390×844, at device scale 1. Local captures
are in `/tmp/simile-design/`: `desktop-dark.png`, `desktop-light.png`,
`desktop-tools.png`, `wide-dark.png`, `tablet.png`, `mobile.png`, and `http.png`.

`comparison.png` places the supplied 3615×2085 DartPad image, reduced to 1440 px
wide, beside the desktop render for composition review. `detail-comparison.png`
compares native-pixel 1100×330 crops of the toolbar, explorer, tabs, and editor.
The source viewport/density is unknown; full-view scaling is not used to judge
font sizes. The screenshots show different applications and execution states.

The usability regression suite captures the final editor and expanded error panel
at 1440×1000 in both themes, plus the author/HTTP layout at 390×844. Run
`CHROMIUM_PATH=/usr/bin/chromium pnpm --dir playground test:e2e` to recreate
`light-editor.png`, `dark-editor.png`, `light-error.png`, `dark-error.png`, and
`mobile-author-http.png` under `playground/test-results/usability-*`.
These captures were reviewed after the cursor, contrast, and toolbar changes.
The follow-up fixture correction adds `light-fixture-mismatch.png` and
`dark-fixture-mismatch.png`, reviewed with the explanation and recovery action
beside the request. Their text and button contrast are also asserted at 4.5:1.

## Findings resolved

- HTTP Send initially sat below the visible request form. Moved it beside the
  route selector and grouped wire options/authentication into expandable sections.
  The revised `http.png` shows Send alongside the definition without scrolling.
- The mobile toolbar occupied three rows. A shorter New label reduces it to two;
  `mobile.png` shows both panes and the primary run action within the viewport.
- The theme button initially required two clicks from system dark mode. It now
  switches the rendered theme on the first click; browser coverage verifies it.
- The dark editor cursor blended into its background. A drawn 2 px caret now uses
  explicit light/dark colors. Both screenshots show the caret against source text.
- Light secondary text and input borders lacked contrast. Stronger semantic
  colors, 450-weight text, and normal browser font smoothing improve readability.
- Action errors could remain outside the visible workflow. Errors now select and
  expand Problems below the source, including repeated errors after dismissal or
  collapse. The editor and runner remain on screen.
- POST request bodies carried into GET operations, and fixture selection reverted
  to the placeholder. Operation drafts are separate, and fixture selection remains
  visible with a modified indicator after edits.
- A create-order fixture could still be loaded deliberately for GET /orders.
  The picker now disables body-bearing fixtures for body-free contracts, explains
  the declaration beside the request, and blocks a manually supplied body before
  running. Remove request body preserves other fields and restores a runnable GET.
- Authoring and formatting were hard to discover. Add operation opens examples
  and the contract; each supported editor has Format. Guide links to verified
  documentation sections and opens the baseline and HTTP tools.

Post-fix full-view and focused comparisons show no remaining actionable issues.

## Review surfaces

- Typography: existing Geist UI text and 13 px monospace source remain readable;
  compact secondary labels and code line heights suit the editor layout. Browser
  assertions require at least 4.5:1 contrast for visible sampled editor/syntax text,
  secondary labels, tabs, select text, and error text in both themes.
- Layout: aligned toolbar edges, narrow explorer, uninterrupted editor, and
  adjacent request/result surfaces follow the reference hierarchy. Tools remain
  collapsible; small screens use vertical panes and internal scrolling.
- Colors: semantic neutral light/dark surfaces, restrained borders, and syntax
  colors preserve contrast. Caret and select-border assertions require at least
  3:1 contrast. The reference's blue palette is intentionally unused.
- Images/icons: no product imagery is required. Existing Lucide icons remain
  consistent; other playground logos and preview content are intentionally absent.
- Copy: labels describe Simile actions. Baseline and HTTP execution remain distinct;
  explicit restart, exact response text, and previous-revision results stay clear.
- Interaction/accessibility: file tabs, keyboard resizing, theme switching,
  expandable controls, errors, imports, and mobile navigation are exercised.
  Ten Chromium tests pass, including editor/result visibility, retained forms,
  formatting, and new OpenAPI/TypeScript operations that compile and execute.

Coverage is Chromium on this Linux host. These targeted contrast checks and visual
reviews do not claim a full accessibility audit or cross-browser certification.

The bundled Petstore workspace also passes a real-browser compile of all 20 routes,
a fixture baseline, HTTP GET, and reload without any requests to petstore.swagger.io.
`petstore-http.png` under `test-results/recovery-samples-*` records the rendered
editor and HTTP 200 response. Separate browser tests inject a compiler crash and
a timeout, then compile successfully while preserving the live service's clock.

## Verification

- [x] Recheck desktop, mobile, light/dark, and HTTP layouts after visual fixes.
- [x] Preserve authenticated create/read, checkpoints, async work, and reproduction.
- [x] Pass typecheck, lint, build, 21 unit tests, and 13 browser tests.
- [x] Run the downloaded workspace and reproduction bundle with the native CLI.
- [x] Pass all five Rust repository checks again after the usability changes.

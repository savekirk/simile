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

## Findings resolved

- HTTP Send initially sat below the visible request form. Moved it beside the
  route selector and grouped wire options/authentication into expandable sections.
  The revised `http.png` shows Send alongside the definition without scrolling.
- The mobile toolbar occupied three rows. A shorter New label reduces it to two;
  `mobile.png` shows both panes and the primary run action within the viewport.
- The theme button initially required two clicks from system dark mode. It now
  switches the rendered theme on the first click; browser coverage verifies it.

Post-fix full-view and focused comparisons show no remaining actionable issues.

## Review surfaces

- Typography: existing Geist UI text and 13 px monospace source remain readable;
  compact secondary labels and code line heights suit the editor layout.
- Layout: aligned toolbar edges, narrow explorer, uninterrupted editor, and
  adjacent request/result surfaces follow the reference hierarchy. Tools remain
  collapsible; small screens use vertical panes and internal scrolling.
- Colors: semantic neutral light/dark surfaces, restrained borders, and syntax
  colors preserve contrast. The reference's blue palette is intentionally unused.
- Images/icons: no product imagery is required. Existing Lucide icons remain
  consistent; other playground logos and preview content are intentionally absent.
- Copy: labels describe Simile actions. Baseline and HTTP execution remain distinct;
  explicit restart, exact response text, and previous-revision results stay clear.
- Interaction/accessibility: file tabs, keyboard resizing, theme switching,
  expandable controls, errors, imports, and mobile navigation are exercised.
  Seven Chromium tests pass, including editor/result visibility and retained forms.

## Verification

- [x] Recheck desktop, mobile, light/dark, and HTTP layouts after visual fixes.
- [x] Preserve authenticated create/read, checkpoints, async work, and reproduction.
- [x] Pass typecheck, lint, build, 13 unit tests, and seven browser tests.
- [x] Run the downloaded workspace and reproduction bundle with the native CLI.

# Selection reliability fixtures

These files are the fixed inputs for the 2026-07-19 mouse-selection reliability remediation. Do not replace them during an acceptance run: changing a fixture changes the baseline and requires updating the remediation plan through its change-control section.

## Frozen files

| Fixture | Purpose | SHA-256 |
| --- | --- | --- |
| `selection-baseline.txt` | TextEdit/native AX baseline | `c91895a9f01b3637f3e34570124ea3c20ac77dff9852e841a732060425ac6a6d` |
| `selection-baseline.html` | Safari selectable DOM baseline | `9dbc666eee63e4d91556a2584de0afcad4e5e9d5649740bcdd9dccc8a6e4ff7b` |
| `selection-baseline.pdf` | Preview selectable PDF baseline | `32e1a2e85170cd4de04c388a816f9e408317237f039f103747f1e14721c26857` |
| `generate_selection_pdf.py` | Deterministic PDF generator | `2b8a8b904ddd22aece5e5038727083c0207b56ed6b76496c039a102785efefd6` |

The PDF generator uses ReportLab's invariant mode. Two consecutive generations on the baseline machine produced the same PDF hash. `pdftotext -layout` confirmed that both repeated-text locations and the explicit `pro-` / `posed` line boundary remain selectable text rather than an image.

## Fixed baseline environment

- macOS 26.5.1 (build 25F80)
- TextEdit 1.20
- Safari 26.5
- Preview 11.0
- Display discovery exposed only the built-in Apple M4 GPU and no second display. Automated negative-origin/mixed-scale geometry tests are therefore required, while physical multi-display acceptance remains a release gate.

## Required runs

Open `selection-baseline.txt` directly in TextEdit; do not paste ad-hoc text into a new document. Its labelled lines provide the drag, double-click, triple-click, Shift-extension, keyboard, repeated-text and A→B targets. Use the exact fixture strings so the acceptance session can compare them in memory and export only privacy-safe classifications, never the selected content or raw position.

For TextEdit Shift-extension, place the insertion point immediately before `amber`, hold Shift, click immediately after `ember`, and release the mouse while Shift is still held. This must produce the native `mouse_up_shift` path. For the keyboard-only scenario, place the insertion point immediately before `Silent`, then use Shift+Command+Right Arrow to select to the end of that line; this must produce the AX-notification path. Do not swap these two procedures.

For TextEdit repeated-text evidence, use a normal mouse drag. The same-location scenario always selects position A. The different-location scenario alternates position A on odd ordinals and position B on even ordinals; the runtime derives the privacy-safe position class from the real mouse-up context and must reject selecting A for every ordinal.

Safari and Preview each have exactly one dedicated `full_baseline` sample. In each application, drag-select the exact sentence `The same selection appears here.` once from the corresponding fixed HTML/PDF fixture and record its source application, version, fixture digest and latency. The TextEdit 30-run same-location/different-location matrix does not apply to Safari or Preview, and the dedicated Safari/Preview scenarios must not be replaced with the TextEdit scenarios.

The quantitative repetitions and latency limits are defined in `docs/SELECTION_RELIABILITY_REMEDIATION_PLAN_2026-07-19.md`; this README does not lower or replace them.

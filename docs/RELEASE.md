# macOS DMG Release

This project ships macOS beta builds as direct-download DMG artifacts.

## Prerequisites

- Apple Developer Program membership.
- Xcode Command Line Tools on the release machine.
- A `Developer ID Application` signing certificate available in the login keychain, or imported by CI.
- Notarization credentials provided through environment variables or CI secrets.

Do not commit Apple certificates, API keys, notary profiles, or keychain exports.

## Build

Run the normal checks first:

```sh
npm run quality
bash apps/desktop/scripts/test-native-watcher-context.sh
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --locked --all-targets --features acceptance-testing
cargo clippy --manifest-path apps/desktop/src-tauri/Cargo.toml --locked --all-targets --features acceptance-testing -- -D warnings
```

`npm run quality` includes TypeScript, Vitest, renderer production build,
Playwright browser UI, Cargo check/test/format, and strict clippy. The native
watcher command additionally compiles the Objective-C bridge for arm64 and
x86_64 with warnings as errors and runs the 50-restart context ownership smoke
test plus the frozen-window, selection/work-area, multi-click, exit/restart, and
cross-thread main-queue overtaking harnesses. The feature build is a separate
acceptance gate; release verification must prove that its injection command and
native test symbols are absent from the default production artifact.

There are three distinct artifact classes:

- Default production `.app`: built without `acceptance-testing`; it must not
  contain test injection commands or native test symbols.
- Local acceptance `.app`: built with `acceptance-testing` for explicit,
  content-free diagnostics; never distribute it to end users.
- Local API-key-file `.app`: built with `local-api-key-file` into its own
  `src-tauri/target/local-api-key-file` tree. It never opens Keychain and is
  rejected by the release signer/verifier marker gates.
- Developer ID release: built, signed, notarized, and verified with the final
  distribution identity.

Build the first two app-only artifacts with:

```sh
npm run build:mac:app -w @paper-float-translator/desktop
npm run build:mac:app:acceptance -w @paper-float-translator/desktop
```

Build the signed DMG for each architecture:

```sh
npm run build:mac:arm64 -w @paper-float-translator/desktop
npm run build:mac:x64 -w @paper-float-translator/desktop
```

The Tauri build compiles the macOS native bridge into the main app process. End
users do not need Swift, Xcode, or a separately authorized helper executable to
run the app.

For local diagnostics when no Developer ID certificate is available, build only
the `.app` bundle with a complete ad-hoc signature:

```sh
npm run build:mac:app:local -w @paper-float-translator/desktop
```

This path is useful for checking app behavior without the DMG mounting step. It
is not a public release artifact because Gatekeeper, notarization, and stable
TCC identity all require a Developer ID signature.

Do not use an ad-hoc build as the final permission acceptance test. macOS TCC
permissions are tied to the app code identity. A local unsigned/ad-hoc rebuild
can invalidate an earlier Accessibility approval even when the app name has not
changed. If System Settings still shows Paper Float Translator as checked after
an ad-hoc overwrite but the app reports `accessibility_not_trusted`, remove the
old Accessibility entry, add `/Applications/Paper Float Translator.app` again,
fully quit the app, and relaunch it from `/Applications`.

For repeat local TCC permission debugging on the same Mac, prefer a stable local
self-signed codesigning identity:

```sh
npm run build:mac:app:local-signed -w @paper-float-translator/desktop
```

This creates or reuses `Paper Float Translator Local Codesign` in the login
keychain and signs the app with that stable identity. It is still not a public
release artifact, but it avoids the worst ad-hoc CDHash churn during local
Accessibility/Input Monitoring tests. Its bundle is written to
`src-tauri/target/local-api-key-file/release/bundle/macos/` so it cannot
overwrite the production build tree.

## Notarization Inputs

Keep signing and notarization identity outside the repo. For local release
builds, prefer a keychain profile or the environment variables supported by
Tauri/Apple tooling, such as:

```sh
export APPLE_API_KEY="..."
export APPLE_API_ISSUER="..."
export APPLE_API_KEY_PATH="/secure/path/AuthKey_XXXX.p8"
```

If CI imports the certificate, use CI secrets for the certificate material and
password. Tauri can infer the signing identity from the imported Developer ID
certificate; otherwise pass it through the release environment rather than
hard-coding it in `tauri.conf.json`.

For a local beta package, sign the complete app with a stable Developer ID
identity before running manual permission tests. Treat ad-hoc builds as local
behavior diagnostics only, not final Accessibility or Input Monitoring
acceptance builds.

If Tauri produced an app-only bundle and it needs to be signed explicitly, run:

```sh
APPLE_SIGNING_IDENTITY="Developer ID Application: Example, Inc. (TEAMID)" \
  npm run sign:mac:app -w @paper-float-translator/desktop -- \
  "apps/desktop/src-tauri/target/release/bundle/macos/Paper Float Translator.app"
```

Without `APPLE_SIGNING_IDENTITY`, `sign:mac:app` uses an ad-hoc identity for
local diagnostics only.

## Verify

After building, validate the `.app` and `.dmg` artifacts:

```sh
npm run verify:mac -w @paper-float-translator/desktop -- \
  "apps/desktop/src-tauri/target/aarch64-apple-darwin/release/bundle/macos/Paper Float Translator.app" \
  "apps/desktop/src-tauri/target/aarch64-apple-darwin/release/bundle/dmg/Paper Float Translator_0.1.0_aarch64.dmg"
```

Run the same verification for the x64 artifact path.

If `hdiutil` fails with `device not configured` / `设备未配置` inside a
sandboxed automation environment, rerun the DMG build from a normal Terminal or
CI runner with disk image mounting privileges.

For local ad-hoc diagnostics only, set `ALLOW_ADHOC_MAC_VERIFY=1` when running
`verify:mac`. Do not use that override for public release artifacts.

## TCC Permission Diagnostics

If drag-selection does not show the floating popup but `Cmd+C+C` still works,
only the key/pasteboard fallback and translation path have been demonstrated.
It does not prove that automatic selection is enabled, that its watcher
generation reached a terminal ready/degraded state, or that mouse/AX sources
are healthy. Read the separate capability snapshot and selection status before
diagnosing Accessibility or Input Monitoring.

1. Quit Paper Float Translator.
2. In System Settings > Privacy & Security > Accessibility, remove old entries
   for Paper Float Translator. A stale `paper-float-watcher` entry can only be a
   residue of a pre-native-bridge build; the current app has no separate helper.
3. Install or copy the current signed app to `/Applications`.
4. Launch the app from Finder and grant Accessibility again.
5. If the settings page says mouse or key monitoring failed, grant Input
   Monitoring to Paper Float Translator, then restart the app.

## Manual Release Smoke Test

1. Install from the DMG into `/Applications`.
2. Launch the app from Finder and confirm Gatekeeper does not block it.
3. Grant Accessibility/Input Monitoring when prompted by macOS.
4. Verify selection popup, `Cmd+C+C`, translation, Keychain API key save, popup close, and popup drag.
5. Disable automatic selection and verify the runtime reports a confirmed
   `selection_disabled_by_setting` terminal state, mouse tap/AX observer/direct
   read and pending retry counts are zero, and `Cmd+C+C` still works. Re-enable
   it and verify a new watcher generation reaches ready or an explicit degraded
   terminal state within two seconds, with no callback from the old generation.
6. While a settings save or watcher refresh is in progress, quit the app and
   confirm no watcher source is started after the final stop. The RC-46
   deterministic gates cover both restart-first/exit-second and
   exit-first/restart-second, but a release smoke test must still confirm the
   installed GUI exits without a residual process.

For keyboard accessibility, record all three native capability paths rather than
testing only the browser preview:

- With the key event tap ready, the selection popup must preserve source-app
  focus; global Tab and Shift+Tab must enter the first and last enabled popup
  controls, and global Escape must close without stealing focus.
- With Accessibility/AX selection ready but the key event tap unavailable, the
  popup must focus itself, support normal Tab/Shift+Tab/Escape, and return focus
  to the frozen source application after an explicit close.
- After a key tap disabled event and recovery, the visible popup must never lose
  both keyboard paths. Close it, then make a second AX-only selection to confirm
  the observer and source focus recovered.

Use the frozen baselines in `apps/desktop/tests/fixtures/README.md`: Safari must
open `selection-baseline.html`, and Preview must open
`selection-baseline.pdf`. Record the macOS/app versions, fixture hashes,
individual failures, and latency samples. The release gate requires 30/30 for
each TextEdit selection mode (drag, double-click, triple-click, Shift extend,
keyboard), 30 repeated same-text selections, 20 rapid A→B selections with no A
overwrite, five minutes of self-window interaction with zero selection triggers,
tap-disabled recovery or explicit degradation within two seconds, and stable
observer/tap/context counts through 50 restarts.

Latency uses the RC-44 contract from
`SELECTION_RELIABILITY_REMEDIATION_PLAN_2026-07-19.md`:

- TextEdit drag, final triple-click mouse-up, Shift extend, and keyboard
  selection: P95 <= 350 ms and maximum <= 800 ms.
- Independent double-click: continue measuring end-to-end from the second
  mouse-up. The content-free report must record the production watcher value
  `Q = multiClickQuietWindowMs`; P95 <= Q + 350 ms and maximum <= Q + 800 ms.
- Safari/Preview paths that need no multi-click coalescing must remain within
  1,200 ms; an independent double-click path must remain within Q + 1,200 ms.
- Missing, zero, out-of-range, overflowed, or production/report-mismatched Q is
  an acceptance failure. Never shorten Q to make latency appear green.

The exported JSON evidence must contain counters, revisions, classifications,
latencies, Q, recovery timing, and resource counts only. It must not contain
selected/cleaned/translated text, clipboard content, window titles, text-derived
hashes or lengths, or credentials. A single successful demonstration does not
satisfy this gate. Browser Playwright and no-TCC native harnesses do not replace
the real Tauri/TextEdit/Safari/Preview run.

If no second display is attached, the automated negative-coordinate and mixed
scale geometry tests remain required, but the real left/upper/lower/mixed-scale
display matrix must stay open as a pre-release manual gate.

## Current Release Blockers

The current 2026-07-19 deterministic remediation snapshot passed:

- root quality: Vitest/contracts 78/78, browser UI 47/47, renderer build,
  default Rust 201/201, format/check, and strict default clippy;
- acceptance feature: Rust 216/216 plus feature check and strict
  acceptance-only clippy; the mutually exclusive local API-key-file feature
  is checked and linted separately by `npm run quality`;
- native strict arm64/x86_64: restart 50, frozen-window 37, work-area 65,
  multi-click 95, `lifecycleRaces=true`, and `raceFinalSources=0`;
- isolated default and acceptance `.app` builds with valid ad-hoc code-signing
  structure and distinct identities. The default executable SHA-256 is
  `0170593afc6d16017b3dfa555cca7bd0a415e6cc64796df601b71474278b466e`,
  uses `com.paperfloat.translator`, and contains neither the acceptance
  namespace nor injection/test symbols. The acceptance executable SHA-256 is
  `35cb69330699df810a4a3cdbd6604922c2522b3299f7ccb4ee5d2e9d699746df`,
  uses `com.paperfloat.translator.acceptance`, and intentionally contains the
  acceptance injection entries. Both artifacts passed the isolation verifier;
  using the same path for both sides is rejected.

The independent read-only reviewer approved the RC-47/REV-36 deterministic
source, automation, and unlaunched-artifact scope with P0/P1/P2 all zero. This
approval only permits requesting current user authorization for the dedicated
acceptance identity. It is not evidence that GUI/TCC acceptance has run and is
not approval of the public release.

These results are deterministic code, browser, host-native, and artifact-class
evidence only. The dedicated acceptance app-data/report/Keychain/TCC namespace
must remain separate from the default app, and formal settings/cache/glossary/
report sentinels must remain unchanged before and after a real acceptance run.
At the last recorded release-identity preflight,
`security find-identity -v -p codesigning` reports `0 valid identities found`.
The strict release verifier rejects the default app because its identity is
ad-hoc, rejects the acceptance app because it contains test symbols, and
Gatekeeper rejects the default local app. Therefore local ad-hoc behavior checks
may continue, but the project must not be described as publicly releasable:
Developer ID signing, notarization, Gatekeeper, stable TCC identity, and
overwrite-permission persistence remain hard release gates. Real GUI/TCC
testing also requires the user's explicit confirmation before launching the
local unpublished app or changing system permissions.

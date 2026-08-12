# Paper Float Translator Windows Port Plan

Date: 2026-08-11
Repository state reviewed: `codex/tauri` at `be1730f`

Revision: 2.0, incorporating the independent Windows/Tauri audit. The implementation must close every audit-blocking item before the corresponding phase exit gate.

## 1. Recommendation

Keep the current macOS implementation intact, extract a small platform boundary around it, and add a Windows adapter. Do not rewrite the translator, renderer, request pipeline, popup state machine, geometry logic, settings persistence, or tests that are already platform-neutral.

Deliver Windows in two product increments:

1. **Reliable Windows MVP**: selected text is read on demand through Windows UI Automation after a configurable global shortcut; Ctrl+C+C remains a clipboard-confirmed fallback. This path is predictable and suitable for an early signed beta.
2. **Automatic-selection parity**: UI Automation selection-change events are the primary trigger. A Raw Input spike determines whether background mouse/key observation is sufficient; a non-consuming low-level hook is allowed only as a documented fallback. The guaranteed shortcut and Ctrl+C+C paths remain available.

This sequencing does not lower the final feature target. It isolates the highest-risk part—cross-application automatic selection—so the Windows build, secure API-key storage, translation, popup, installer, and updater can be validated first.

Expected effort for one experienced Rust/Tauri engineer:

- Shortcut-driven Windows beta: re-baseline after the Phase 3 UIA/Raw Input spike.
- Reliable automatic-selection beta with packaging and compatibility testing: initial planning range **25–45 engineering days**, normally **6–10 calendar weeks** including beta observation.
- Certificate procurement and organizational signing approval are external lead-time items and are not included in engineering days.

## 2. What the current repository already gives us

### Reuse without redesign

- Vite/React/TypeScript renderer.
- Translation request and streaming behavior.
- DeepSeek provider integration.
- Shared settings and popup state machines.
- Cancellation/revision behavior that prevents stale results from replacing newer selections.
- Popup geometry math and Tauri's non-macOS monitor work-area path.
- Clipboard content access through `arboard`.
- Existing unit and acceptance concepts.

### macOS-specific areas that block a real Windows product

1. `build.rs` correctly avoids compiling the Objective-C bridge outside Apple targets, so the build boundary already has a useful starting point.
2. `PaperFloatNativeBridge.m` implements macOS Accessibility selection reads, event taps, pasteboard monitoring, focus coordination, work-area lookup, and Keychain access.
3. Non-macOS watcher states currently report `unsupported_os`; Windows has no capture implementation yet.
4. Production API-key writes return an error outside macOS, while the local-file feature is designed around Unix file permissions. Windows therefore needs a real secure-store backend.
5. `tauri.conf.json` only bundles `app` and `dmg`, contains macOS descriptions, and has no `.ico` or Windows installer configuration.
6. UI and shared capability types contain hard-coded `macOS`, `Cmd+C+C`, Keychain, Accessibility, Input Monitoring, Dock, and `Cmd+Q` language.
7. Package scripts and release documentation are Mac-focused; there is no GitHub Actions workflow.
8. The current Windows host has no Rust/Cargo toolchain on `PATH`, so a live Windows Cargo build has not yet been verified. This must be fixed in Phase 0; static inspection is not a substitute for compilation.

## 3. Reuse policy

The rule for this port is: adopt an official or maintained public implementation when it matches the required contract; adapt a permissively licensed implementation when it is close; write only the product-specific glue that remains.

| Capability | Adopt | Custom code left | Decision |
|---|---|---|---|
| Windows API bindings | Microsoft [`windows-rs`](https://github.com/microsoft/windows-rs) | Feature selection and RAII wrappers | Use; no handwritten Win32/COM FFI |
| UI Automation | [`uiautomation-rs`](https://github.com/leexgone/uiautomation-rs) | Dedicated worker, selected-range policy, event routing | Use after a current-version spike |
| Global shortcut | [Tauri global-shortcut plugin](https://v2.tauri.app/plugin/global-shortcut/) | Bind shortcut to existing controller command | Use; no custom shortcut registry |
| Clipboard content | Existing `arboard` | None for text read/write | Keep; do not add a duplicate clipboard crate |
| Clipboard notification | `AddClipboardFormatListener` through `windows-rs` | Hidden message window and event forwarding | Thin adapter; no polling |
| Background input trigger | Raw Input through `windows-rs`; adapt lifecycle patterns from [MelliLex's MIT hook](https://github.com/trrahul/MelliLex/blob/main/src-tauri/src/services/mouse_hook.rs) only if a low-level hook is proven necessary | Non-consuming left-button-up and double-copy state machine | Spike Raw Input first; never choose a hook merely because the reference app uses one |
| Secret storage | Existing Objective-C Keychain bridge on macOS; `keyring-core` plus a pinned Windows-native store or an explicitly configured Windows-only `keyring` backend | Stable service/account names and detailed error mapping | Do not change Mac Keychain behavior in the Windows port |
| Monitor/work area | Existing Tauri monitor code | Selection-rectangle/cursor anchor selection | Keep existing path first |
| Popup window style | Tauri window APIs, then `windows-rs` only for missing no-activate flags | Small Windows window helper | Thin adapter |
| Installer | Tauri NSIS bundler | Product metadata and languages | Use official bundler |
| Signing | Tauri Windows signing support | CI secret/provider configuration | Use official support |
| Updater | Tauri updater and process plugins | Release endpoint and UX | Use official plugins |
| Release CI | [`tauri-action`](https://github.com/tauri-apps/tauri-action) | Project workflow and gates | Use official action |

### Public repositories that can inform the work

- [MelliLex](https://github.com/trrahul/MelliLex) is the closest reusable Windows/Tauri example and is MIT licensed. Its hook thread, message pump, shutdown behavior, UIA traversal ideas, and application diagnostics are useful. Its product behavior is different: it looks up a word under the pointer with Ctrl+right-click and consumes the right-click. Paper Float must read the full current selection and must not intercept user input. Any adapted code must retain the MIT notice in `THIRD_PARTY_NOTICES.md`.
- [Pot](https://github.com/pot-app/pot-desktop), [CopyTranslator](https://github.com/CopyTranslator/CopyTranslator), [Moon Translator](https://github.com/MoonMonet/Translator), and [Trayslate](https://github.com/plaintool/trayslate) are valuable UX, fallback, OCR, packaging, and compatibility references. Their repository-level GPL licenses make their source unsuitable for direct inclusion in this currently proprietary application.

### Things we should explicitly not build

- A separate Windows helper executable, unless testing proves an in-process adapter cannot work.
- Handwritten UIA/COM bindings.
- Clipboard polling.
- A custom global-shortcut framework.
- A custom credential-encryption or API-key-file format.
- A custom installer or updater.
- A new popup geometry engine.
- An OCR subsystem in the first release.
- Source copied from GPL projects.

## 4. Target Windows behavior

### Supported operating systems

- Primary target: Windows 11, x64. Windows 10 22H2 is a legacy/ESU compatibility target because its normal support lifecycle has ended.
- ARM64 follows only after the x64 compatibility matrix passes.
- Windows 7/8 and 32-bit Windows are out of scope for the first release.
- Run as a standard current-user application; do not request administrator privileges or UIAccess.

### Capture paths

#### A. Guaranteed shortcut path

1. User selects text.
2. User presses a configurable shortcut, initially something unlikely to conflict such as `Ctrl+Alt+T`.
3. The UIA worker reads the focused element's Text pattern and calls `GetSelection`.
4. Empty or unsupported selection produces a short non-blocking explanation and offers the clipboard fallback.
5. A valid selection enters the existing revision/cancellation/translation pipeline.

#### B. Automatic-selection path

1. Subscribe to `UIA_Text_TextSelectionChangedEventId` on a dedicated COM MTA worker.
2. On an event, debounce briefly, confirm the event belongs to the foreground target and not Paper Float, then read `GetSelection`.
3. If a provider fails to publish selection events, a passive left-button-up hook asks the UIA worker to perform the same read.
4. Never perform UIA calls inside the hook callback. The callback only posts a small trigger message and immediately calls `CallNextHookEx`.
5. Normalize, length-limit, deduplicate by gesture/source identity rather than text alone, assign a new revision, and use the existing popup controller.

#### C. Ctrl+C+C fallback

1. A low-level keyboard observer records copy gestures without suppressing them.
2. `WM_CLIPBOARDUPDATE` confirms the clipboard actually changed; `arboard` reads text.
3. Two qualifying copies within the existing gesture window trigger translation.
4. Ordinary clipboard changes do not trigger translation unless the user explicitly enables clipboard-listening mode.

The UIA path must never synthesize Ctrl+C or mutate the clipboard. If a future compatibility mode does so, it must be opt-in, preserve/restore the prior clipboard, and have dedicated tests.

### Known Windows boundary

A normal medium-integrity process cannot inspect the UI of elevated applications. The product should say “无法读取以管理员身份运行的应用；请让两个应用以相同权限运行” rather than requesting elevation. Microsoft states that UIAccess is for qualifying assistive technologies, requires signing and a secure install location, and should not be used merely to place general application UI above other apps.

## 5. Proposed code architecture

Do not move the entire 350 KB Rust `lib.rs` in one change. Introduce seams first, keep the Mac adapter delegating to the existing bridge, then extract incrementally.

```text
apps/desktop/src-tauri/src/
  platform/
    mod.rs
    events.rs
    capabilities.rs
    macos.rs                 # delegates to the existing Objective-C bridge
    windows/
      mod.rs
      uia_worker.rs          # COM MTA; no owned windows
      input_worker.rs        # Win32 message pump, hooks, clipboard listener
      secret_store.rs        # keyring-rs
      window.rs              # no-activate/focus metadata only when needed
  controller/                # existing shared selection/translation controller
  popup_geometry.rs          # existing shared geometry
```

Use narrow interfaces instead of a single all-purpose platform trait:

```rust
trait SelectionMonitor {
    fn start(&self, sink: PlatformEventSink) -> Result<MonitorGeneration, PlatformError>;
    fn stop(&self) -> Result<(), PlatformError>;
    fn read_current_selection(&self) -> Result<Option<SelectionPayload>, PlatformError>;
    fn snapshot(&self) -> SelectionCapability;
}

trait InputTriggerMonitor { /* start, stop, snapshot */ }
trait SecretStore { /* get, set, delete, presence */ }
trait WindowContext { /* foreground identity, anchor, no-activate show */ }
```

Use one platform-neutral event envelope:

```text
PlatformEvent
  source: uia_event | mouse_release | global_shortcut | double_copy
  generation: watcher lifecycle generation
  gesture_id: unique input gesture identity
  foreground_process_id
  text (only after a successful read)
  anchor rectangle or cursor fallback
  timestamp
```

### Required Windows thread split

- **UIA worker**: dedicated COM MTA thread, owns UI Automation objects, adds/removes UIA handlers on the same thread, and owns no window.
- **Input worker**: owns the hidden message window, clipboard listener, low-level hook handles, and Win32 message pump.
- **Tauri/controller thread**: receives small events over a channel and invokes existing application logic.

This follows Microsoft's UIA threading guidance and prevents accessibility calls or hook callbacks from blocking the renderer.

### Capability model cleanup

Replace UI-facing assumptions such as `accessibility`, `inputMonitoring`, and `open_accessibility_settings` with platform-neutral sources and platform-specific help actions:

```text
selected_text_source: mac_ax | windows_uia
automatic_selection: available | degraded | unavailable
shortcut_capture: available | conflict | unavailable
double_copy: available | degraded | unavailable
privilege_boundary: normal | elevated_target_unreadable
actions: open_macos_accessibility | edit_windows_shortcut | show_windows_help
```

Mac-specific labels remain in the Mac presentation mapping; Windows shows Ctrl, tray, UI Automation status, and elevated-process limitations. This avoids forcing Windows into a fake “Accessibility permission” workflow that does not exist.

## 6. Phased implementation plan

### Phase 0 — Reproducible Windows baseline (1–2 days)

Tasks:

1. Perform a read-only inventory of Rust, Node/npm, MSVC, Windows SDK, WebView2, free disk, and existing installation locations. Do not reinstall a suitable component.
2. Keep project-managed tools and caches on the E drive: `.tooling/rustup`, `.tooling/cargo`, `.tooling/node`, `.cache/npm`, and `target/cargo-windows`. Ignore all of them in Git.
3. Install only the missing official Tauri Windows prerequisites. Configure Visual Studio Build Tools for the smallest C++ workload and an E-drive install/cache where supported; record unavoidable C-drive system components before installation.
4. Add `rust-toolchain.toml` and Node engine/version declarations so local and CI environments agree.
5. Run `npm ci`, TypeScript checks, frontend tests, `cargo check --locked --all-targets`, Rust tests, and a debug Tauri launch.
6. Record every actual Windows compile/runtime error before changing architecture.
7. Add `windows-latest` CI for checks only; retain a macOS job so Windows changes cannot silently break the original product.

Exit criteria:

- The repository builds to an executable on Windows, even if capture is still reported unsupported.
- CI reproduces the same build from a clean checkout.
- Baseline failures are recorded as issues/tasks, not hidden by conditional compilation.

### Phase 1 — Platform seam and cross-platform product language (2–4 days)

Tasks:

1. Add the narrow platform interfaces and event envelope.
2. Wrap existing macOS bridge calls behind the Mac adapter without behavioral change.
3. Rename internal lifecycle functions such as `restart_macos_watchers` to platform-neutral names.
4. Generalize capability types and recovery actions while keeping Mac tests passing.
5. Map UI labels by platform: Cmd/Ctrl, Dock/tray, Keychain/Credential Manager, permission/help text, and exit shortcut.
6. Add compile-time tests for both platform implementations and serialization contract tests for capabilities.
7. Add the official Tauri system tray and single-instance plugin. Closing hides to tray; the tray can open settings, pause/resume monitoring, and perform an ordered full exit. A second launch must wake the existing instance.
8. Define a fail-closed Windows acceptance identity with a separate product name, Tauri identifier, executable, AppData root, Credential Manager target, install directory, single-instance identity, Cargo target, and updater channel.
9. Record source commit, file, license, and modification provenance as soon as any permissively licensed code is adapted; do not defer this to release time.

Exit criteria:

- Mac behavior and tests are unchanged.
- Windows can start and reports precise per-source capability states rather than one generic unsupported OS state.

### Phase 2 — Secure storage and Windows packaging skeleton (2–3 days)

Tasks:

1. Introduce `SecretStore` without replacing the existing macOS Keychain implementation. Implement Windows Credential Manager through a pinned Windows-only backend and preserve detailed `missing`, `unavailable`, `access_denied`, and `malformed` states.
2. Disable the Unix-oriented local API-key file in production Windows builds.
3. Add tests for missing/present/update/delete/error secret states and verify no secret appears in logs or settings JSON.
4. Split shared and platform-specific Tauri configuration using `tauri.macos.conf.json` and `tauri.windows.conf.json`.
5. Generate `.ico` from the existing source icon with the Tauri icon tool; do not hand-maintain several icon conversions.
6. Add a basic x64 NSIS `currentUser` installer with the default WebView2 bootstrapper policy.
7. Decide and implement cache disclosure, TTL/size limits, clear controls, uninstall retention, and Credential Manager cleanup behavior before automatic capture reaches beta. Do not design custom cryptography.

Exit criteria:

- A standard Windows user can save, use, update, and delete the DeepSeek key.
- An unsigned internal NSIS installer installs and uninstalls without elevation.
- macOS still builds its app/DMG configuration independently.

### Phase 3 — Shortcut-driven Windows MVP (4–6 days)

Tasks:

1. Add the official Tauri global-shortcut plugin and conflict diagnostics.
2. Implement the dedicated UIA MTA worker with `uiautomation-rs`.
3. Add bounded request queues, per-read controller deadlines, stale-generation rejection, explicit UIA error mapping, and a stuck-worker watchdog. Never use `TerminateThread`; an out-of-process helper remains a go/no-go fallback only if an in-process worker cannot recover in bounded time.
4. Read selected TextPattern ranges, concatenate non-contiguous ranges predictably, reject empty/secure content, and use the selected range's bounding rectangle when available.
5. Define the coordinate contract before positioning: UIA rectangles, cursor anchors, and monitor work areas remain physical pixels until one explicit conversion boundary into Tauri/CSS logical coordinates.
6. Implement the minimum non-activating Windows action popup in this phase so the foreground-focus exit criterion is testable. Explicitly define how intentional user interaction transitions to an activatable window.
7. Fall back to the current cursor anchor and existing Tauri monitor-work-area geometry only through the same physical-coordinate contract.
8. Feed the selection into the existing revision/cancellation/translation path.
9. Implement the input message worker, `WM_CLIPBOARDUPDATE`, `GetClipboardSequenceNumber`, bounded `ClipboardOccupied` retry, and Ctrl+C+C confirmation while continuing to use `arboard` for content. Same-text consecutive copies must remain valid.
10. Add source-specific diagnostics without logging selected text or hashing source text.

Exit criteria:

- Shortcut capture succeeds 30/30 times in every Tier 1 application listed below.
- Ctrl+C+C succeeds 30/30 times and does not trigger on a single copy.
- Selecting rapidly from A to B never allows A's late translation to replace B.
- UIA capture does not alter clipboard contents or foreground focus.

This phase is the first useful Windows beta.

### Phase 4 — Automatic-selection parity (6–10 days)

Tasks:

1. Register `UIA_Text_TextSelectionChangedEventId` on the UIA worker.
2. Implement foreground/self-window/security filters, per-element debounce, lifecycle generation checks, and repeated-same-text gesture handling.
3. Run a Raw Input spike for passive left-button-up and Ctrl+C observation across physical keyboard, touchpad, touch, injected input, and RDP. Do not use `RIDEV_NOLEGACY`.
4. Only if the spike records an unmet requirement, adapt the MIT MelliLex hook lifecycle. Do not reuse its Ctrl+right-click behavior or `LRESULT(1)` input suppression; handle injected-event flags and silent hook loss.
5. Add provider-specific fallback ordering for applications that expose UIA inconsistently.
6. Add start/stop/restart diagnostics and resource counts comparable to the existing Mac acceptance runtime.
7. Handle lock/unlock, suspend/resume, user/session switch, RDP reconnect, display reconnect, and stale events through lifecycle generations.
8. Keep automatic selection disabled by default in early beta; enable it per tester until the compatibility matrix meets the gate.

Exit criteria:

- Automatic selection succeeds 30/30 times in each Tier 1 application for mouse drag, double-click word selection, triple-click/paragraph selection where supported, Shift+arrow, and Ctrl+A.
- Hook callbacks never swallow or noticeably delay input.
- Fifty watcher restart cycles leave exactly one active source set and no extra threads/hooks/listeners.
- Selecting inside Paper Float never recursively triggers translation.

### Phase 5 — Popup, focus, DPI, and UX polish (3–5 days)

Tasks:

1. Refine the Phase 3 non-activating action popup; use `WS_EX_NOACTIVATE`, `WS_EX_TOOLWINDOW`, and `SWP_NOACTIVATE` only through a small `windows-rs` helper if Tauri does not expose the behavior reliably.
2. Validate the explicit activation transition chosen in Phase 3 and preserve source focus for passive display.
3. Test 100%, 125%, 150%, 175%, and 200% scaling; mixed-DPI monitors; negative monitor coordinates; taskbar on every edge; and display reconnects.
4. Add platform-correct tray, close, exit, shortcut, and capability wording.
5. Make failure recovery actionable: shortcut conflict, unsupported provider, elevated target, missing WebView2, credential-store failure, and network failure are distinct states.

Exit criteria:

- Foreground typing continues in the source app while the passive popup appears.
- Popup stays fully inside the correct monitor work area across the DPI matrix.
- Keyboard navigation works after the user intentionally enters the popup.

### Phase 6 — Signed release pipeline and beta gate (2–4 engineering days plus 1–2 calendar weeks of observation)

Tasks:

1. Add `tauri-action` CI for Windows x64 artifacts; build Mac in the same release matrix.
2. Produce NSIS first. Add WiX/MSI later only if managed enterprise deployment requires it.
3. Configure Authenticode SHA-256 signing and timestamping for the executable/installer.
4. Configure Tauri updater signatures separately, publish `latest.json`, and use passive Windows update mode.
5. Finalize `THIRD_PARTY_NOTICES.md`, verify per-file provenance recorded since first reuse, and enforce an automated license allowlist across Cargo, npm, vendored code, fonts, and icons.
6. Run install, upgrade, cancel, rollback, uninstall, and clean-VM smoke tests.
7. Ship to a small signed beta cohort with an opt-in diagnostics export that contains status codes and environment metadata but never selected text or API keys.

Exit criteria:

- Authenticode and Tauri updater signatures both verify.
- Install/update/uninstall works as a standard user on clean Windows 10 and 11 VMs.
- No P0/P1 defects remain, and all Tier 1 rows meet the capture and focus gates.
- Mac release checks still pass.

## 7. Compatibility and test matrix

### Tier 1: supported at first stable release

| Application class | Examples | Required paths |
|---|---|---|
| Native text control | Windows Notepad | shortcut, auto selection, Ctrl+C+C |
| Office | Microsoft Word | shortcut, auto selection, Ctrl+C+C |
| Chromium HTML | Edge and Chrome | shortcut, auto selection, Ctrl+C+C |
| Electron editor | VS Code | shortcut, auto selection, Ctrl+C+C |
| Browser PDF | Edge PDF viewer | shortcut and Ctrl+C+C; auto if provider permits |

### Tier 2: best effort during beta, promoted only after evidence

- Firefox HTML.
- Adobe Acrobat Reader, Foxit, and SumatraPDF.
- Kindle desktop/cloud readers.
- Java, Qt, and custom-drawn document applications.

### Explicit boundaries

- Password/secure fields: never capture.
- Elevated target while Paper Float is not elevated: unsupported with clear help.
- Canvas, image-only PDF, game, remote desktop surface: shortcut/UIA may return no text; use Ctrl+C+C where available. OCR is a later milestone.
- UAC secure desktop and system-integrity UI: unsupported.

### Core acceptance gates

- Local action-popup latency from trigger to visible UI: P95 at or below 250 ms, excluding translation network time.
- Translation first-chunk latency measured separately by provider/network; never hide it inside capture metrics.
- Rapid A→B selection: zero stale A results displayed after B becomes current.
- Repeating the same text in a new gesture: creates a new valid revision.
- No input swallowed; no clipboard mutation on UIA paths; no self-triggering.
- Fifty watcher restarts: no duplicate hooks/listeners/event handlers.
- Four-hour mixed-selection soak: no unbounded thread, handle, or memory growth.
- Logs and diagnostic exports: zero selected text, clipboard content, API keys, or full prompts.

## 8. Security, privacy, and trust decisions

1. Automatic cross-application selection monitoring is privacy-sensitive. Explain it clearly and keep it off by default for the first beta.
2. Monitor only the interactive desktop and immediately discard unsupported/secure/self content.
3. Keep API keys exclusively in the OS credential store; settings contain only presence/status.
4. Do not request administrator privileges or UIAccess.
5. Sign installers and updates, minimize Tauri capabilities, and keep release secrets out of PR workflows.
6. Do not log raw selected text or direct hashes of source text. Use random event IDs, source, status code, timing, and coarse length buckets only when diagnostics require them.
7. State plainly that translation sends the selected text to the configured model provider; capture success is not consent to hidden telemetry.
8. Disclose that the current cache persists source text and translations. Provide TTL/size limits, clear controls, per-app exclusions or pause controls, and an explicit uninstall-retention policy before automatic capture beta.

## 9. Principal risks and mitigations

| Risk | Impact | Mitigation |
|---|---|---|
| UIA provider inconsistency | Automatic selection fails in some apps | Guaranteed shortcut, mouse-up query trigger, Ctrl+C+C, compatibility tiers, later OCR |
| UIA threading/provider failure | Hangs, stale COM objects, leaked handlers, permanently blocked worker | One long-lived MTA worker; bounded queue/deadline; watchdog; same-thread add/remove; no UIA objects crossing threads |
| Background input observer bug | Input lag, interference, or silent loss | Raw Input spike first; if a hook is required use a minimal callback, always chain, detect injected input/silent loss, RAII unhook, lifecycle counters, soak/restart tests |
| Popup steals focus | Breaks reading/typing workflow | No-activate window first; focus restoration only as fallback |
| DPI/coordinate mismatch | Popup appears off-screen | Reuse Tauri monitor work area and shared geometry; explicit physical/logical boundary tests |
| Elevated target | Selection unavailable | Do not elevate globally; explain same-integrity requirement |
| Secret-store regression | API key unavailable or exposed | Preserve the proven Mac bridge; pinned Windows-native backend, detailed status mapping, no file fallback in production, redaction tests |
| Mac regression during extraction | Existing product breaks | Mac adapter first, CI build/test matrix, behavior-contract tests |
| GPL contamination | Proprietary distribution obligations | Reference-only list, file-level attribution, automated license allowlist, code-review checklist |
| Oversized `lib.rs` refactor | Large merge/regression risk | Introduce seams incrementally; do not combine cleanup with behavior changes |

## 10. Recommended first implementation sprint

Work in this order:

1. Make a clean Windows x64 build and capture the exact compiler/runtime baseline.
2. Add the platform interfaces while leaving Mac behavior unchanged.
3. Keep Mac Keychain unchanged; add and prove a Windows-only Credential Manager backend behind `SecretStore`.
4. Split Tauri configuration and produce an internal NSIS installer.
5. Build the shortcut-driven UIA spike against Notepad, Word, Edge, Chrome, and VS Code.
6. Run Raw Input first; only after an evidenced gap, adapt MelliLex's hook lifecycle and implement the low-level fallback.

The first go/no-go checkpoint is the end of Phase 3. At that point there should be a useful Windows beta even if automatic selection still has provider-specific gaps. If the UIA spike fails in an important application, collect its UIA tree and event behavior before adding another library or native helper.

## 11. Research basis

- Microsoft selected-text API: https://learn.microsoft.com/en-us/windows/win32/api/uiautomationclient/nf-uiautomationclient-iuiautomationtextpattern-getselection
- Microsoft selection-change events: https://learn.microsoft.com/en-us/windows/win32/winauto/uiauto-handlingtextrelatedevents
- Microsoft UIA threading: https://learn.microsoft.com/en-us/windows/win32/winauto/uiauto-threading
- Microsoft hooks: https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setwindowshookexw
- Microsoft clipboard listener: https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-addclipboardformatlistener
- Microsoft UIAccess/security boundaries: https://learn.microsoft.com/en-us/windows/win32/winauto/uiauto-securityoverview
- Tauri Windows prerequisites: https://v2.tauri.app/start/prerequisites/
- Tauri platform-specific configuration: https://v2.tauri.app/reference/config/
- Tauri Windows installer: https://v2.tauri.app/distribute/windows-installer/
- Tauri Windows signing: https://v2.tauri.app/distribute/sign/windows/
- Tauri updater: https://v2.tauri.app/plugin/updater/

Detailed research notes are in:

- `research_windows_port/findings_windows_capture.md`
- `research_windows_port/findings_tauri_release.md`
- `research_windows_port/findings_public_repos.md`

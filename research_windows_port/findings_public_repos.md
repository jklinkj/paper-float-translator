# Findings: public repositories and reuse decisions

## Studied Windows reference implementation

### MelliLex

- Repository: https://github.com/trrahul/MelliLex
- License: MIT.
- Relevance: a Windows Tauri AI word/phrase tool with a working low-level mouse hook, UI Automation capture strategies, process/application diagnostics, and fallback handling.
- Candidate source areas:
  - https://github.com/trrahul/MelliLex/blob/main/src-tauri/src/services/mouse_hook.rs
  - https://github.com/trrahul/MelliLex/blob/main/src-tauri/crates/mellilex_capture/src/strategy/uia.rs
- Adoption result: reference only; no MelliLex source was copied. Its architecture confirmed that Tauri, UI Automation, and Windows input diagnostics can coexist, but Paper Float's implementation uses maintained crates plus a smaller non-consuming Raw Input adapter.
- Dependency caution: MelliLex currently demonstrates an older `uiautomation` dependency. Evaluate the current maintained API instead of inheriting its pinned version.

## Reused maintained building blocks

- `windows-rs`: official Microsoft Win32/COM/WinRT bindings; use instead of handwritten FFI.
  - https://github.com/microsoft/windows-rs
- `uiautomation-rs`: focused Windows UI Automation wrapper; use for selection extraction and event handlers.
  - https://github.com/leexgone/uiautomation-rs
- `keyring-rs`: cross-platform secret storage; use instead of writing a Credential Manager wrapper.
  - https://github.com/open-source-cooperative/keyring-rs
- Official Tauri plugins and `tauri-action`: use for shortcut registration, updating, process restart, logging, and release builds.
  - https://github.com/tauri-apps/plugins-workspace
  - https://github.com/tauri-apps/tauri-action

Implemented reuse: `windows-rs`, `uiautomation-rs`, `keyring-core` with `windows-native-keyring-store`, Tauri global-shortcut/single-instance/tray facilities, Tauri NSIS bundling, and the official GitHub Actions pattern. Updater code was not added because a signing key and owned release endpoint do not yet exist.

## Reference only because of copyleft licensing

### Pot

- Repository: https://github.com/pot-app/pot-desktop
- License: GPL-3.0.
- Useful evidence: Tauri can support Windows/macOS/Linux selection translation, clipboard-listening mode, OCR, multiple architectures, and WebView2 troubleshooting.
- Adoption: study UX, compatibility categories, and release matrix only; do not copy code into this proprietary project.

### CopyTranslator

- Repository: https://github.com/CopyTranslator/CopyTranslator
- License: GPL-2.0.
- Useful evidence: clipboard-driven translation, PDF line-break cleanup, global hotkeys, and OpenAI-compatible AI providers.
- Adoption: use as a product/acceptance reference only.

### Moon Translator and Trayslate

- Repositories:
  - https://github.com/MoonMonet/Translator
  - https://github.com/plaintool/trayslate
- License: GPL-3.0 at repository level.
- Useful evidence: double-Ctrl+C popup, tray behavior, focus strategy, automatic update UX, and Windows application compatibility.
- Adoption: reference only. Do not import their source. Prefer official Tauri shortcuts and the maintained Win32/UIA crates over their keyboard-hook dependencies.

## Explicitly rejected reinvention

- No separate native helper executable unless an unavoidable cross-bitness or sandbox problem is demonstrated by tests.
- No handwritten COM/UIA bindings.
- No clipboard polling loop.
- No custom global-shortcut manager.
- No custom Windows credential encryption format.
- No custom installer/updater framework.
- No GPL source copied into the proprietary repository.

## Small custom layer that remains necessary

Public components do not provide the exact behavior contract of Paper Float: automatic full-range selection, revision cancellation, self-window isolation, no-focus-steal popup, and the existing translation state machine. A thin Windows adapter is still needed to connect UIA events, input triggers, and popup/window metadata to the current shared Rust controller. This is integration code, not a replacement for the underlying system libraries.

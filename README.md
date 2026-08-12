# Paper Float Translator

Desktop floating translator for academic reading. Select English text in another app, use a deliberate trigger, and get a DeepSeek-powered translation near the cursor.

[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

Paper Float Translator is fully open source under Apache License 2.0. It has no
project-operated telemetry or translation proxy. Translation requests go directly
to DeepSeek only after the user chooses a translation or terminology action, or
deliberately performs the double-copy translation gesture; see the
[privacy policy](PRIVACY.md) before sending confidential text.

## Windows beta

The Windows x64 port currently provides:

- `Ctrl+Alt+T` selected-text capture through Windows UI Automation.
- `Ctrl+C+C` as an event-driven fallback for applications whose selection is not exposed through UI Automation.
- Experimental mouse-only automatic selection, independently opt-in and **off by default** on Windows.
- A non-focus-stealing popup, system-tray lifecycle, and single-instance activation.
- Windows Credential Manager storage for the API key.
- A per-user NSIS installer configuration in Simplified Chinese and English.

Automatic capture never simulates copy and never rewrites the clipboard. `Ctrl+C+C` responds only after the user performs real copy gestures. See the [implementation status](research_windows_port/IMPLEMENTATION_STATUS.md) and [compatibility matrix](research_windows_port/WINDOWS_COMPATIBILITY_MATRIX.md) before treating the beta as release-ready.

### Windows beta quick start

1. Install and launch Paper Float Translator, then save a DeepSeek API key in Settings. The key is stored in Windows Credential Manager, not in `settings.json`.
2. Leave experimental automatic selection off initially. Select text in another application and press `Ctrl+Alt+T`.
3. If that application's selection is not exposed through Windows UI Automation, copy the same selection twice with `Ctrl+C` within one second (`Ctrl+C+C`).
4. Enable mouse-only automatic selection only if desired; it is an independent opt-in and can be turned off without disabling the two deliberate triggers.
5. Closing Settings keeps the background agent in the system tray. Use the tray menu's Quit action to stop it completely.

## Development

```bash
npm install
npm run dev
```

On Windows, after installing the pinned Node/Rust versions and the official MSVC/Windows SDK prerequisites, use the wrapper that keeps Rust/Cargo/npm temporary and build data under the project directory:

```powershell
npm ci
npm run dev:windows
```

Build the per-user NSIS package with:

```powershell
npm run build:windows
```

The local bundle is written below `target/cargo-windows/release/bundle/nsis`. Public distribution still requires Authenticode signing and clean-VM install/uninstall verification.

The NSIS installer creates its own `uninstall.exe`; no separate uninstaller
download is required. Normal uninstall preserves settings for upgrades. Follow the
[Windows release and complete-removal guide](docs/WINDOWS_RELEASE.md) when replacing
an old beta or removing local data.

## Useful Scripts

```bash
npm run test
npm run typecheck
npm run build
npm run quality
```

See [docs/PROJECT_PLAN.md](docs/PROJECT_PLAN.md) for the shared architecture and [research_windows_port/WINDOWS_PORT_PLAN.md](research_windows_port/WINDOWS_PORT_PLAN.md) for the Windows plan and build-vs-reuse decisions.

## Open-source project

- [Apache License 2.0](LICENSE)
- [Project notice](NOTICE), [third-party overview](THIRD_PARTY_NOTICES.md), and
  generated [npm](THIRD_PARTY_LICENSES_NPM.txt) / [Rust](THIRD_PARTY_LICENSES_RUST.html)
  full-license attributions
- [Privacy policy / 隐私政策](PRIVACY.md)
- [Security policy](SECURITY.md)
- [Contribution guide](CONTRIBUTING.md)

The npm workspaces remain marked `private` only to prevent accidental publication
to npm; that flag does not restrict the Apache-2.0 rights granted by this repository.

## Code signing policy

The current Windows beta is unsigned. The project is preparing a SignPath
Foundation application and will not describe an artifact as signed until the
application and automated trusted-build integration are active. See the full
[Code signing policy](CODE_SIGNING_POLICY.md) and
[SignPath application checklist](docs/SIGNPATH_APPLICATION.md).

After acceptance, the provider disclosure will be: “Free code signing provided by
[SignPath.io](https://signpath.io/), certificate by
[SignPath Foundation](https://signpath.org/).” Each signing request will require a
verifiable automated build and manual approval.

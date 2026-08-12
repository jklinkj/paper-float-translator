# Windows Port Implementation Status

Updated: 2026-08-12

## Outcome

The Windows x64 port is implemented as a beta candidate. The native application builds and runs on the current Windows 11-class host, all three selection triggers were exercised against a production-embedded release binary, and the repository has cross-platform checks for Windows and macOS.

This is not yet a public-release candidate. Authenticode signing, a clean-VM install/uninstall run, a real DeepSeek request, and the wider Word/browser/PDF/VS Code compatibility matrix are still release gates.

## Repository baseline

- Branch: `codex/tauri`
- Starting commit: `be1730f`
- The worktree intentionally contains the Windows implementation and research documents as uncommitted changes. Nothing has been staged, committed, pushed, or published.

## Toolchain and storage

| Component | Current state | Location / decision |
|---|---|---|
| Host | Windows NT `10.0.26200`, x64 | Windows 11-class development host |
| Node.js / npm | `24.14.0` / `11.9.0` | Project-local under `E:\Project\translate\.tooling\node` |
| Rust / Cargo | `1.97.1` / `1.97.1` | Project-local rustup and Cargo homes under `E:\Project\translate\.tooling` |
| MSVC | compiler `19.44.35228`, tools `14.44.35207` | Visual Studio Build Tools under `E:\Project\translate\.tooling\vs-buildtools` |
| Windows SDK | `10.0.26100.0` resource tools | OS-level SDK under `C:\Program Files (x86)\Windows Kits`; this is the unavoidable system component |
| WebView2 | Available | Reused the installed runtime; no duplicate runtime was installed |
| Build output | Active | `E:\Project\translate\target\cargo-windows` |
| Package/build caches | Active | `E:\Project\translate\.cache` and project-local tool folders on E: |

The Windows wrapper in `apps/desktop/scripts/windows-dev-env.ps1` establishes the project-local Rust, Cargo, npm cache, temporary directory, target directory, and MSVC environment before development or packaging.

## Implemented Windows behavior

- Platform-specific lifecycle boundary, close-to-tray behavior, tray menu, and single-instance activation.
- Startup state is managed before window setup, and a Windows user-close `ExitRequested` is prevented until an explicit tray/application quit so the background selection agent cannot become a windowless zombie.
- Windows Credential Manager through the maintained `keyring` backend; API keys are not stored in settings JSON.
- `Ctrl+Alt+T` through Tauri's official global-shortcut plugin.
- Selected-text reads through `uiautomation-rs` on a dedicated worker with a caller timeout and a 32,000-character cap.
- Optional UIA selection-change observation plus non-consuming Raw Input mouse-up fallback. This automatic mode is independent and defaults to off on Windows.
- Event-driven `Ctrl+C+C` fallback using Raw Input plus `WM_CLIPBOARDUPDATE`; there is no clipboard polling and the application does not synthesize `Ctrl+C`.
- Password controls and the application's own process are ignored by automatic selection.
- No-focus-activation popup behavior, per-monitor work-area placement, DPI-aware coordinates, and negative virtual-screen coordinate handling.
- Atomic settings/cache writes, a maximum of 500 cached translations, seven-day cache expiry, and visible privacy/storage disclosure.
- Shared Tauri configuration split into base, macOS, and Windows overlays.
- Per-user bilingual NSIS configuration using project-local packaging tools on E:.
- A read-only GitHub Actions matrix for Windows and macOS. It tests and builds but does not publish releases.
- The Playwright check owns Vite as a direct child process and removes only that process after the run, avoiding the Windows `npm` child-process teardown hang and orphaned preview listeners.

## Verification completed on this host

| Check | Result |
|---|---|
| TypeScript type-check | Passed |
| JavaScript/unit/contract tests | 100 passed |
| Renderer production build | Passed |
| Playwright UI and accessibility tests in system Chrome | 51 passed |
| `cargo check --locked --all-targets` | Passed |
| `cargo test --locked` | 235 passed |
| `cargo test --locked --features local-api-key-file` | 231 passed |
| Default and local-feature `cargo clippy --all-targets -- -D warnings` | Passed |
| Rust formatting | Passed |
| Real Notepad selection via `Ctrl+Alt+T` in a release binary | Passed; exact selected text appeared and the source selection remained active |
| Real external-application mouse-only automatic selection in the final fixed release binary | Passed with the opt-in setting enabled; exact selected text reached the popup without stealing focus |
| Real Notepad `Ctrl+C+C` in a release binary | Passed through popup/preflight; translation correctly stopped at missing-API-key setup |
| Close-to-tray / second-launch lifecycle | Live regression reproduced and fixed; 235-test suite includes a Windows background-agent regression test; repeat the final binary on the clean VM |
| Final NSIS artifact | `3,303,636` bytes; SHA-256 `EF7DECA57747DED4042CC8D8E7914BA6A402BB8DD994DE1311DEE610B42A7BAA`; unsigned |

The automatic-selection setting was returned to its Windows default of **off** after the smoke test.

## Remaining release gates

1. Install, launch, upgrade, and uninstall from a clean standard-user Windows VM.
2. Configure a non-production DeepSeek key and verify one real request, cancellation, retry, cache hit, and error path without recording the secret.
3. Run the app matrix in `WINDOWS_COMPATIBILITY_MATRIX.md`, especially Word, Edge/Chrome, VS Code, and at least two PDF viewers.
4. Obtain an Authenticode certificate, sign and timestamp the executable/installer, and verify the signature on a clean machine.
5. Add updater signing and a release endpoint only after signing keys and release ownership are available.
6. Let the checked-in GitHub Actions workflow run remotely after the user authorizes a push or pull request.

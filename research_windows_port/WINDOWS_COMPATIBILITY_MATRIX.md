# Windows Compatibility Matrix

Updated: 2026-08-12

## Status meanings

- **PASS (live):** exercised against a production-embedded release binary on this Windows host.
- **PASS (automated):** covered by repository tests, but not yet repeated against every external application.
- **EXPECTED UNSUPPORTED:** an intentional product boundary with a documented fallback.
- **PENDING:** no claim is made yet.

## Current host and test boundary

- Windows NT `10.0.26200`, x64.
- Real external-application smoke tests used Windows Notepad; the final fixed automatic-selection path was also exercised against a real selection in a second ordinary application.
- The application had no DeepSeek API key during the smoke run. Selection capture, popup behavior, and missing-key preflight were tested; a live provider response was not.
- Experimental automatic selection was enabled only for its test and then restored to the Windows default of off.

## Trigger and safety behavior

| Scenario | Status | Evidence / next action |
|---|---|---|
| Select in Notepad, press `Ctrl+Alt+T` | PASS (live) | Exact selected text appeared in the popup; Notepad selection remained active |
| Select with the mouse while experimental automatic mode is on | PASS (live) | UIA event/Raw Input path opened the popup with the exact selected source in Notepad and a second external application, without synthetic copy or clipboard mutation |
| Select in Notepad, press `Ctrl+C` twice | PASS (live) | User-initiated clipboard events opened the translation popup; missing-key preflight appeared as expected |
| Automatic selection defaults to off on Windows | PASS (live + automated) | Confirmed in the settings UI and restored after the live test |
| Disable the master Windows selection setting | PASS (automated) | Both automatic and shortcut-driven selection are suppressed by state tests |
| Ignore password controls | PASS (automated) | UIA password property maps to a content-free `secure_field` result; add a live password-box smoke test |
| Ignore selections originating from this application | PASS (automated) | Source PID is filtered before automatic handling |
| Bound selected text to 32,000 characters | PASS (automated) | Unit coverage rejects over-limit reads; add a large-document live test |
| Coalesce automatic events | PASS (automated) | Bounded signal channel plus 120 ms quiet window is unit covered |
| UIA provider stalls | PASS (automated) | Caller stops waiting after 450 ms; add hostile-provider soak testing |
| Popup does not take source focus | PASS (live) | Notepad's selection stayed blue after both shortcut and mouse-only capture |

## External application matrix

| Target | `Ctrl+Alt+T` | Auto selection (opt-in) | `Ctrl+C+C` | Notes |
|---|---|---|---|---|
| Windows Notepad | PASS (live) | PASS (live) | PASS (live) | Current minimum Windows evidence |
| Microsoft Word desktop | PENDING | PENDING | PENDING | Test normal text, tables, comments, and protected view |
| Microsoft Edge HTML | PENDING | PENDING | PENDING | Test ordinary page text and browser PDF viewer separately |
| Google Chrome HTML | PENDING | PENDING | PENDING | Test ordinary page text and complex web editors |
| VS Code editor | PENDING | PENDING | PENDING | Electron accessibility behavior may differ by configuration |
| Adobe Acrobat text PDF | PENDING | PENDING | PENDING | Test single/multi-column papers and line-break cleanup |
| Edge PDF text layer | PENDING | PENDING | PENDING | Treat separately from browser HTML |
| Image-only/scanned PDF | EXPECTED UNSUPPORTED | EXPECTED UNSUPPORTED | EXPECTED UNSUPPORTED | OCR is not implemented in this beta |
| Password/secure input | EXPECTED UNSUPPORTED | EXPECTED UNSUPPORTED | User copy behavior only | Automatic/UIA capture must return no content |
| Elevated administrator application | EXPECTED UNSUPPORTED | EXPECTED UNSUPPORTED | Depends on Windows clipboard | The translator intentionally does not request UIAccess or elevation |

No percentage or broad Windows compatibility claim should be published until the pending rows are exercised repeatedly on a clean VM.

## Lifecycle, display, and resilience

| Scenario | Status | Evidence / next action |
|---|---|---|
| Close settings without quitting background service | PASS (automated) | A live run exposed an `ExitRequested` lifecycle regression; the final code prevents user-close exit until explicit quit and has a dedicated regression test; repeat on the clean VM |
| Launch again while already running | PASS (automated) | Official single-instance plugin path and focus callback are implemented; earlier live activation passed, and the final lifecycle build needs the clean-VM repeat |
| Open settings from tray icon/menu | PENDING | Tray is implemented and unit-covered; perform direct tray interaction on the clean VM |
| Quit from tray | PASS (automated) | Owned menu IDs and shutdown paths are covered; perform live tray quit on the clean VM |
| Normal single-monitor popup placement | PASS (live) | Popup appeared near the current selection/cursor work area |
| Negative virtual-screen coordinates | PASS (automated) | Coordinate math is covered; requires a physical left/above secondary-monitor run |
| 125%, 150%, and mixed-DPI displays | PENDING | Run physical or VM display-scaling checks |
| Sleep/resume and display reconfiguration | PENDING | Soak test shortcut, Raw Input, UIA observer, and tray recovery |
| Explorer restart / tray recreation | PENDING | Verify the icon and menu recover correctly |
| Rapid repeated selections | PASS (automated) | Bounded/debounced scheduling is covered; add a long live soak run |

## Storage and packaging

| Scenario | Status | Evidence / next action |
|---|---|---|
| Atomic settings/cache writes | PASS (automated) | Managed JSON writes use an atomic-write dependency and tests |
| Cache maximum 500 entries and seven-day expiry | PASS (automated) | Limit, expiry, and cleanup tests pass |
| API key excluded from settings JSON | PASS (automated) | Windows path uses Credential Manager backend; perform a live save/read/remove test with a disposable key |
| Per-user NSIS package build | PASS (local build) | Final x64 artifact is 3,303,636 bytes with SHA-256 `EF7DECA57747DED4042CC8D8E7914BA6A402BB8DD994DE1311DEE610B42A7BAA`; Tauri NSIS tooling stayed on E: |
| Standard-user install/uninstall | PENDING | Do not infer this from a successful bundle build; use a clean VM |
| Upgrade over an older version | PENDING | Requires two signed test versions |
| Authenticode signature | PENDING | `Get-AuthenticodeSignature` reports `NotSigned` for both the final installer and application because no certificate was supplied |
| Signed updater and rollback | PENDING | Intentionally excluded until signing keys and endpoint ownership exist |
| Windows + macOS remote CI | PENDING (remote) | Workflow is checked in and locally inspected; it has not run on GitHub without an authorized push |

## Clean-VM release script

For each supported Windows version, record the OS build, architecture, WebView2 state, scale factor, application version, installer SHA-256, and whether the user is standard or elevated. Then:

1. Verify Authenticode before executing the installer.
2. Install as a standard user and confirm no elevation prompt.
3. Launch, save/remove a disposable API key, and confirm settings JSON contains no secret.
4. Execute every application row above at least ten times per trigger, recording exact failures rather than a single aggregate percentage.
5. Test close-to-tray, tray reopen/quit, second launch, sleep/resume, Explorer restart, and mixed-DPI monitors.
6. Exercise provider success, cancellation, retry, invalid key, rate limit, network loss, cache hit, and cache clearing.
7. Upgrade from the previous signed beta, then uninstall and inventory intentionally retained user data/credentials.

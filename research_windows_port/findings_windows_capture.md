# Findings: Windows input and selected-text capture

## Official primitives

- Microsoft UI Automation (UIA) is the primary non-destructive selection source. `IUIAutomationTextPattern::GetSelection` returns the currently selected text ranges for controls that expose the Text pattern.
  - https://learn.microsoft.com/en-us/windows/win32/api/uiautomationclient/nf-uiautomationclient-iuiautomationtextpattern-getselection
- Text providers can raise `UIA_Text_TextSelectionChangedEventId` when text is selected or deselected. This is the best primary trigger for automatic selection mode, but support depends on the target application's accessibility provider.
  - https://learn.microsoft.com/en-us/windows/win32/winauto/uiauto-handlingtextrelatedevents
- A desktop-wide UIA client must make calls and event subscriptions on a dedicated COM MTA thread; event handlers should be added and removed on that same thread.
  - https://learn.microsoft.com/en-us/windows/win32/winauto/uiauto-threading
- Raw Input can deliver keyboard and mouse input to a registered background window through `WM_INPUT`. It provides the non-consuming input signal needed here without installing a global low-level hook.
  - https://learn.microsoft.com/en-us/windows/win32/inputdev/about-raw-input
  - https://learn.microsoft.com/en-us/windows/win32/inputdev/wm-input
- `AddClipboardFormatListener` provides event-driven clipboard change notifications through `WM_CLIPBOARDUPDATE`, so a polling loop is unnecessary.
  - https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-addclipboardformatlistener
- Regular medium-integrity desktop applications cannot inspect elevated application UI. UIAccess is intended for qualifying assistive technologies and requires signing plus a secure install location; it should not be requested by a general translator.
  - https://learn.microsoft.com/en-us/windows/win32/winauto/uiauto-securityoverview

## Recommended capture pipeline

1. Guaranteed/manual path: an official Tauri global shortcut requests the current selection.
2. Primary automatic path: UIA selection-change events trigger `GetSelection` on a dedicated MTA worker.
3. Compatibility trigger: a non-consuming Raw Input left-button-up signal requests a UIA read when a provider does not emit selection events.
4. Clipboard fallback: double-Ctrl+C or an explicit copy-and-translate mode listens for `WM_CLIPBOARDUPDATE` and reads text with the project's existing `arboard` dependency.
5. Later optional path: OCR for canvas, image-only PDF, and applications that expose neither UIA text nor useful clipboard text.

The primary path should never synthesize Ctrl+C or replace clipboard contents. Clipboard mutation is acceptable only in an explicit fallback mode that preserves and restores prior content and is covered by integration tests.

## Rust implementation choice

- Use `uiautomation-rs` for UIA element, TextPattern, text-range, and event wrappers.
  - https://github.com/leexgone/uiautomation-rs
- Use Microsoft's `windows-rs` for the small set of missing Win32 details: Raw Input, hidden message window, clipboard listener, foreground-window metadata, monitor work area, and no-activate window flags.
  - https://github.com/microsoft/windows-rs
- Keep the current `arboard` crate for clipboard contents; add Win32 only for notification.

## Known boundaries

- UIA support varies by application and embedded document renderer. Browser HTML, native text boxes, Electron editors, Office, and different PDF viewers must be tested separately.
- Password and secure fields must always return no content and must never be logged.
- Elevated targets are reported as unsupported unless the translator is deliberately launched at the same elevation; the app itself should not request administrator privileges by default.
- Raw Input handlers are triggers only. The application deliberately omits `RIDEV_NOLEGACY`, performs minimal work in the message handler, and never blocks normal keyboard or mouse delivery.

## Implemented decision (2026-08-12)

The port uses `uiautomation-rs` selection-change events as the primary opt-in automatic signal and Raw Input mouse-up as the compatibility signal. Both are debounced into the same dedicated UIA worker. The guaranteed manual path remains the official Tauri `Ctrl+Alt+T` global shortcut, and the clipboard fallback listens for a real user `Ctrl+C` gesture followed by `WM_CLIPBOARDUPDATE`.

This replaces the earlier low-level-hook candidate. No MelliLex hook code or GPL reference-project code was copied.

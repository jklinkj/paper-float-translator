# Interaction and visual findings

Updated: 2026-08-12

## Primary-source findings

- Microsoft describes flyouts as lightweight, contextual, light-dismiss surfaces. Outside click and Escape are valid dismissal gestures, and repeated flyouts can become distracting. A dismissal therefore has to be treated as user intent rather than as a temporary rendering detail.
  - https://learn.microsoft.com/en-us/windows/apps/design/controls/dialogs-and-flyouts/
  - https://learn.microsoft.com/en-us/windows/apps/develop/ui/controls/dialogs-and-flyouts/flyouts
- Microsoft's notification guidance says interruptions should remain valuable, not noisy, and behavior after an interaction should deliberately respond to user intent.
  - https://learn.microsoft.com/en-us/windows/apps/develop/notifications/app-notifications/app-notifications-ux-guidance
- Windows command-bar guidance distinguishes proactive selection UI, which appears without taking focus and dismisses when ignored, from explicitly requested reactive UI. This supports giving deliberate shortcuts precedence over automatic suppression.
  - https://learn.microsoft.com/en-us/windows/apps/develop/ui/controls/command-bar-flyout
- Current Windows settings guidance recommends a single readable column, related sections, binary toggles, smart defaults, roughly four or five primary settings, progressive disclosure for advanced options, and immediate application of changed settings.
  - https://learn.microsoft.com/en-us/windows/apps/design/app-settings/guidelines-for-app-settings
- Windows typography guidance recommends one UI font, regular body text, semibold titles, a 12/16 caption and 14/20 body scale, sentence case, and left alignment. Spacing guidance emphasizes a consistent 8/12 px rhythm, while elevation guidance uses subtle contour and shadow to distinguish transient surfaces.
  - https://learn.microsoft.com/en-us/windows/apps/design/signature-experiences/typography
  - https://learn.microsoft.com/en-us/windows/apps/design/basics/content-basics
  - https://learn.microsoft.com/en-us/windows/apps/design/signature-experiences/layering
- Windows accessibility guidance requires meaningful accessible names, complete keyboard operation, and visible focus treatment.
  - https://learn.microsoft.com/en-us/windows/apps/design/accessibility/accessibility-checklist
  - https://learn.microsoft.com/en-us/windows/apps/develop/input/keyboard-interactions

## Product inference: dismissal state machine

An automatic selection has an identity formed from its source process and normalized selected text. The popup records whether the visible selection was opened automatically or deliberately.

1. **Automatic selection opens popup:** remember its identity and automatic origin.
2. **User explicitly closes it, presses Escape, or clicks outside:** close immediately, cancel any translation, and suppress that identity.
3. **Duplicate automatic UIA/mouse events for the suppressed identity:** ignore them before mutating popup state or sending a provider request.
4. **Selection becomes empty:** clear suppression because the rejected selection no longer exists.
5. **Selection changes to different text or a different source process:** clear the old suppression and allow the new automatic selection once.
6. **User presses `Ctrl+Alt+T` or performs `Ctrl+C+C`:** this deliberate request overrides automatic suppression for that invocation.
7. **User changes automatic-selection settings or restarts the watcher:** clear stale suppression.
8. **Clicks generated while dismissing the app's own popup:** never become an external automatic-selection trigger; self-process filtering remains mandatory.
9. **Copying the selected source from the popup:** this is also a deliberate completion gesture. It closes the popup and keeps the same automatic identity muted; copying must not create a reopen loop.
10. **The popup's own focus has no readable selection:** that internal empty read must not clear the user's dismissal. Suppression resets only when an external focused process reports no selection, the external selection changes, settings restart the watcher, or a deliberate shortcut overrides it.

The suppression is bounded to the current selection identity, not a timer. A timer alone would eventually reopen the exact selection against the user's intent; permanent text-only suppression would incorrectly block the same phrase selected later in another context.

## Settings information architecture

Primary page:

1. **Status header:** compact health summary and one recovery action only when needed.
2. **Translation:** API key, model, mode, target language.
3. **Triggers:** one master selection switch; three trigger rows with deliberate vs automatic semantics; automatic mode labeled as optional and interruptive.
4. **Popup:** width and explicit dismissal behavior, including the statement that the same automatic selection stays muted until it changes.
5. **Storage and advanced:** PDF cleanup, local cache, cache clearing, credential removal, diagnostics and uninstall disclosure under progressive disclosure.

The footer retains Save/Cancel for now because the backend uses atomic multi-field settings commits; a later migration to immediate save should be a separate transactional change.

## Popup visual specification

- Use a restrained neutral surface with one thin contour and one soft elevation shadow; remove heavy nested card borders.
- Keep the source preview visually secondary and the translation as the main reading surface.
- Provide one primary action per state. Copy, pin and close are compact secondary icon commands with accessible labels and tooltips.
- Make the close affordance safe and predictable. On activation it closes immediately and suppresses the current automatic identity.
- Use the Windows type ramp, an 8/12 px spacing rhythm, 12 px rounded geometry, and minimum 32 px pointer targets.
- Do not steal focus when appearing. Preserve Escape, external Tab entry, visible focus outlines, reduced-motion behavior and RTL layout support.

## Acceptance criteria

- Closing an automatic popup produces zero reopen events for repeated reads of the same source/text identity, regardless of debounce timing.
- No provider request begins for a suppressed automatic identity.
- A different selection opens once; an empty selection resets suppression.
- `Ctrl+Alt+T` and `Ctrl+C+C` work immediately after dismissal.
- Closing pending, ready, translating, success and error states all honor the same user-intent rule.
- UI tests cover 320, 379, 380, 420 and 640 px widths; long Latin, CJK, emoji, multiline and RTL text; keyboard and automated accessibility checks.

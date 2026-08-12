# Paper Float interaction and visual redesign research plan

Updated: 2026-08-12

## Main question

How should Paper Float prioritize an explicit user dismissal over automatic selection events, while keeping deliberate translation triggers available and presenting a calmer, clearer Windows popup and settings experience?

## Subtopics

1. **Dismissal and notification behavior**
   - Find primary platform guidance for transient surfaces, explicit dismissal, repeated notifications, and user-control principles.
   - Derive a precise suppression/resume state machine for automatic selection.

2. **Windows popup and settings hierarchy**
   - Find primary Windows guidance for command priority, progressive disclosure, spacing, typography, and settings organization.
   - Convert the guidance into repo-native React/CSS changes rather than copied visual assets.

3. **Accessible state feedback**
   - Verify keyboard, focus, motion, contrast, and status-announcement expectations for a non-focus-stealing translation surface.

## Synthesis structure

- Evidence-backed principles
- Product interaction state machine
- Settings information architecture
- Popup visual and behavioral specification
- Testable acceptance criteria and remaining limitations

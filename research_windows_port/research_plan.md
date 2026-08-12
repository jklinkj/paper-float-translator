# Paper Float Translator Windows Port Research Plan

## Main question

How should the existing macOS-first Tauri translator be brought to Windows while reusing mature public components and repositories wherever practical?

## Subtopics

1. **Windows input and selected-text capture**
   - Identify official Windows APIs and maintained libraries for global input observation, UI Automation text selection, clipboard fallback, focus restoration, and multi-monitor cursor/work-area handling.
2. **Tauri integration, storage, packaging, and release**
   - Identify Tauri-supported plugins or mature Rust crates for shortcuts, window behavior, credential storage, installer generation, signing, updates, and CI builds.
3. **Reference implementations and adoption decision**
   - Review relevant public repositories, distinguish reusable dependencies from architectural references, and define the minimum custom Windows adapter that remains necessary.

## Final synthesis

The final plan will begin with the recommended Windows product scope, then map each macOS-specific subsystem to a reusable Windows component, define the proposed architecture and phased milestones, and finish with testing, release gates, risks, estimates, and explicit build-vs-reuse decisions.

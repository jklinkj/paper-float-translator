# Contributing

Thank you for improving Paper Float Translator. Small, focused changes with clear
user impact are easiest to review.

## Before starting

Open an issue before a large architecture, privacy, permission, installer, or
release-pipeline change. Never add proprietary code, copied assets without clear
rights, credentials, user data, or opaque precompiled binaries. Prefer maintained
upstream libraries and platform APIs over new custom implementations, and explain
the choice when adding a dependency.

Repository `private: true` fields only prevent accidental npm publication; they do
not change the repository's Apache-2.0 license.

## Development setup

The repository pins Node in `.node-version` and Rust in `rust-toolchain.toml`.
Install the platform prerequisites documented in [README.md](README.md), then run:

```text
npm ci
npm run typecheck
npm test
```

Windows contributors should use `npm run dev:windows` so Cargo, npm cache, build
output, and temporary files stay under the project directory. macOS contributors
can use `npm run dev` after installing Xcode Command Line Tools.

Before requesting review, run the checks relevant to your platform. CI runs the
cross-platform suite and open-source dependency policy. Release or signing changes
also require the checks in [docs/WINDOWS_RELEASE.md](docs/WINDOWS_RELEASE.md).

## User-interaction invariants

Changes to selection behavior must preserve these rules:

- user actions take priority over background detection;
- closing a popup suppresses the same still-active selection instead of reopening
  it repeatedly;
- a popup is triggered by a deliberate user selection/copy gesture, not merely by
  the existence of selected text;
- automatic capture never simulates copy and never rewrites the clipboard; and
- translation is not requested until the user chooses a translation action.

Add contract tests for lifecycle and intent changes, plus renderer/UI tests for
visible behavior. Describe any application-specific compatibility limits.

## Pull requests

1. Create a focused branch and keep unrelated local changes out of the pull
   request.
2. Update tests and user-facing documentation with the code.
3. Call out new network transfers, permissions, persistent data, dependencies,
   installer changes, or release-workflow changes explicitly.
4. Confirm that no secrets, private documents, or proprietary components are
   included.
5. Wait for required review and CI before merge. Contributions from people without
   commit access require maintainer review.

By intentionally submitting a contribution for inclusion in this project, you
agree that it is provided under Apache License 2.0, as described in section 5 of
that license, unless you conspicuously mark it in writing as “Not a Contribution.”
You must have the right to submit every part of the contribution, including
AI-assisted output, fixtures, fonts, icons, and other assets.

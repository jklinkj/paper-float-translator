# Code signing policy

## Current status

Paper Float Translator is preparing an application to SignPath Foundation. Until
the application is accepted and the automated signing configuration is active,
Windows beta artifacts are **unsigned** and release notes must say so. No unsigned
artifact may claim to be an official signed release.

After acceptance, the required provider disclosure is:

> Free code signing provided by [SignPath.io](https://signpath.io/), certificate by [SignPath Foundation](https://signpath.org/).

The certificate publisher will therefore be **SignPath Foundation**, not the
project name or maintainer's personal name.

## Team roles

- Authors/committers: [@jklinkj](https://github.com/jklinkj)
- Reviewers: [@jklinkj](https://github.com/jklinkj); every change proposed by a
  non-committer must receive maintainer review before merge.
- Signing approvers: [@jklinkj](https://github.com/jklinkj); every signing request
  requires a manual approval after release evidence has been reviewed.

All people with repository, CI, SignPath, review, or approval privileges must use
multi-factor authentication. Roles must be updated here before access changes.

## What may be signed

Only Paper Float Translator artifacts built from this public repository may be
submitted under the project's signing policy. The signed scope is limited to the
project-owned Windows executable and installer generated from reviewed source and
build scripts. Upstream binaries must retain their upstream identity and must not
be presented as project-owned signed binaries.

All submitted binaries must consistently identify the product as **Paper Float
Translator** and use the same version as `package.json`, Tauri configuration, and
`Cargo.toml`.

## Verifiable build and approval

1. An unsigned candidate is built from a manually selected, reviewed source
   revision by the repository's Windows GitHub Actions workflow using checked-in
   lockfiles and pinned toolchain versions. After SignPath setup, production
   signing accepts only protected version tags.
2. The workflow runs tests, license/source checks, builds the NSIS installer,
   records the source revision, generates SHA-256 checksums, and produces an SPDX
   SBOM.
3. The signing approver compares the tag, version metadata, workflow run, hashes,
   SBOM, and expected artifact list. Approval is always manual.
4. After SignPath approval and project configuration, only the artifact produced
   from a protected version tag by the trusted workflow is submitted to SignPath.
   Local builds are never substituted into a signing request.
5. The signed executable and installer are verified with Windows Authenticode
   tools and smoke-tested on a clean Windows virtual machine before publication.
6. Release notes link this **Code signing policy**, [PRIVACY.md](PRIVACY.md), the
   source tag, checksums, SBOM, and install/uninstall instructions.

Build and signing workflow changes receive the same review as application source
because they affect the relationship between source and signed binaries.

## Privacy and user control

The app's data flows and user controls are documented in [PRIVACY.md](PRIVACY.md).
The installer shows a privacy notice before copying files. Selection detection is
local; selected text is transferred directly to DeepSeek only after the user
chooses a translation or terminology action or deliberately performs the
configured double-copy translation gesture. Installation and complete-removal
instructions are in [docs/WINDOWS_RELEASE.md](docs/WINDOWS_RELEASE.md).

## Incident response

If a signed artifact, signing workflow, maintainer account, or signing approval is
suspected to be compromised, publication stops immediately. Maintainers preserve
logs, notify SignPath, request suspension or revocation where appropriate, publish
a security advisory, and do not resume signing until the root cause is corrected.

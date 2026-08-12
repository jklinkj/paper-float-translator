# SignPath Foundation application checklist

This file separates repository work from external actions that only the project
owner can complete.

## Repository evidence prepared here

- OSI-approved Apache-2.0 project license: `LICENSE`
- project and third-party attribution: `NOTICE`, `THIRD_PARTY_NOTICES.md`
- privacy, security, contribution, install/uninstall, and code-signing policies
- public, locked dependency manifests with automated license/source checks
- no project-owned opaque precompiled helper; the macOS native bridge builds from
  `apps/desktop/src-tauri/native/PaperFloatNativeBridge.m`
- automated Windows release-candidate build with hashes and SPDX SBOM
- explicit maintain/review/signing roles and manual signing approval policy

## Owner actions before applying

1. Merge and publish these files to the public default branch.
2. Enable two-factor authentication for GitHub and later for SignPath.
3. Enable the GitHub dependency graph, private vulnerability reporting, and branch
   protection/rules for `main`; require CI and dependency review.
4. Publish at least one documented **unsigned beta** release in exactly the NSIS
   form that should later be signed. Link its source tag, checksum, SBOM, privacy
   policy, install/uninstall guide, and **Code signing policy**.
5. Confirm the public repository and release contain no proprietary components,
   secrets, private documents, or unaccounted precompiled project binaries.
6. Apply at <https://signpath.org/apply.html> using the public repository and
   release URLs.

## After acceptance

1. Create the SignPath organization/project, trusted build system, artifact
   configuration, signing policy, and certificate configuration exactly as
   instructed by SignPath.
2. Configure file metadata restrictions for product name and version.
3. Connect the GitHub workflow using the identifiers and credentials issued by
   SignPath; store secrets only in GitHub/SignPath secret stores.
4. Protect the signing environment with a required manual approver.
5. Make the first signed request from a version tag, verify its provenance and
   Authenticode signature, then perform the clean-VM release checks.
6. Update the README and release page from “application pending” to the exact
   provider disclosure in `CODE_SIGNING_POLICY.md` only after signing is active.

SignPath Foundation decides eligibility. Repository preparation does not imply
acceptance or guarantee that SmartScreen reputation warnings disappear immediately.

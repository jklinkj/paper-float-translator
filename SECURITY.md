# Security policy

## Supported versions

Paper Float Translator is currently beta software. Security fixes target the
latest published release and the `main` branch. Older beta builds may not receive
backports; users should upgrade after a fixed release is published.

## Report a vulnerability privately

Use [GitHub private vulnerability reporting](https://github.com/jklinkj/paper-float-translator/security/advisories/new).
If that form is unavailable, open a public issue containing only a request for a
private contact channel. Do not disclose exploit details publicly before a fix is
available.

Never include:

- DeepSeek API keys or other credentials;
- selected or translated confidential text;
- personal data; or
- an unredacted local settings, cache, glossary, or credential export.

Include the affected version, operating system, reproduction steps, impact, and
the smallest redacted proof needed to understand the problem. Reports about an
official release should include its SHA-256 digest and signature status.

## Maintainer process

The maintainer will acknowledge a usable private report, assess its impact,
prepare a fix, and coordinate disclosure on a best-effort basis. Response times
are not guaranteed and the project does not currently operate a bug bounty.

If a signing key, release workflow, or signed artifact may be compromised, stop
publishing, preserve evidence, notify the signing provider, and revoke or suspend
affected signing material before resuming releases.

## Security boundaries

- The app sends selected text to DeepSeek only after an explicit translation or
  terminology action. See [PRIVACY.md](PRIVACY.md).
- API keys belong in Windows Credential Manager or macOS Keychain, never source,
  logs, screenshots, issues, fixtures, or CI artifacts.
- Official release candidates must be built from a reviewed source revision by
  repository automation, accompanied by checksums and an SBOM, and reviewed under
  [CODE_SIGNING_POLICY.md](CODE_SIGNING_POLICY.md).
- A valid code signature establishes artifact origin and integrity; it is not a
  guarantee that software is vulnerability-free.

## Dependency policy

CI checks the locked Rust dependency graph against the current RustSec advisory
database. Known vulnerabilities, unsoundness advisories, yanked packages, and an
unmaintained crate selected directly by this project fail the check. A
maintenance-only advisory in a framework's transitive dependency remains visible
but does not by itself fail a release when upstream offers no safe upgrade.

Dependabot monitors npm, Cargo, and GitHub Actions dependencies weekly. A
maintainer must review dependency updates and must not suppress a vulnerability
solely to make CI pass. Any exception must identify the exact advisory, explain
why the affected code is unreachable or otherwise mitigated, and state a review
or removal condition in `deny.toml`.

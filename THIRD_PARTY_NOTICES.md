# Third-party notices

Paper Float Translator is licensed under Apache License 2.0. It uses third-party
open-source components that remain subject to their own copyright notices and
license terms. Nothing in the project's Apache-2.0 license replaces those terms.

The lockfiles are the authoritative inventories for an exact source revision:

- `package-lock.json` records JavaScript dependencies.
- `apps/desktop/src-tauri/Cargo.lock` records Rust dependencies.
- Release candidates include an SPDX software bill of materials generated from
  the built artifacts and the checked-out source tree.

## Direct runtime dependencies

| Component | Purpose | License declared upstream |
| --- | --- | --- |
| React and React DOM | Settings and popup user interface | MIT |
| Lucide React | User-interface icons | ISC |
| Tauri API, runtime, plugins, CLI, and build tooling | Desktop runtime and packaging | Apache-2.0 OR MIT |
| arboard | Clipboard integration | MIT OR Apache-2.0 |
| reqwest | Direct HTTPS requests to the configured DeepSeek API endpoint | MIT OR Apache-2.0 |
| serde and serde_json | Local settings, cache, glossary, and API serialization | MIT OR Apache-2.0 |
| uiautomation | Windows UI Automation selection access | Apache-2.0 |
| windows and windows-native-keyring-store | Windows APIs and Credential Manager integration | MIT OR Apache-2.0 |
| keyring-core | Cross-platform credential-store abstraction | MIT OR Apache-2.0 |
| atomic-write-file | Atomic local-state persistence | BSD-3-Clause |

The complete transitive dependency graph also contains components under common
open-source licenses including 0BSD, Apache-2.0, BSD-2-Clause, BSD-3-Clause,
BSL-1.0, CC0-1.0, CC-BY-3.0, CC-BY-4.0, CDLA-Permissive-2.0, ISC, MIT, MIT-0, MPL-2.0,
Unicode-3.0, Unlicense, WTFPL, and Zlib. CI rejects a new npm or Cargo license
until a maintainer explicitly reviews and adds it to the repository policy.

Full package-specific copyright, notice, and license text for the components
actually distributed with the application is generated into
`THIRD_PARTY_LICENSES_NPM.txt` and `THIRD_PARTY_LICENSES_RUST.html`. Both files are
included in the installer and release bundle. The Apache-2.0 license for Paper
Float Translator itself is in `LICENSE`; project attribution is in `NOTICE`.

## Platform components and network services

- Microsoft Edge WebView2 and Windows system libraries are platform components,
  not project-owned code. If WebView2 is absent, the Windows bootstrapper may
  download Microsoft's runtime under Microsoft's terms.
- macOS system frameworks are platform components supplied by Apple.
- DeepSeek is a user-configured network service, not bundled software. DeepSeek's
  name and service remain subject to its own terms and privacy policy. Paper Float
  Translator is not affiliated with or endorsed by DeepSeek.

## Project assets

Unless a file says otherwise, repository documentation, test fixtures, and
project artwork are distributed with Paper Float Translator under Apache-2.0.
The license does not grant trademark rights beyond the limited use described in
Apache-2.0 section 6.

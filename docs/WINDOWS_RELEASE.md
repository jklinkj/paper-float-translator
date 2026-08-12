# Windows release, installation, and removal

## Trust status

The current Windows beta is unsigned. A locally built installer will normally
show an unknown-publisher or SmartScreen warning and must not be described as a
trusted signed release. The project is preparing a SignPath Foundation
application; see the [Code signing policy](../CODE_SIGNING_POLICY.md).

## Install and upgrade

The Tauri build produces a per-user NSIS `-setup.exe`. The installer lets the user
choose an install directory and creates an `uninstall.exe` inside that directory,
plus the normal Windows uninstall registration. There is no separate uninstall
download.

For a development or unsigned beta build:

1. Verify the SHA-256 checksum supplied with the release candidate.
2. If replacing an older installation whose uninstall registration is missing,
   run that installation's `uninstall.exe` first.
3. Run the new `-setup.exe`, read the license and privacy notice, and choose the
   destination directory. Installing under `D:` is supported.
4. Launch the app, configure a DeepSeek API key, and start with deliberate
   `Ctrl+Alt+T` selection capture. Enable experimental automatic selection only if
   wanted.

Never infer that a warning can be ignored solely because a file is linked from a
project discussion. Verify the expected source revision and checksum.

## Uninstall and complete removal

Use **Settings > Apps > Installed apps > Paper Float Translator > Uninstall**, or
run `uninstall.exe` from the selected install directory. The executable removes
the application; it is the project's automated uninstallation facility.

Normal uninstall deliberately preserves settings for upgrades and reinstalls. To
remove user data completely:

1. Before uninstalling, open Paper Float Translator Settings.
2. Choose **Clear cache**, then **Clear Key**. This removes cached source/translated
   text and the API key from Windows Credential Manager.
3. Quit the app from the system tray.
4. Run the uninstaller.
5. Delete `%APPDATA%\com.paperfloat.translator` if you also want to remove settings
   and glossary data.

Deleting that data directory is irreversible. Do not delete a broader `%APPDATA%`
directory. An installer on `D:` does not move the per-user data directory out of
the Windows profile.

## Release-candidate workflow

Official release candidates are built by `.github/workflows/windows-release-candidate.yml`.
The workflow is intentionally manual and produces an artifact bundle rather than
publishing a release automatically. It must:

- verify that npm, Cargo, and Tauri versions agree;
- install from `package-lock.json` and `Cargo.lock`;
- run tests plus dependency advisory, open-source license, and source checks;
- build the NSIS package from a clean GitHub-hosted Windows runner;
- include `LICENSE`, `NOTICE`, `THIRD_PARTY_NOTICES.md`, the generated npm and
  Rust full-license files, source revision, and SHA-256 checksums; and
- generate an SPDX SBOM.

After SignPath accepts the project, its trusted-build connector and manual signing
approval are added using the organization/project identifiers issued by SignPath.
Those identifiers cannot be invented or committed in advance. See
[SIGNPATH_APPLICATION.md](SIGNPATH_APPLICATION.md).

## Pre-publication verification

For every signed release, check both the app executable and installer with
`Get-AuthenticodeSignature` and `signtool verify /pa /v`, then test on a clean
Windows VM:

- fresh install and first launch;
- license/privacy notice and chosen install directory;
- deliberate selection, translation, popup close suppression, copy, and tray quit;
- upgrade from the previous release;
- uninstall and optional complete data removal; and
- absence of leftover app processes or unexpected system changes.

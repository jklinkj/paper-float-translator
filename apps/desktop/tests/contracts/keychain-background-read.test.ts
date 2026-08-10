import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const NATIVE_BRIDGE_PATH = fileURLToPath(
  new URL("../../src-tauri/native/PaperFloatNativeBridge.m", import.meta.url)
);
const RUST_LIB_PATH = fileURLToPath(new URL("../../src-tauri/src/lib.rs", import.meta.url));
const LOCAL_BUILD_SCRIPT_PATH = fileURLToPath(
  new URL("../../scripts/build-mac-app-local-signed.sh", import.meta.url)
);
const LOCAL_PACKAGE_PATH = fileURLToPath(new URL("../../package.json", import.meta.url));
const RELEASE_VERIFIER_PATH = fileURLToPath(
  new URL("../../scripts/verify-mac-release.sh", import.meta.url)
);
const SIGN_SCRIPT_PATH = fileURLToPath(
  new URL("../../scripts/sign-mac-app.sh", import.meta.url)
);

function functionBody(source: string, startMarker: string, endMarker: string): string {
  const start = source.indexOf(startMarker);
  const end = source.indexOf(endMarker, start + startMarker.length);
  expect(start, `missing start marker: ${startMarker}`).toBeGreaterThanOrEqual(0);
  expect(end, `missing end marker after: ${startMarker}`).toBeGreaterThan(start);
  return source.slice(start, end);
}

describe("macOS Keychain background-read contract", () => {
  it("checks startup status without decrypting the stored secret", () => {
    const source = readFileSync(NATIVE_BRIDGE_PATH, "utf8");
    const presenceProbe = functionBody(
      source,
      "int paper_float_keychain_contains(",
      "int paper_float_keychain_get("
    );

    expect(presenceProbe).toContain("kSecReturnAttributes");
    expect(presenceProbe).not.toContain("kSecReturnData");
    expect(presenceProbe).not.toContain("kSecValueData");
    expect(presenceProbe).toContain("interactionNotAllowed = YES");
    expect(presenceProbe).toContain("kSecUseAuthenticationUIFail");
    expect(presenceProbe).toMatch(/SecItemCopyMatching\([\s\S]*query/);

    const rustSource = readFileSync(RUST_LIB_PATH, "utf8");
    const statusReader = functionBody(
      rustSource,
      "fn get_api_key_status_for_settings(data_dir: &Path)",
      "fn set_api_key("
    );
    expect(statusReader).toContain("keychain_secret_exists");
    expect(statusReader).not.toContain("get_api_key()");
  });

  it("keeps full secret reads noninteractive", () => {
    const source = readFileSync(NATIVE_BRIDGE_PATH, "utf8");
    const getter = functionBody(
      source,
      "int paper_float_keychain_get(",
      "int paper_float_keychain_set("
    );

    expect(getter).toContain("LAContext");
    expect(getter).toContain("interactionNotAllowed = YES");
    expect(getter).toContain("kSecUseAuthenticationContext");
    expect(getter).toContain("kSecUseAuthenticationUIFail");
    expect(getter).toContain("kSecUseAuthenticationUI");
    expect(getter).toContain("legacyAuthenticationUIFail");
    expect(getter).toMatch(/SecItemCopyMatching\([\s\S]*query/);
  });

  it("creates and refreshes an ACL that explicitly trusts the signed app", () => {
    const source = readFileSync(NATIVE_BRIDGE_PATH, "utf8");
    const setter = functionBody(
      source,
      "int paper_float_keychain_set(",
      "int paper_float_keychain_delete("
    );

    expect(setter).toContain("SecItemUpdate");
    expect(setter).toContain("SecItemAdd");
    expect(setter).toContain("SecTrustedApplicationCreateFromPath(NULL");
    expect(setter).toContain("SecAccessCreate");
    expect(setter).toContain("kSecAttrAccess");
    expect(setter).toContain("trustedApplications");
  });

  it("keeps production on v2 Keychain while local builds bypass Keychain entirely", () => {
    const rustSource = readFileSync(RUST_LIB_PATH, "utf8");

    expect(rustSource).toContain(
      'const KEYCHAIN_SERVICE: &str = "Paper Float Translator API Key v2";'
    );
    expect(rustSource).toContain('const DEEPSEEK_ACCOUNT: &str = "deepseek-api-key-v2";');

    const getter = functionBody(
      rustSource,
      "fn get_api_key(data_dir: &Path)",
      "fn summarize_api_key_presence_for_settings("
    );
    expect(getter).toContain('#[cfg(feature = "local-api-key-file")]');
    expect(getter).toContain("read_local_api_key(data_dir)");
    expect(getter).toContain("KEYCHAIN_SERVICE");
    expect(getter).toContain("DEEPSEEK_ACCOUNT");
    expect(getter).not.toContain('"Paper Float Translator"');
    expect(getter).not.toContain('"deepseek-api-key"');

    const localBuildScript = readFileSync(LOCAL_BUILD_SCRIPT_PATH, "utf8");
    const localPackage = JSON.parse(readFileSync(LOCAL_PACKAGE_PATH, "utf8")) as {
      scripts?: Record<string, string>;
    };
    const releaseVerifier = readFileSync(RELEASE_VERIFIER_PATH, "utf8");
    const signScript = readFileSync(SIGN_SCRIPT_PATH, "utf8");
    expect(localBuildScript).toContain("--features local-api-key-file");
    expect(localBuildScript).toContain('LOCAL_TARGET_DIR="$APP_ROOT/src-tauri/target/local-api-key-file"');
    expect(localBuildScript).toContain('CARGO_TARGET_DIR="$LOCAL_TARGET_DIR"');
    expect(localBuildScript).not.toMatch(/--features\s+acceptance-testing/);
    expect(localPackage.scripts?.["build:mac:app:local"]).toContain(
      "CARGO_TARGET_DIR=src-tauri/target/local-api-key-file"
    );
    expect(rustSource).toContain("PAPER_FLOAT_ARTIFACT_CLASS_LOCAL_API_KEY_FILE_V1");
    expect(releaseVerifier).toContain("local-api-key-file artifact marker is present");
    expect(signScript).toContain("local-api-key-file diagnostics cannot receive a release identity");
  });
});

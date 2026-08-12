import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const DEFAULT_CONFIG_PATH = fileURLToPath(
  new URL("../../src-tauri/tauri.conf.json", import.meta.url)
);
const ACCEPTANCE_CONFIG_PATH = fileURLToPath(
  new URL("../../src-tauri/tauri.acceptance.conf.json", import.meta.url)
);
const PACKAGE_PATH = fileURLToPath(new URL("../../package.json", import.meta.url));
const LIB_PATH = fileURLToPath(new URL("../../src-tauri/src/lib.rs", import.meta.url));
const VERIFIER_PATH = fileURLToPath(
  new URL("../../scripts/verify-mac-artifact-isolation.sh", import.meta.url)
);

interface TauriWindowContract {
  label: string;
  title: string;
  url: string;
  width: number;
  height: number;
  [key: string]: unknown;
}

interface TauriConfigContract {
  productName: string;
  identifier: string;
  app?: { windows?: TauriWindowContract[] };
}

const withoutWindowTitle = (window: TauriWindowContract): Record<string, unknown> => {
  const contract: Record<string, unknown> = { ...window };
  delete contract.title;
  return contract;
};

const RUN_CONTEXT_MARKER = "let context = tauri::generate_context!();";
const BUILDER_MARKER = "tauri::Builder::default()";
const SETUP_MARKER = ".setup(move |app| {";

const readUtf8WithLf = (path: string): string =>
  readFileSync(path, "utf8").replace(/\r\n/g, "\n");

const RUN_PRE_BUILDER_IDENTITY_GUARD =
  /^let context = tauri::generate_context!\(\);\s*if let Err\(error\) = validate_compiled_runtime_identity\(\s*&context\.config\(\)\.identifier,\s*&context\.package_info\(\)\.name,\s*\) \{\s*eprintln!\("\{error\}"\);\s*return;\s*\}/s;

const SETUP_IDENTITY_GUARD =
  /^\s*if let Err\(error\) = validate_compiled_runtime_identity\(\s*&app\.config\(\)\.identifier,\s*&app\.package_info\(\)\.name,\s*\) \{\s*return Err\(std::io::Error::other\(error\)\.into\(\)\);\s*\}/s;

const hasUnconditionalPreBuilderIdentityGuard = (source: string): boolean => {
  const contextGeneration = source.indexOf(RUN_CONTEXT_MARKER);
  const builderStart = source.indexOf(BUILDER_MARKER, contextGeneration);
  if (contextGeneration < 0 || builderStart <= contextGeneration) {
    return false;
  }

  return RUN_PRE_BUILDER_IDENTITY_GUARD.test(
    source.slice(contextGeneration, builderStart)
  );
};

const hasUnconditionalSetupIdentityGuard = (source: string): boolean => {
  const setupStart = source.indexOf(SETUP_MARKER);
  if (setupStart < 0) {
    return false;
  }

  return SETUP_IDENTITY_GUARD.test(source.slice(setupStart + SETUP_MARKER.length));
};

describe("acceptance artifact isolation contract", () => {
  it("freezes distinct product, bundle, and visible window identities", () => {
    const production = JSON.parse(readFileSync(DEFAULT_CONFIG_PATH, "utf8")) as TauriConfigContract;
    const acceptance = JSON.parse(
      readFileSync(ACCEPTANCE_CONFIG_PATH, "utf8")
    ) as TauriConfigContract;

    expect(production.productName).toBe("Paper Float Translator");
    expect(production.identifier).toBe("com.paperfloat.translator");
    expect(acceptance.productName).toBe("Paper Float Translator Acceptance");
    expect(acceptance.identifier).toBe("com.paperfloat.translator.acceptance");
    expect(acceptance.productName).not.toBe(production.productName);
    expect(acceptance.identifier).not.toBe(production.identifier);

    const productionWindows = production.app?.windows ?? [];
    const acceptanceWindows = acceptance.app?.windows ?? [];
    expect(productionWindows.length).toBeGreaterThan(0);
    expect(acceptanceWindows).toHaveLength(productionWindows.length);
    expect(acceptanceWindows.map(withoutWindowTitle)).toEqual(
      productionWindows.map(withoutWindowTitle)
    );
    expect(acceptanceWindows.every((window) => window.title === acceptance.productName)).toBe(true);

    const mutatedAcceptanceWindows = acceptanceWindows.map((window, index) =>
      index === 0 ? { ...window, visible: window.visible !== true } : window
    );
    expect(mutatedAcceptanceWindows.map(withoutWindowTitle)).not.toEqual(
      productionWindows.map(withoutWindowTitle)
    );
  });

  it("fail-closes the artifact verifier and freezes signed bundle evidence", () => {
    const script = readFileSync(VERIFIER_PATH, "utf8");

    for (const plistKey of [
      "CFBundleIdentifier",
      "CFBundleName",
      "CFBundleDisplayName",
      "CFBundleExecutable",
    ]) {
      expect(script).toContain(plistKey);
    }

    expect(script).toContain("capture_stdout_or_reject");
    expect(script).toContain('if ! "$@" >"$output_path"; then');
    expect(script).toContain('capture_stdout_or_reject "$default_symbols" "default nm" /usr/bin/nm "$default_binary"');
    expect(script).toContain('capture_stdout_or_reject "$acceptance_symbols" "acceptance nm" /usr/bin/nm "$acceptance_binary"');
    expect(script).toContain('capture_stdout_or_reject "$default_strings" "default strings" /usr/bin/strings -a "$default_binary"');
    expect(script).toContain('capture_stdout_or_reject "$acceptance_strings" "acceptance strings" /usr/bin/strings -a "$acceptance_binary"');
    expect(script).toContain('capture_stdout_or_reject "$default_sha_output" "default SHA-256" /usr/bin/shasum -a 256 "$default_binary"');
    expect(script).toContain('capture_stdout_or_reject "$acceptance_sha_output" "acceptance SHA-256" /usr/bin/shasum -a 256 "$acceptance_binary"');
    expect(script).not.toMatch(/\/usr\/bin\/(?:nm|strings)[^\n]*\|/);

    expect(script).toContain("capture_codesign_metadata_or_reject");
    expect(script).toContain("capture_designated_requirement_or_reject");
    expect(script).toContain("/usr/bin/codesign -d -r-");
    expect(script).toContain("default_codesign_identifier");
    expect(script).toContain("acceptance_codesign_identifier");
    expect(script).toContain("default_cdhash");
    expect(script).toContain("acceptance_cdhash");
    expect(script).toContain("defaultDesignatedRequirement=");
    expect(script).toContain("acceptanceDesignatedRequirement=");
    expect(script).toContain("identical default and acceptance binaries");
    expect(script).toMatch(/paper_float_test_\|inject_acceptance_/);
  });

  it("builds acceptance into a dedicated target with the dedicated config", () => {
    const packageJson = JSON.parse(readFileSync(PACKAGE_PATH, "utf8")) as {
      scripts?: Record<string, string>;
    };
    const script = packageJson.scripts?.["build:mac:app:acceptance"] ?? "";
    const defaultScript = packageJson.scripts?.["build:mac:app"] ?? "";

    expect(script).toContain("CARGO_TARGET_DIR=target/acceptance");
    expect(script).toContain("--features acceptance-testing");
    expect(script).toContain("--config src-tauri/tauri.acceptance.conf.json");
    expect(defaultScript).not.toContain("acceptance-testing");
    expect(defaultScript).not.toContain("tauri.acceptance.conf.json");
  });

  it("guards both compiled artifact identities before Builder and again in setup", () => {
    const source = readUtf8WithLf(LIB_PATH);
    const validatorStart = source.indexOf("fn validate_compiled_runtime_identity(");
    const validatorDeclarationPrefix = source.slice(
      Math.max(0, validatorStart - 160),
      validatorStart
    );
    const setupStart = source.indexOf(SETUP_MARKER);
    const setupEnd = source.indexOf("Ok(())", setupStart);
    const setup = source.slice(setupStart, setupEnd);

    const builderStart = source.indexOf(BUILDER_MARKER, source.indexOf(RUN_CONTEXT_MARKER));
    const preBuilder = source.slice(source.indexOf(RUN_CONTEXT_MARKER), builderStart);

    const acceptanceDataResolution = preBuilder.indexOf(
      "let data_dir = match expected_acceptance_data_dir()"
    );
    const dataPreparation = preBuilder.indexOf("prepare_acceptance_data_dir(&data_dir)");
    const productionDataResolution = preBuilder.indexOf("let data_dir = dirs::data_dir()");
    const dataCreation = preBuilder.indexOf("ensure_data_dir(&data_dir)");
    const runtimeDataResolution = setup.indexOf("app.path().app_data_dir()");
    const watcherStart = setup.indexOf("restart_platform_watchers");

    expect(source).toMatch(
      /#\[cfg\(not\(feature = "acceptance-testing"\)\)\]\nconst EXPECTED_PRODUCT_NAME: &str = "Paper Float Translator";/
    );
    expect(source).toMatch(
      /#\[cfg\(not\(feature = "acceptance-testing"\)\)\]\nconst EXPECTED_BUNDLE_IDENTIFIER: &str = "com\.paperfloat\.translator";/
    );
    expect(source).toMatch(
      /#\[cfg\(feature = "acceptance-testing"\)\]\nconst EXPECTED_PRODUCT_NAME: &str = "Paper Float Translator Acceptance";/
    );
    expect(source).toMatch(
      /#\[cfg\(feature = "acceptance-testing"\)\]\nconst EXPECTED_BUNDLE_IDENTIFIER: &str = "com\.paperfloat\.translator\.acceptance";/
    );
    expect(source).toMatch(
      /fn validate_compiled_runtime_identity\(\s*bundle_identifier: &str,\s*product_name: &str,\s*\) -> Result<\(\), String> \{\s*if bundle_identifier != EXPECTED_BUNDLE_IDENTIFIER\s*\|\| product_name != EXPECTED_PRODUCT_NAME/s
    );
    expect(validatorStart).toBeGreaterThanOrEqual(0);
    expect(validatorDeclarationPrefix).not.toMatch(/#\[cfg\([^\n]+\)\]\s*$/);

    expect(hasUnconditionalPreBuilderIdentityGuard(source)).toBe(true);
    expect(hasUnconditionalSetupIdentityGuard(source)).toBe(true);
    expect(acceptanceDataResolution).toBeGreaterThanOrEqual(0);
    expect(acceptanceDataResolution).toBeLessThan(dataPreparation);
    expect(dataPreparation).toBeLessThan(productionDataResolution);
    expect(productionDataResolution).toBeLessThan(dataCreation);
    expect(dataCreation).toBeLessThan(preBuilder.length);
    expect(runtimeDataResolution).toBeGreaterThanOrEqual(0);
    expect(runtimeDataResolution).toBeLessThan(watcherStart);

    const reverseMismatchMutation = source.replace(
      "    if let Err(error) = validate_compiled_runtime_identity(\n        &context.config().identifier,",
      "    #[cfg(feature = \"acceptance-testing\")]\n    if let Err(error) = validate_compiled_runtime_identity(\n        &context.config().identifier,"
    );
    expect(reverseMismatchMutation).not.toBe(source);
    expect(hasUnconditionalPreBuilderIdentityGuard(reverseMismatchMutation)).toBe(false);
  });

  it("keeps acceptance Keychain namespaces behind compile-time feature gates", () => {
    const source = readUtf8WithLf(LIB_PATH);

    expect(source).toMatch(
      /#\[cfg\(all\(\s*not\(feature = "acceptance-testing"\),\s*not\(feature = "local-api-key-file"\)\s*\)\)\]\s*const KEYCHAIN_SERVICE: &str = "Paper Float Translator API Key v2";/
    );
    expect(source).toMatch(
      /#\[cfg\(feature = "acceptance-testing"\)\]\nconst KEYCHAIN_SERVICE: &str = "Paper Float Translator Acceptance API Key v2";/
    );
    expect(source).toMatch(
      /#\[cfg\(feature = "acceptance-testing"\)\]\nconst DEEPSEEK_ACCOUNT: &str = "deepseek-api-key-acceptance-v2";/
    );
  });
});

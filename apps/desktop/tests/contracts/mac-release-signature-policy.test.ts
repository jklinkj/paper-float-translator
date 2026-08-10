import {
  chmodSync,
  existsSync,
  readFileSync,
  mkdirSync,
  mkdtempSync,
  rmSync,
  writeFileSync
} from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";
import { afterEach, describe, expect, it } from "vitest";

const POLICY_PATH = fileURLToPath(
  new URL("../../scripts/mac-release-signature-policy.sh", import.meta.url)
);
const VERIFIER_PATH = fileURLToPath(
  new URL("../../scripts/verify-mac-release.sh", import.meta.url)
);

const DEVELOPER_ID_METADATA = [
  "Executable=/tmp/Paper Float Translator",
  "Identifier=com.paperfloat.translator",
  "Authority=Developer ID Application: Paper Float LLC (ABCDE12345)",
  "Authority=Developer ID Certification Authority",
  "Authority=Apple Root CA",
  "TeamIdentifier=ABCDE12345"
].join("\n");
const VALID_DEVELOPER_ID_REQUIREMENT = [
  "Executable=/tmp/Paper Float Translator",
  [
    'designated => identifier "com.paperfloat.translator" and anchor apple generic',
    "and certificate 1[field.1.2.840.113635.100.6.2.6] exists",
    "and certificate leaf[field.1.2.840.113635.100.6.1.13] exists",
    'and certificate leaf[subject.OU] = "ABCDE12345"'
  ].join(" ")
].join("\n");
const STRICT_DEVELOPER_ID_REQUIREMENT = [
  'identifier "com.paperfloat.translator"',
  "anchor apple generic",
  "certificate 1[field.1.2.840.113635.100.6.2.6] exists",
  "certificate leaf[field.1.2.840.113635.100.6.1.13] exists",
  'certificate leaf[subject.OU] = "ABCDE12345"'
].join(" and ");

const temporaryDirectories: string[] = [];

afterEach(() => {
  for (const directory of temporaryDirectories.splice(0)) {
    rmSync(directory, { recursive: true, force: true });
  }
});

function runPolicy(command: string, input: string, ...args: string[]) {
  return spawnSync("bash", [POLICY_PATH, command, ...args], {
    encoding: "utf8",
    input
  });
}

function runVerifier(
  signatureMetadata: string,
  designatedRequirement: string,
  local = false,
  acceptanceSymbols = false,
  localApiKeyArtifact = false,
  legacyLocalApiKeyArtifact = false
) {
  const fixtureRoot = mkdtempSync(join(tmpdir(), "paper-float-signature-policy-"));
  temporaryDirectories.push(fixtureRoot);
  const binDirectory = join(fixtureRoot, "bin");
  const appPath = join(fixtureRoot, "Paper Float Translator.app");
  const codesignLogPath = join(fixtureRoot, "codesign.log");
  mkdirSync(binDirectory);
  mkdirSync(join(appPath, "Contents", "MacOS"), { recursive: true });
  writeFileSync(
    join(appPath, "Contents", "MacOS", "paper-float-translator"),
    [
      acceptanceSymbols ? "_paper_float_test_inject_tap_disabled" : "release-fixture",
      localApiKeyArtifact ? "PAPER_FLOAT_ARTIFACT_CLASS_LOCAL_API_KEY_FILE_V1" : "",
      legacyLocalApiKeyArtifact ? "api-key.local" : ""
    ].filter(Boolean).join("\n")
  );

  const codesignPath = join(binDirectory, "codesign");
  writeFileSync(
    codesignPath,
    `#!/usr/bin/env bash
set -euo pipefail
printf '%s\\n' "\$*" >> "\${FAKE_CODESIGN_LOG}"
if [[ "\${1:-}" == "-dvvv" ]]; then
  printf '%s\\n' "\${FAKE_SIGNATURE_METADATA}"
elif [[ "\${1:-}" == "-d" && "\${2:-}" == "-r-" ]]; then
  printf '%s\\n' "\${FAKE_DESIGNATED_REQUIREMENT}"
fi
`
  );
  chmodSync(codesignPath, 0o755);

  for (const executable of ["spctl", "xcrun"]) {
    const path = join(binDirectory, executable);
    writeFileSync(path, "#!/usr/bin/env bash\nexit 0\n");
    chmodSync(path, 0o755);
  }

  const result = spawnSync("bash", [VERIFIER_PATH, appPath], {
    encoding: "utf8",
    env: {
      ...process.env,
      PATH: `${binDirectory}:${process.env.PATH ?? ""}`,
      FAKE_SIGNATURE_METADATA: signatureMetadata,
      FAKE_DESIGNATED_REQUIREMENT: designatedRequirement,
      FAKE_CODESIGN_LOG: codesignLogPath,
      ...(local ? { ALLOW_ADHOC_MAC_VERIFY: "1" } : {})
    }
  });

  return {
    ...result,
    codesignCalls: existsSync(codesignLogPath) ? readFileSync(codesignLogPath, "utf8") : ""
  };
}

describe("macOS release signature policy", () => {
  it.each([
    [DEVELOPER_ID_METADATA, "developer_id_application"],
    ["Signature=adhoc\nTeamIdentifier=not set", "adhoc"],
    ["Authority=Apple Development: Developer (ABCDE12345)\nTeamIdentifier=ABCDE12345", "apple_development"],
    ["Authority=Apple Distribution: Example (ABCDE12345)\nTeamIdentifier=ABCDE12345", "unknown"],
    ["TeamIdentifier=ABCDE12345", "unknown"]
  ])("classifies injected codesign metadata", (metadata, expected) => {
    const result = runPolicy("classify-signature", metadata);
    expect(result.status).toBe(0);
    expect(result.stdout.trim()).toBe(expected);
  });

  it("accepts only a Developer ID designated requirement bound to the team", () => {
    const accepted = runPolicy(
      "validate-developer-id-requirement",
      VALID_DEVELOPER_ID_REQUIREMENT,
      "ABCDE12345",
      "com.paperfloat.translator"
    );
    expect(accepted.status).toBe(0);
    expect(accepted.stdout.trim()).toBe("valid");

    for (const rejected of [
      'designated => cdhash H"a93a10facb634e91d45cbe0bec5e4aabcf5913f4"',
      'designated => identifier "com.paperfloat.translator" and anchor apple generic',
      'designated => identifier "com.paperfloat.translator" and anchor apple generic and certificate leaf[field.1.2.840.113635.100.6.1.13] exists and certificate leaf[subject.OU] = "ABCDE12345"',
      [
        VALID_DEVELOPER_ID_REQUIREMENT,
        "or true"
      ].join(" "),
      [
        'designated => identifier "wrong.bundle.identifier" and anchor apple generic',
        "and certificate 1[field.1.2.840.113635.100.6.2.6] exists",
        "and certificate leaf[field.1.2.840.113635.100.6.1.13] exists",
        'and certificate leaf[subject.OU] = "ABCDE12345"'
      ].join(" "),
      ""
    ]) {
      const result = runPolicy(
        "validate-developer-id-requirement",
        rejected,
        "ABCDE12345",
        "com.paperfloat.translator"
      );
      expect(result.status).toBe(1);
      expect(result.stdout.trim()).toBe("invalid");
    }

    const wrongTeam = runPolicy(
      "validate-developer-id-requirement",
      VALID_DEVELOPER_ID_REQUIREMENT,
      "ZZZZZ99999",
      "com.paperfloat.translator"
    );
    expect(wrongTeam.status).toBe(1);
    expect(wrongTeam.stdout.trim()).toBe("invalid");
  });

  it("constructs a strict release requirement from trusted policy inputs", () => {
    const result = runPolicy(
      "build-developer-id-requirement",
      "",
      "com.paperfloat.translator",
      "ABCDE12345"
    );
    expect(result.status).toBe(0);
    expect(result.stdout.trim()).toBe(STRICT_DEVELOPER_ID_REQUIREMENT);

    const unsafe = runPolicy(
      "build-developer-id-requirement",
      "",
      'com.paperfloat.translator" or true',
      "ABCDE12345"
    );
    expect(unsafe.status).toBe(1);
  });

  it("accepts a Developer ID Application artifact with a valid designated requirement", () => {
    const result = runVerifier(DEVELOPER_ID_METADATA, VALID_DEVELOPER_ID_REQUIREMENT);
    expect(result.status, result.stderr).toBe(0);
    expect(result.stdout).toContain("signature policy (app): developer_id_application");
    expect(result.codesignCalls).toContain(`-R=${STRICT_DEVELOPER_ID_REQUIREMENT}`);
  });

  it.each([
    ["ad-hoc", "Signature=adhoc\nTeamIdentifier=not set", "adhoc"],
    [
      "Apple Development",
      "Authority=Apple Development: Developer (ABCDE12345)\nTeamIdentifier=ABCDE12345",
      "apple_development"
    ],
    ["unknown", "Authority=Apple Distribution: Example (ABCDE12345)\nTeamIdentifier=ABCDE12345", "unknown"]
  ])("rejects %s signatures in release mode", (_label, metadata, classification) => {
    const result = runVerifier(metadata, VALID_DEVELOPER_ID_REQUIREMENT);
    expect(result.status).toBe(65);
    expect(result.stderr).toContain(`expected Authority=Developer ID Application, got ${classification}`);
  });

  it("rejects Developer ID metadata whose designated requirement is invalid", () => {
    const result = runVerifier(
      DEVELOPER_ID_METADATA,
      'designated => identifier "com.paperfloat.translator" and anchor apple generic'
    );
    expect(result.status).toBe(65);
    expect(result.stderr).toContain("designated requirement is not strictly bound");
  });

  it("rejects a malicious permissive designated requirement despite valid substrings", () => {
    const malicious = `${VALID_DEVELOPER_ID_REQUIREMENT} or true`;
    const result = runVerifier(DEVELOPER_ID_METADATA, malicious);
    expect(result.status).toBe(65);
    expect(result.stderr).toContain("designated requirement is not strictly bound");
  });

  it("rejects a Developer ID app whose signed identifier is not the frozen bundle id", () => {
    const wrongIdentifierMetadata = DEVELOPER_ID_METADATA.replace(
      "Identifier=com.paperfloat.translator",
      "Identifier=com.attacker.replacement"
    );
    const result = runVerifier(wrongIdentifierMetadata, VALID_DEVELOPER_ID_REQUIREMENT);
    expect(result.status).toBe(65);
    expect(result.stderr).toContain("expected Identifier=com.paperfloat.translator");
  });

  it("rejects an acceptance-testing binary even when its Developer ID metadata is valid", () => {
    const result = runVerifier(
      DEVELOPER_ID_METADATA,
      VALID_DEVELOPER_ID_REQUIREMENT,
      false,
      true
    );
    expect(result.status).toBe(65);
    expect(result.stderr).toContain("acceptance-testing native symbols are present");
  });

  it("rejects a local API-key-file binary even after Developer ID re-signing", () => {
    const result = runVerifier(
      DEVELOPER_ID_METADATA,
      VALID_DEVELOPER_ID_REQUIREMENT,
      false,
      false,
      true
    );
    expect(result.status).toBe(65);
    expect(result.stderr).toContain("local-api-key-file artifact marker is present");
  });

  it("rejects a legacy local-file binary built before the explicit marker existed", () => {
    const result = runVerifier(
      DEVELOPER_ID_METADATA,
      VALID_DEVELOPER_ID_REQUIREMENT,
      false,
      false,
      false,
      true
    );
    expect(result.status).toBe(65);
    expect(result.stderr).toContain("local-api-key-file artifact marker is present");
  });

  it("keeps the explicit ad-hoc local diagnostics override non-release", () => {
    const result = runVerifier(
      "Signature=adhoc\nTeamIdentifier=not set",
      'designated => cdhash H"a93a10facb634e91d45cbe0bec5e4aabcf5913f4"',
      true
    );
    expect(result.status, result.stderr).toBe(0);
    expect(result.stdout).toContain("LOCAL DIAGNOSTIC ONLY");
  });
});

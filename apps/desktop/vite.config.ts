import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import license, { type Dependency } from "rollup-plugin-license";

const configDirectory = dirname(fileURLToPath(import.meta.url));
const npmLicenseOutput = resolve(configDirectory, "../..", "THIRD_PARTY_LICENSES_NPM.txt");

function formatNpmLicenseReport(input: Dependency[]): string {
  const dependencies = [...input].sort((left, right) =>
    `${left.name}@${left.version}`.localeCompare(`${right.name}@${right.version}`)
  );

  const sections = dependencies.map((dependency) => {
    if (!dependency.name || !dependency.version || !dependency.license || !dependency.licenseText) {
      throw new Error(
        `Incomplete bundled npm license metadata for ${dependency.name ?? "unknown"}@${dependency.version ?? "unknown"}`
      );
    }

    const repository =
      typeof dependency.repository === "string"
        ? dependency.repository
        : dependency.repository?.url;
    return [
      "================================================================================",
      `${dependency.name}@${dependency.version}`,
      `License: ${dependency.license}`,
      repository ? `Source: ${repository}` : undefined,
      dependency.homepage ? `Homepage: ${dependency.homepage}` : undefined,
      "",
      dependency.licenseText.trim(),
      dependency.noticeText ? `\nNOTICE\n------\n${dependency.noticeText.trim()}` : undefined
    ]
      .filter((line): line is string => line !== undefined)
      .join("\n");
  });

  return [
    "Paper Float Translator - bundled npm third-party licenses",
    "Generated deterministically by rollup-plugin-license from the production renderer bundle.",
    "Project code is licensed separately under Apache-2.0; see LICENSE and NOTICE.",
    "",
    ...sections,
    ""
  ].join("\n");
}

export default defineConfig({
  root: resolve(configDirectory, "renderer"),
  plugins: [
    react(),
    license({
      cwd: configDirectory,
      thirdParty: {
        includePrivate: false,
        includeSelf: false,
        multipleVersions: true,
        allow: {
          test: "(Apache-2.0 OR BSD-2-Clause OR BSD-3-Clause OR ISC OR MIT OR MPL-2.0)",
          failOnUnlicensed: true,
          failOnViolation: true
        },
        output: {
          file: npmLicenseOutput,
          encoding: "utf-8",
          template: formatNpmLicenseReport
        }
      }
    })
  ],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true
  },
  envPrefix: ["VITE_", "TAURI_"],
  build: {
    outDir: resolve(configDirectory, "dist"),
    emptyOutDir: true
  }
});

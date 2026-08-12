import { readFile } from "node:fs/promises";

const lock = JSON.parse(await readFile(new URL("../package-lock.json", import.meta.url), "utf8"));
const allowedIdentifiers = new Set([
  "Apache-2.0",
  "BSD-2-Clause",
  "BSD-3-Clause",
  "CC-BY-3.0",
  "CC-BY-4.0",
  "CC0-1.0",
  "ISC",
  "MIT",
  "MPL-2.0",
  "WTFPL"
]);
const workspaceNames = new Set(
  Object.values(lock.packages)
    .filter((entry) => entry?.link === true)
    .map((entry) => entry?.name)
    .filter(Boolean)
);
const failures = [];
const seenLicenses = new Map();

for (const [packagePath, entry] of Object.entries(lock.packages)) {
  if (!packagePath.startsWith("node_modules/")) {
    continue;
  }

  const name = entry.name ?? packagePath.slice("node_modules/".length);
  if (entry.link === true || workspaceNames.has(name)) {
    continue;
  }

  const expression = entry.license;
  if (typeof expression !== "string" || expression.trim() === "") {
    failures.push(`${name}@${entry.version ?? "unknown"}: missing license metadata`);
    continue;
  }

  const identifiers = expression.match(/[A-Za-z0-9][A-Za-z0-9.+-]*/g) ?? [];
  const licenseIdentifiers = identifiers.filter(
    (identifier) => identifier !== "AND" && identifier !== "OR" && identifier !== "WITH"
  );
  const disallowed = licenseIdentifiers.filter((identifier) => !allowedIdentifiers.has(identifier));
  if (disallowed.length > 0) {
    failures.push(
      `${name}@${entry.version ?? "unknown"}: ${expression} (unapproved: ${[...new Set(disallowed)].join(", ")})`
    );
    continue;
  }

  seenLicenses.set(expression, (seenLicenses.get(expression) ?? 0) + 1);
}

if (failures.length > 0) {
  console.error("npm dependency license policy failed:\n");
  for (const failure of failures) {
    console.error(`- ${failure}`);
  }
  process.exitCode = 1;
} else {
  console.log(`npm dependency license policy passed for ${[...seenLicenses.values()].reduce((a, b) => a + b, 0)} locked packages.`);
  for (const [license, count] of [...seenLicenses.entries()].sort(([left], [right]) =>
    left.localeCompare(right)
  )) {
    console.log(`${String(count).padStart(3)}  ${license}`);
  }
}

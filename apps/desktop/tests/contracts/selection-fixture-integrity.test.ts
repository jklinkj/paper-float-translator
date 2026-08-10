import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const FIXTURES = {
  "selection-baseline.txt": "c91895a9f01b3637f3e34570124ea3c20ac77dff9852e841a732060425ac6a6d",
  "selection-baseline.html": "9dbc666eee63e4d91556a2584de0afcad4e5e9d5649740bcdd9dccc8a6e4ff7b",
  "selection-baseline.pdf": "32e1a2e85170cd4de04c388a816f9e408317237f039f103747f1e14721c26857"
} as const;

describe("selection acceptance fixture integrity", () => {
  it.each(Object.entries(FIXTURES))("keeps %s frozen", (name, expectedHash) => {
    const path = fileURLToPath(new URL(`../fixtures/${name}`, import.meta.url));
    const digest = createHash("sha256").update(readFileSync(path)).digest("hex");
    expect(digest).toBe(expectedHash);
  });
});

import { mkdtemp, rm } from "node:fs/promises";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { JsonCacheStore } from "./cache-store";
import { JsonSettingsStore } from "./settings-store";

let dir: string;

beforeEach(async () => {
  dir = await mkdtemp(join(tmpdir(), "paper-float-translator-"));
});

afterEach(async () => {
  await rm(dir, { recursive: true, force: true });
});

describe("JsonSettingsStore", () => {
  it("loads default settings when no file exists", async () => {
    const store = new JsonSettingsStore(join(dir, "settings.json"));

    await expect(store.load()).resolves.toMatchObject({
      shortcut: "CommandOrControl+Shift+Y",
      model: "deepseek-v4-flash",
      cleanPdfText: true
    });
  });

  it("saves normalized settings", async () => {
    const store = new JsonSettingsStore(join(dir, "settings.json"));
    const settings = await store.update({ model: "deepseek-v4-pro", popupWidth: 999 });

    expect(settings.model).toBe("deepseek-v4-pro");
    expect(settings.popupWidth).toBe(640);
  });
});

describe("JsonCacheStore", () => {
  it("persists cache entries", async () => {
    const store = new JsonCacheStore(join(dir, "cache.json"));
    await store.set({
      key: "key",
      cleanedText: "hello",
      translation: "你好",
      model: "deepseek-v4-flash",
      mode: "academic_zh",
      glossaryVersion: "v1",
      createdAt: "2026-04-27T00:00:00.000Z"
    });

    await expect(store.get("key")).resolves.toMatchObject({ translation: "你好" });
  });
});

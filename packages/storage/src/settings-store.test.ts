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
      triggerMode: "clipboard_shortcut",
      doubleCopyWindowMs: 1200,
      cleanPdfText: true
    });
  });

  it("fills trigger mode defaults for old settings files", async () => {
    const store = new JsonSettingsStore(join(dir, "settings.json"));
    const settings = await store.save({
      shortcut: "CommandOrControl+Shift+Y",
      model: "deepseek-v4-flash",
      mode: "academic_zh",
      cleanPdfText: true,
      enableCache: true,
      popupWidth: 420
    } as unknown as Awaited<ReturnType<JsonSettingsStore["load"]>>);

    expect(settings.triggerMode).toBe("clipboard_shortcut");
    expect(settings.doubleCopyWindowMs).toBe(1200);
  });

  it("saves normalized settings", async () => {
    const store = new JsonSettingsStore(join(dir, "settings.json"));
    const settings = await store.update({ model: "deepseek-v4-pro", popupWidth: 999 });

    expect(settings.model).toBe("deepseek-v4-pro");
    expect(settings.popupWidth).toBe(640);
  });

  it("accepts popup width values that arrive as numeric strings", async () => {
    const store = new JsonSettingsStore(join(dir, "settings.json"));
    const settings = await store.update({ popupWidth: "520" as unknown as number });

    expect(settings.popupWidth).toBe(520);
  });

  it("normalizes double copy window values", async () => {
    const store = new JsonSettingsStore(join(dir, "settings.json"));
    const settings = await store.update({
      triggerMode: "mac_double_copy",
      doubleCopyWindowMs: "1800" as unknown as number
    });

    expect(settings.triggerMode).toBe("mac_double_copy");
    expect(settings.doubleCopyWindowMs).toBe(1800);
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

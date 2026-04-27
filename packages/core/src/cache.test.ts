import { describe, expect, it } from "vitest";
import { createCacheKey, createGlossaryVersion } from "./cache";

describe("cache keys", () => {
  it("is stable for equivalent input", async () => {
    const left = await createCacheKey({
      model: "deepseek-v4-flash",
      mode: "academic_zh",
      targetLanguage: "中文",
      glossaryVersion: "v1",
      cleanedText: "hello"
    });
    const right = await createCacheKey({
      model: "deepseek-v4-flash",
      mode: "academic_zh",
      targetLanguage: "中文",
      glossaryVersion: "v1",
      cleanedText: "hello"
    });

    expect(left).toBe(right);
  });

  it("changes when model changes", async () => {
    const flash = await createCacheKey({
      model: "deepseek-v4-flash",
      mode: "academic_zh",
      targetLanguage: "中文",
      glossaryVersion: "v1",
      cleanedText: "hello"
    });
    const pro = await createCacheKey({
      model: "deepseek-v4-pro",
      mode: "academic_zh",
      targetLanguage: "中文",
      glossaryVersion: "v1",
      cleanedText: "hello"
    });

    expect(flash).not.toBe(pro);
  });

  it("changes when target language changes", async () => {
    const chinese = await createCacheKey({
      model: "deepseek-v4-flash",
      mode: "academic_zh",
      targetLanguage: "中文",
      glossaryVersion: "v1",
      cleanedText: "hello"
    });
    const japanese = await createCacheKey({
      model: "deepseek-v4-flash",
      mode: "academic_zh",
      targetLanguage: "日文",
      glossaryVersion: "v1",
      cleanedText: "hello"
    });

    expect(chinese).not.toBe(japanese);
  });

  it("normalizes glossary version by key order", async () => {
    const left = await createGlossaryVersion({ b: "乙", a: "甲" });
    const right = await createGlossaryVersion({ a: "甲", b: "乙" });

    expect(left).toBe(right);
  });
});

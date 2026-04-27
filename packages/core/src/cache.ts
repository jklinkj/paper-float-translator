import { normalizeTargetLanguage, type DeepSeekModel, type TranslateMode } from "./types";

export interface CacheKeyInput {
  model: DeepSeekModel;
  mode: TranslateMode;
  targetLanguage: string;
  glossaryVersion: string;
  cleanedText: string;
}

export async function createCacheKey(input: CacheKeyInput): Promise<string> {
  const payload = JSON.stringify({
    model: input.model,
    mode: input.mode,
    targetLanguage: normalizeTargetLanguage(input.targetLanguage),
    glossaryVersion: input.glossaryVersion,
    cleanedText: input.cleanedText
  });

  return sha256(payload);
}

export async function createGlossaryVersion(glossary: Record<string, string>): Promise<string> {
  const sortedEntries = Object.entries(glossary)
    .filter(([source, target]) => source.trim() && target.trim())
    .sort(([left], [right]) => left.localeCompare(right));

  return sha256(JSON.stringify(sortedEntries));
}

async function sha256(payload: string): Promise<string> {
  if (globalThis.crypto?.subtle) {
    const bytes = new TextEncoder().encode(payload);
    const digest = await globalThis.crypto.subtle.digest("SHA-256", bytes);
    return Array.from(new Uint8Array(digest), (byte) => byte.toString(16).padStart(2, "0")).join("");
  }

  const { createHash } = await import("node:crypto");
  return createHash("sha256").update(payload).digest("hex");
}

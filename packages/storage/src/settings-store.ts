import {
  DEFAULT_SETTINGS,
  type AppSettings,
  type DeepSeekModel,
  type TriggerMode,
  type TranslateMode
} from "@paper-float-translator/core";
import { readJsonFile, writeJsonFile } from "./file-utils";

const MODE_VALUES: TranslateMode[] = ["academic_zh", "bilingual", "literal", "natural", "terminology"];
const MODEL_VALUES: DeepSeekModel[] = ["deepseek-v4-flash", "deepseek-v4-pro"];
const TRIGGER_MODE_VALUES: TriggerMode[] = ["clipboard_shortcut", "auto_copy_shortcut", "mac_double_copy"];

export class JsonSettingsStore {
  constructor(private readonly filePath: string) {}

  async load(): Promise<AppSettings> {
    const raw = await readJsonFile<Partial<AppSettings>>(this.filePath, {});
    return normalizeSettings(raw);
  }

  async save(settings: AppSettings): Promise<AppSettings> {
    const normalized = normalizeSettings(settings);
    await writeJsonFile(this.filePath, normalized);
    return normalized;
  }

  async update(patch: Partial<AppSettings>): Promise<AppSettings> {
    const current = await this.load();
    return this.save({ ...current, ...patch });
  }
}

export function normalizeSettings(raw: Partial<AppSettings>): AppSettings {
  return {
    shortcut: typeof raw.shortcut === "string" && raw.shortcut.trim() ? raw.shortcut.trim() : DEFAULT_SETTINGS.shortcut,
    model: isDeepSeekModel(raw.model) ? raw.model : DEFAULT_SETTINGS.model,
    mode: isTranslateMode(raw.mode) ? raw.mode : DEFAULT_SETTINGS.mode,
    triggerMode: isTriggerMode(raw.triggerMode) ? raw.triggerMode : DEFAULT_SETTINGS.triggerMode,
    doubleCopyWindowMs: normalizeDoubleCopyWindowMs(raw.doubleCopyWindowMs),
    cleanPdfText: typeof raw.cleanPdfText === "boolean" ? raw.cleanPdfText : DEFAULT_SETTINGS.cleanPdfText,
    enableCache: typeof raw.enableCache === "boolean" ? raw.enableCache : DEFAULT_SETTINGS.enableCache,
    popupWidth: normalizePopupWidth(raw.popupWidth)
  };
}

function normalizeDoubleCopyWindowMs(value: unknown): number {
  const numericValue = typeof value === "string" && value.trim() ? Number(value) : value;

  if (typeof numericValue !== "number" || Number.isNaN(numericValue)) {
    return DEFAULT_SETTINGS.doubleCopyWindowMs;
  }

  return Math.min(3000, Math.max(400, Math.round(numericValue)));
}

function normalizePopupWidth(value: unknown): number {
  const numericValue = typeof value === "string" && value.trim() ? Number(value) : value;

  if (typeof numericValue !== "number" || Number.isNaN(numericValue)) {
    return DEFAULT_SETTINGS.popupWidth;
  }

  return Math.min(640, Math.max(320, Math.round(numericValue)));
}

function isDeepSeekModel(value: unknown): value is DeepSeekModel {
  return MODEL_VALUES.includes(value as DeepSeekModel);
}

function isTranslateMode(value: unknown): value is TranslateMode {
  return MODE_VALUES.includes(value as TranslateMode);
}

function isTriggerMode(value: unknown): value is TriggerMode {
  return TRIGGER_MODE_VALUES.includes(value as TriggerMode);
}

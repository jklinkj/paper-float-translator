import {
  DEFAULT_SETTINGS,
  type AppSettings,
  type DeepSeekModel,
  type TranslateMode
} from "@paper-float-translator/core";
import { readJsonFile, writeJsonFile } from "./file-utils";

const MODE_VALUES: TranslateMode[] = ["academic_zh", "bilingual", "literal", "natural", "terminology"];
const MODEL_VALUES: DeepSeekModel[] = ["deepseek-v4-flash", "deepseek-v4-pro"];

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
    cleanPdfText: typeof raw.cleanPdfText === "boolean" ? raw.cleanPdfText : DEFAULT_SETTINGS.cleanPdfText,
    enableCache: typeof raw.enableCache === "boolean" ? raw.enableCache : DEFAULT_SETTINGS.enableCache,
    popupWidth: normalizePopupWidth(raw.popupWidth)
  };
}

function normalizePopupWidth(value: unknown): number {
  if (typeof value !== "number" || Number.isNaN(value)) {
    return DEFAULT_SETTINGS.popupWidth;
  }

  return Math.min(640, Math.max(320, Math.round(value)));
}

function isDeepSeekModel(value: unknown): value is DeepSeekModel {
  return MODEL_VALUES.includes(value as DeepSeekModel);
}

function isTranslateMode(value: unknown): value is TranslateMode {
  return MODE_VALUES.includes(value as TranslateMode);
}

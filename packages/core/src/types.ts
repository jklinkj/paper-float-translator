export const DEFAULT_DEEPSEEK_MODEL = "deepseek-v4-flash" as const;
export const HIGH_QUALITY_DEEPSEEK_MODEL = "deepseek-v4-pro" as const;
export const DEFAULT_TARGET_LANGUAGE = "中文" as const;

export type DeepSeekModel =
  | typeof DEFAULT_DEEPSEEK_MODEL
  | typeof HIGH_QUALITY_DEEPSEEK_MODEL;

export type TranslateMode =
  | "academic_zh"
  | "bilingual"
  | "literal"
  | "natural"
  | "terminology";

export type Glossary = Record<string, string>;

export interface TranslateRequest {
  text: string;
  model: DeepSeekModel;
  mode: TranslateMode;
  targetLanguage: string;
  glossary?: Glossary;
}

export interface TranslateResult {
  cleanedText: string;
  translation: string;
  cached: boolean;
}

export interface AppSettings {
  model: DeepSeekModel;
  mode: TranslateMode;
  cleanPdfText: boolean;
  enableCache: boolean;
  enableSelectionPopup: boolean;
  targetLanguage: string;
  popupWidth: number;
}

export const DEFAULT_SETTINGS: AppSettings = {
  model: DEFAULT_DEEPSEEK_MODEL,
  mode: "academic_zh",
  cleanPdfText: true,
  enableCache: true,
  enableSelectionPopup: true,
  targetLanguage: DEFAULT_TARGET_LANGUAGE,
  popupWidth: 420
};

export type PopupStatus = "idle" | "selection" | "loading" | "success" | "error";

export interface PopupState {
  status: PopupStatus;
  sourceText?: string;
  selectedText?: string;
  cleanedText?: string;
  translation?: string;
  error?: string;
  cached?: boolean;
  targetLanguage?: string;
  pinned: boolean;
}

export interface WatcherStatus {
  available: boolean;
  running: boolean;
  message: string;
}

export interface CacheEntry {
  key: string;
  cleanedText: string;
  translation: string;
  model: DeepSeekModel;
  mode: TranslateMode;
  targetLanguage: string;
  glossaryVersion: string;
  createdAt: string;
}

export function normalizeTargetLanguage(value: unknown, fallback: unknown = DEFAULT_TARGET_LANGUAGE): string {
  const normalizedFallback = String(fallback || DEFAULT_TARGET_LANGUAGE)
    .replace(/[\r\n\t]+/g, " ")
    .replace(/\s+/g, " ")
    .trim();
  const normalizedValue = typeof value === "string"
    ? value
        .replace(/[\r\n\t]+/g, " ")
        .replace(/\s+/g, " ")
        .trim()
    : "";

  return normalizedValue || normalizedFallback || DEFAULT_TARGET_LANGUAGE;
}

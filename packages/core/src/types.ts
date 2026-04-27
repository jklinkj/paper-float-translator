export const DEFAULT_DEEPSEEK_MODEL = "deepseek-v4-flash" as const;
export const HIGH_QUALITY_DEEPSEEK_MODEL = "deepseek-v4-pro" as const;

export type DeepSeekModel =
  | typeof DEFAULT_DEEPSEEK_MODEL
  | typeof HIGH_QUALITY_DEEPSEEK_MODEL;

export type TranslateMode =
  | "academic_zh"
  | "bilingual"
  | "literal"
  | "natural"
  | "terminology";

export type TriggerMode =
  | "clipboard_shortcut"
  | "auto_copy_shortcut"
  | "mac_double_copy";

export type Glossary = Record<string, string>;

export interface TranslateRequest {
  text: string;
  model: DeepSeekModel;
  mode: TranslateMode;
  glossary?: Glossary;
}

export interface TranslateResult {
  cleanedText: string;
  translation: string;
  cached: boolean;
}

export interface SelectionProvider {
  readSelectedText(): Promise<string>;
}

export interface AppSettings {
  shortcut: string;
  model: DeepSeekModel;
  mode: TranslateMode;
  triggerMode: TriggerMode;
  doubleCopyWindowMs: number;
  cleanPdfText: boolean;
  enableCache: boolean;
  popupWidth: number;
}

export const DEFAULT_SETTINGS: AppSettings = {
  shortcut: "CommandOrControl+Shift+Y",
  model: DEFAULT_DEEPSEEK_MODEL,
  mode: "academic_zh",
  triggerMode: "clipboard_shortcut",
  doubleCopyWindowMs: 1200,
  cleanPdfText: true,
  enableCache: true,
  popupWidth: 420
};

export type PopupStatus = "idle" | "loading" | "success" | "error";

export interface PopupState {
  status: PopupStatus;
  sourceText?: string;
  cleanedText?: string;
  translation?: string;
  error?: string;
  cached?: boolean;
  pinned: boolean;
}

export interface CacheEntry {
  key: string;
  cleanedText: string;
  translation: string;
  model: DeepSeekModel;
  mode: TranslateMode;
  glossaryVersion: string;
  createdAt: string;
}

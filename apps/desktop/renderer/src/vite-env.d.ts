/// <reference types="vite/client" />

import type { AppSettings, PopupState, WatcherStatus } from "@paper-float-translator/core";

interface SettingsPayload {
  settings: AppSettings;
  hasApiKey: boolean;
  doubleCopyStatus: WatcherStatus;
  selectionStatus: WatcherStatus;
}

interface PaperFloatTranslatorApi {
  getSettings(): Promise<SettingsPayload>;
  saveSettings(settings: AppSettings): Promise<AppSettings>;
  saveApiKey(apiKey: string): Promise<{ hasApiKey: boolean }>;
  clearCache(): Promise<{ ok: true }>;
  openSettings(): Promise<void>;
  openAccessibilitySettings(): Promise<void>;
  copyTranslation(): Promise<void>;
  copySource(): Promise<void>;
  closePopup(): Promise<void>;
  togglePin(): Promise<void>;
  translateSelection(): Promise<void>;
  retryTranslation(targetLanguage?: string): Promise<void>;
  explainTerms(targetLanguage?: string): Promise<void>;
  resizePopup(height: number): Promise<void>;
  openExternal(url: string): Promise<void>;
  onPopupState(callback: (state: PopupState) => void): () => void;
}

declare global {
  interface Window {
    paperFloatTranslator: PaperFloatTranslatorApi;
  }
}

export {};

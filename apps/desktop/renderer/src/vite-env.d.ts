/// <reference types="vite/client" />

import type { AppSettings, PopupState } from "@paper-float-translator/core";

interface SettingsPayload {
  settings: AppSettings;
  hasApiKey: boolean;
  doubleCopyStatus: {
    available: boolean;
    running: boolean;
    message: string;
  };
}

interface PaperFloatTranslatorApi {
  getSettings(): Promise<SettingsPayload>;
  saveSettings(settings: AppSettings): Promise<AppSettings>;
  saveApiKey(apiKey: string): Promise<{ hasApiKey: boolean }>;
  clearCache(): Promise<{ ok: true }>;
  openSettings(): Promise<void>;
  copyTranslation(): Promise<void>;
  closePopup(): Promise<void>;
  togglePin(): Promise<void>;
  retryTranslation(): Promise<void>;
  explainTerms(): Promise<void>;
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

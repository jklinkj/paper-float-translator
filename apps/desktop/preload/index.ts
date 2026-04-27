import { contextBridge, ipcRenderer } from "electron";
import type { AppSettings, PopupState, WatcherStatus } from "@paper-float-translator/core";

export interface SettingsPayload {
  settings: AppSettings;
  hasApiKey: boolean;
  doubleCopyStatus: WatcherStatus;
  selectionStatus: WatcherStatus;
}

export const api = {
  getSettings: (): Promise<SettingsPayload> => ipcRenderer.invoke("settings:get"),
  saveSettings: (settings: AppSettings): Promise<AppSettings> => ipcRenderer.invoke("settings:save", settings),
  saveApiKey: (apiKey: string): Promise<{ hasApiKey: boolean }> =>
    ipcRenderer.invoke("settings:saveApiKey", apiKey),
  clearCache: (): Promise<{ ok: true }> => ipcRenderer.invoke("settings:clearCache"),
  openSettings: (): Promise<void> => ipcRenderer.invoke("app:openSettings"),
  openAccessibilitySettings: (): Promise<void> => ipcRenderer.invoke("app:openAccessibilitySettings"),
  copyTranslation: (): Promise<void> => ipcRenderer.invoke("popup:copyTranslation"),
  copySource: (): Promise<void> => ipcRenderer.invoke("popup:copySource"),
  closePopup: (): Promise<void> => ipcRenderer.invoke("popup:close"),
  togglePin: (): Promise<void> => ipcRenderer.invoke("popup:togglePin"),
  translateSelection: (): Promise<void> => ipcRenderer.invoke("popup:translateSelection"),
  retryTranslation: (targetLanguage?: string): Promise<void> => ipcRenderer.invoke("popup:retry", targetLanguage),
  explainTerms: (targetLanguage?: string): Promise<void> => ipcRenderer.invoke("popup:explainTerms", targetLanguage),
  resizePopup: (height: number): Promise<void> => ipcRenderer.invoke("popup:resize", height),
  openExternal: (url: string): Promise<void> => ipcRenderer.invoke("shell:openExternal", url),
  onPopupState: (callback: (state: PopupState) => void): (() => void) => {
    const listener = (_event: Electron.IpcRendererEvent, state: PopupState): void => callback(state);
    ipcRenderer.on("popup:state", listener);

    return () => {
      ipcRenderer.removeListener("popup:state", listener);
    };
  }
};

contextBridge.exposeInMainWorld("paperFloatTranslator", api);

export type PaperFloatTranslatorApi = typeof api;

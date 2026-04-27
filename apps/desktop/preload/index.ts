import { contextBridge, ipcRenderer } from "electron";
import type { AppSettings, PopupState } from "@paper-float-translator/core";

export interface SettingsPayload {
  settings: AppSettings;
  hasApiKey: boolean;
}

export const api = {
  getSettings: (): Promise<SettingsPayload> => ipcRenderer.invoke("settings:get"),
  saveSettings: (settings: AppSettings): Promise<AppSettings> => ipcRenderer.invoke("settings:save", settings),
  saveApiKey: (apiKey: string): Promise<{ hasApiKey: boolean }> =>
    ipcRenderer.invoke("settings:saveApiKey", apiKey),
  clearCache: (): Promise<{ ok: true }> => ipcRenderer.invoke("settings:clearCache"),
  openSettings: (): Promise<void> => ipcRenderer.invoke("app:openSettings"),
  copyTranslation: (): Promise<void> => ipcRenderer.invoke("popup:copyTranslation"),
  closePopup: (): Promise<void> => ipcRenderer.invoke("popup:close"),
  togglePin: (): Promise<void> => ipcRenderer.invoke("popup:togglePin"),
  retryTranslation: (): Promise<void> => ipcRenderer.invoke("popup:retry"),
  explainTerms: (): Promise<void> => ipcRenderer.invoke("popup:explainTerms"),
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

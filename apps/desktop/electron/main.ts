import { execFile } from "node:child_process";
import { mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { dirname } from "node:path";
import { join } from "node:path";
import { promisify } from "node:util";
import {
  app,
  BrowserWindow,
  clipboard,
  globalShortcut,
  ipcMain,
  Menu,
  safeStorage,
  screen,
  shell,
  type BrowserWindowConstructorOptions,
  type NativeImage,
  type Point
} from "electron";
import {
  cleanSelectedText,
  createCacheKey,
  DEFAULT_SETTINGS,
  type AppSettings,
  type PopupState,
  type TranslateMode
} from "@paper-float-translator/core";
import { DeepSeekClient } from "@paper-float-translator/deepseek";
import {
  JsonCacheStore,
  JsonGlossaryStore,
  JsonSettingsStore,
  KeytarSecretStore,
  type SecretStore
} from "@paper-float-translator/storage";

const execFileAsync = promisify(execFile);
const POPUP_DEFAULT_HEIGHT = 260;
const POPUP_OFFSET = 18;
const KEYCHAIN_SERVICE = "Paper Float Translator";
const DEEPSEEK_ACCOUNT = "deepseek-api-key";

let settingsStore: JsonSettingsStore;
let cacheStore: JsonCacheStore;
let glossaryStore: JsonGlossaryStore;
let secretStore: SecretStore;
let settings: AppSettings = DEFAULT_SETTINGS;

let settingsWindow: BrowserWindow | null = null;
let popupWindow: BrowserWindow | null = null;
let currentPopupState: PopupState = { status: "idle", pinned: false };
let lastCursorPoint: Point | null = null;
let lastCleanedText = "";

interface ClipboardSnapshot {
  text: string;
  html: string;
  rtf: string;
  image?: NativeImage;
}

app.whenReady().then(async () => {
  createStores();
  settings = await settingsStore.load();
  setupApplicationMenu();
  setupIpcHandlers();
  registerTranslationShortcut();
  await createSettingsWindow();
});

app.on("activate", () => {
  void createSettingsWindow();
});

app.on("will-quit", () => {
  globalShortcut.unregisterAll();
});

function createStores(): void {
  const userDataPath = app.getPath("userData");
  settingsStore = new JsonSettingsStore(join(userDataPath, "settings.json"));
  cacheStore = new JsonCacheStore(join(userDataPath, "cache.json"));
  glossaryStore = new JsonGlossaryStore(join(userDataPath, "glossary.json"));
  secretStore = new ResilientSecretStore(
    new KeytarSecretStore(KEYCHAIN_SERVICE, DEEPSEEK_ACCOUNT),
    new SafeStorageSecretStore(join(userDataPath, "secrets", "deepseek-api-key.enc"))
  );
}

function setupApplicationMenu(): void {
  Menu.setApplicationMenu(
    Menu.buildFromTemplate([
      {
        label: "Paper Float Translator",
        submenu: [
          {
            label: "Settings",
            accelerator: "CommandOrControl+,",
            click: () => void createSettingsWindow()
          },
          { type: "separator" },
          { role: "quit" }
        ]
      },
      {
        label: "Edit",
        submenu: [
          { role: "copy" },
          { role: "paste" },
          { role: "selectAll" }
        ]
      }
    ])
  );
}

function setupIpcHandlers(): void {
  ipcMain.handle("settings:get", async () => {
    settings = await settingsStore.load();
    return {
      settings,
      hasApiKey: Boolean(await getApiKeySafely())
    };
  });

  ipcMain.handle("settings:save", async (_event, nextSettings: AppSettings) => {
    settings = await settingsStore.save(nextSettings);
    registerTranslationShortcut();
    return settings;
  });

  ipcMain.handle("settings:saveApiKey", async (_event, apiKey: string) => {
    const trimmed = apiKey.trim();

    if (!trimmed) {
      await secretStore.delete();
      return { hasApiKey: false };
    }

    await secretStore.set(trimmed);
    return { hasApiKey: true };
  });

  ipcMain.handle("settings:clearCache", async () => {
    await cacheStore.clear();
    return { ok: true };
  });

  ipcMain.handle("app:openSettings", async () => {
    await createSettingsWindow();
  });

  ipcMain.handle("popup:copyTranslation", () => {
    if (currentPopupState.translation) {
      clipboard.writeText(currentPopupState.translation);
    }
  });

  ipcMain.handle("popup:close", () => {
    popupWindow?.hide();
  });

  ipcMain.handle("popup:togglePin", () => {
    updatePopupState({ ...currentPopupState, pinned: !currentPopupState.pinned });
  });

  ipcMain.handle("popup:retry", async () => {
    if (lastCleanedText) {
      await translateText(lastCleanedText, { bypassCache: true, mode: settings.mode });
    }
  });

  ipcMain.handle("popup:explainTerms", async () => {
    if (lastCleanedText) {
      await translateText(lastCleanedText, { bypassCache: true, mode: "terminology" });
    }
  });

  ipcMain.handle("popup:resize", (_event, requestedHeight: number) => {
    if (!popupWindow) {
      return;
    }

    const height = Math.min(520, Math.max(180, Math.round(requestedHeight)));
    popupWindow.setSize(settings.popupWidth, height, false);

    if (lastCursorPoint) {
      positionPopup(lastCursorPoint, settings.popupWidth, height);
    }
  });

  ipcMain.handle("shell:openExternal", (_event, url: string) => {
    if (url.startsWith("https://")) {
      void shell.openExternal(url);
    }
  });
}

function registerTranslationShortcut(): void {
  globalShortcut.unregisterAll();

  const registered = globalShortcut.register(settings.shortcut, () => {
    void handleTranslateShortcut();
  });

  if (!registered) {
    console.warn(`Failed to register global shortcut: ${settings.shortcut}`);
  }
}

async function handleTranslateShortcut(): Promise<void> {
  lastCursorPoint = screen.getCursorScreenPoint();
  await showPopup({
    status: "loading",
    pinned: currentPopupState.pinned,
    sourceText: "",
    cleanedText: "",
    translation: "",
    error: "",
    cached: false
  });

  try {
    const selectedText = await readSelectedTextFromClipboardCopy();
    const cleanedText = cleanSelectedText(selectedText, { enabled: settings.cleanPdfText });

    if (!cleanedText) {
      updatePopupState({
        status: "error",
        pinned: currentPopupState.pinned,
        error: "没有读取到选中文本。请先选中英文句子或段落，再按快捷键。",
        sourceText: selectedText,
        cleanedText: ""
      });
      return;
    }

    lastCleanedText = cleanedText;
    await translateText(cleanedText, { bypassCache: false, mode: settings.mode });
  } catch (error) {
    updatePopupState({
      status: "error",
      pinned: currentPopupState.pinned,
      error: toUserMessage(error),
      cleanedText: lastCleanedText
    });
  }
}

async function translateText(
  cleanedText: string,
  options: { bypassCache: boolean; mode: TranslateMode }
): Promise<void> {
  lastCursorPoint = lastCursorPoint ?? screen.getCursorScreenPoint();
  await showPopup({
    status: "loading",
    pinned: currentPopupState.pinned,
    cleanedText,
    sourceText: cleanedText,
    translation: "",
    cached: false
  });

  const apiKey = await getApiKeySafely();

  if (!apiKey) {
    updatePopupState({
      status: "error",
      pinned: currentPopupState.pinned,
      cleanedText,
      error: "请先在设置页保存 DeepSeek API Key。"
    });
    await createSettingsWindow();
    return;
  }

  const glossary = await glossaryStore.load();
  const glossaryVersion = await glossaryStore.version();
  const cacheKey = await createCacheKey({
    model: settings.model,
    mode: options.mode,
    glossaryVersion,
    cleanedText
  });

  if (settings.enableCache && !options.bypassCache) {
    const cached = await cacheStore.get(cacheKey);

    if (cached) {
      updatePopupState({
        status: "success",
        pinned: currentPopupState.pinned,
        cleanedText,
        translation: cached.translation,
        cached: true
      });
      return;
    }
  }

  const client = new DeepSeekClient({ apiKey });
  const result = await client.translate({
    text: cleanedText,
    model: settings.model,
    mode: options.mode,
    glossary
  });

  if (settings.enableCache && result.translation) {
    await cacheStore.set({
      key: cacheKey,
      cleanedText,
      translation: result.translation,
      model: settings.model,
      mode: options.mode,
      glossaryVersion,
      createdAt: new Date().toISOString()
    });
  }

  updatePopupState({
    status: "success",
    pinned: currentPopupState.pinned,
    cleanedText,
    translation: result.translation,
    cached: false
  });
}

async function readSelectedTextFromClipboardCopy(): Promise<string> {
  const snapshot = captureClipboard();

  try {
    clipboard.clear();
    await simulateCopyShortcut();
    await delay(140);
    return clipboard.readText();
  } finally {
    restoreClipboard(snapshot);
  }
}

function captureClipboard(): ClipboardSnapshot {
  const image = clipboard.readImage();

  return {
    text: clipboard.readText(),
    html: clipboard.readHTML(),
    rtf: clipboard.readRTF(),
    image: image.isEmpty() ? undefined : image
  };
}

function restoreClipboard(snapshot: ClipboardSnapshot): void {
  const data: Parameters<typeof clipboard.write>[0] = {};

  if (snapshot.text) {
    data.text = snapshot.text;
  }

  if (snapshot.html) {
    data.html = snapshot.html;
  }

  if (snapshot.rtf) {
    data.rtf = snapshot.rtf;
  }

  if (snapshot.image) {
    data.image = snapshot.image;
  }

  if (Object.keys(data).length === 0) {
    clipboard.clear();
    return;
  }

  clipboard.write(data);
}

async function simulateCopyShortcut(): Promise<void> {
  if (process.platform === "darwin") {
    await execFileAsync("osascript", [
      "-e",
      'tell application "System Events" to keystroke "c" using command down'
    ]);
    return;
  }

  if (process.platform === "win32") {
    await execFileAsync("powershell.exe", [
      "-NoProfile",
      "-Command",
      "$wshell = New-Object -ComObject WScript.Shell; $wshell.SendKeys('^c')"
    ]);
    return;
  }

  await execFileAsync("xdotool", ["key", "ctrl+c"]);
}

async function getApiKeySafely(): Promise<string | null> {
  try {
    return await secretStore.get();
  } catch {
    return null;
  }
}

async function createSettingsWindow(): Promise<void> {
  if (settingsWindow && !settingsWindow.isDestroyed()) {
    settingsWindow.show();
    settingsWindow.focus();
    return;
  }

  settingsWindow = new BrowserWindow({
    width: 780,
    height: 650,
    minWidth: 680,
    minHeight: 560,
    title: "Paper Float Translator",
    webPreferences: secureWebPreferences()
  });

  settingsWindow.on("closed", () => {
    settingsWindow = null;
  });

  await loadRenderer(settingsWindow, "settings");
}

async function ensurePopupWindow(): Promise<BrowserWindow> {
  if (popupWindow && !popupWindow.isDestroyed()) {
    return popupWindow;
  }

  popupWindow = new BrowserWindow({
    width: settings.popupWidth,
    height: POPUP_DEFAULT_HEIGHT,
    frame: false,
    resizable: false,
    maximizable: false,
    minimizable: false,
    fullscreenable: false,
    alwaysOnTop: true,
    skipTaskbar: true,
    show: false,
    title: "Translation",
    webPreferences: secureWebPreferences()
  });

  popupWindow.setAlwaysOnTop(true, "floating");

  popupWindow.on("blur", () => {
    if (!currentPopupState.pinned) {
      popupWindow?.hide();
    }
  });

  popupWindow.on("closed", () => {
    popupWindow = null;
  });

  popupWindow.webContents.on("did-finish-load", () => {
    sendPopupState();
  });

  await loadRenderer(popupWindow, "popup");
  return popupWindow;
}

async function showPopup(state: PopupState): Promise<void> {
  updatePopupState(state);
  const popup = await ensurePopupWindow();
  const [width, height] = popup.getSize();
  positionPopup(lastCursorPoint ?? screen.getCursorScreenPoint(), width, height);
  popup.showInactive();
  sendPopupState();
}

function updatePopupState(state: PopupState): void {
  currentPopupState = state;
  sendPopupState();
}

function sendPopupState(): void {
  if (!popupWindow || popupWindow.isDestroyed() || popupWindow.webContents.isLoading()) {
    return;
  }

  popupWindow.webContents.send("popup:state", currentPopupState);
}

function positionPopup(cursorPoint: Point, width: number, height: number): void {
  if (!popupWindow || popupWindow.isDestroyed()) {
    return;
  }

  const workArea = screen.getDisplayNearestPoint(cursorPoint).workArea;
  const rightEdge = workArea.x + workArea.width;
  const bottomEdge = workArea.y + workArea.height;

  let x = cursorPoint.x + POPUP_OFFSET;
  let y = cursorPoint.y + POPUP_OFFSET;

  if (x + width > rightEdge) {
    x = cursorPoint.x - width - POPUP_OFFSET;
  }

  if (y + height > bottomEdge) {
    y = cursorPoint.y - height - POPUP_OFFSET;
  }

  x = Math.min(Math.max(x, workArea.x), rightEdge - width);
  y = Math.min(Math.max(y, workArea.y), bottomEdge - height);

  popupWindow.setPosition(Math.round(x), Math.round(y), false);
}

async function loadRenderer(window: BrowserWindow, view: "settings" | "popup"): Promise<void> {
  const devServerUrl = process.env.ELECTRON_RENDERER_URL;

  if (devServerUrl) {
    await window.loadURL(`${devServerUrl}?view=${view}`);
    return;
  }

  await window.loadFile(join(__dirname, "../renderer/index.html"), {
    query: { view }
  });
}

function secureWebPreferences(): BrowserWindowConstructorOptions["webPreferences"] {
  return {
    preload: join(__dirname, "../preload/index.mjs"),
    contextIsolation: true,
    nodeIntegration: false,
    sandbox: false
  };
}

function delay(milliseconds: number): Promise<void> {
  return new Promise((resolve) => {
    setTimeout(resolve, milliseconds);
  });
}

function toUserMessage(error: unknown): string {
  if (error instanceof Error) {
    if (error.message.includes("osascript")) {
      return "无法模拟复制。请在系统设置中允许 Paper Float Translator 使用辅助功能权限。";
    }

    if (error.message.includes("xdotool")) {
      return "无法模拟复制。Linux 环境需要安装 xdotool，或后续接入平台专用复制实现。";
    }

    return error.message;
  }

  return "发生未知错误。";
}

class ResilientSecretStore implements SecretStore {
  constructor(
    private readonly primary: SecretStore,
    private readonly fallback: SecretStore
  ) {}

  async get(): Promise<string | null> {
    try {
      const value = await this.primary.get();

      if (value) {
        return value;
      }
    } catch {
      // Fall through to the local OS-encrypted fallback for development builds.
    }

    return this.fallback.get();
  }

  async set(value: string): Promise<void> {
    try {
      await this.primary.set(value);
      await this.fallback.delete();
      return;
    } catch {
      await this.fallback.set(value);
    }
  }

  async delete(): Promise<void> {
    await Promise.allSettled([this.primary.delete(), this.fallback.delete()]);
  }
}

class SafeStorageSecretStore implements SecretStore {
  constructor(private readonly filePath: string) {}

  async get(): Promise<string | null> {
    try {
      const encrypted = await readFile(this.filePath, "utf8");
      return safeStorage.decryptString(Buffer.from(encrypted, "base64"));
    } catch {
      return null;
    }
  }

  async set(value: string): Promise<void> {
    if (!safeStorage.isEncryptionAvailable()) {
      throw new Error("当前系统不可用安全加密存储，无法保存 API Key。");
    }

    await mkdir(dirname(this.filePath), { recursive: true });
    const encrypted = safeStorage.encryptString(value).toString("base64");
    await writeFile(this.filePath, encrypted, "utf8");
  }

  async delete(): Promise<void> {
    await rm(this.filePath, { force: true });
  }
}

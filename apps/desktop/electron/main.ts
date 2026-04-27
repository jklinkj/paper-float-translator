import { spawn, type ChildProcessWithoutNullStreams } from "node:child_process";
import { mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { dirname } from "node:path";
import { join } from "node:path";
import {
  app,
  BrowserWindow,
  clipboard,
  ipcMain,
  Menu,
  safeStorage,
  screen,
  shell,
  type BrowserWindowConstructorOptions,
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

const POPUP_DEFAULT_HEIGHT = 260;
const POPUP_OFFSET = 18;
const DOUBLE_COPY_POLL_INTERVAL_SECONDS = 0.05;
const DOUBLE_COPY_WINDOW_MS = 900;
const DOUBLE_COPY_COOLDOWN_MS = 1000;
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
let macDoubleCopyWatcher: ChildProcessWithoutNullStreams | null = null;
let macDoubleCopyStatus = buildInitialDoubleCopyStatus();
let macDoubleCopyBuffer = "";
let lastMacCopy: { text: string; copiedAt: number } | null = null;
let lastDoubleCopyTrigger: { text: string; triggeredAt: number } | null = null;

interface DoubleCopyStatus {
  available: boolean;
  running: boolean;
  message: string;
}

const MAC_PASTEBOARD_WATCHER_SCRIPT = `
import AppKit
import Foundation

var lastChangeCount = NSPasteboard.general.changeCount

while true {
  let pasteboard = NSPasteboard.general
  let changeCount = pasteboard.changeCount

  if changeCount != lastChangeCount {
    lastChangeCount = changeCount
    let text = pasteboard.string(forType: .string) ?? ""
    let encoded = text.data(using: .utf8)?.base64EncodedString() ?? ""
    print("\\(changeCount)\\t\\(encoded)")
    fflush(stdout)
  }

  Thread.sleep(forTimeInterval: ${DOUBLE_COPY_POLL_INTERVAL_SECONDS})
}
`;

app.whenReady().then(async () => {
  createStores();
  settings = await settingsStore.load();
  setupApplicationMenu();
  setupIpcHandlers();
  startMacDoubleCopyWatcher();
  await createSettingsWindow();
  await prewarmPopupWindow();
});

app.on("activate", () => {
  void createSettingsWindow();
});

app.on("will-quit", () => {
  stopMacDoubleCopyWatcher();
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
      hasApiKey: Boolean(await getApiKeySafely()),
      doubleCopyStatus: macDoubleCopyStatus
    };
  });

  ipcMain.handle("settings:save", async (_event, nextSettings: AppSettings) => {
    settings = await settingsStore.save(nextSettings);
    resizeExistingPopupToSettings();
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

async function translateCopiedText(rawText: string): Promise<void> {
  const cleanedText = cleanSelectedText(rawText, { enabled: settings.cleanPdfText });

  if (!cleanedText) {
    await showPopup({
      status: "error",
      pinned: currentPopupState.pinned,
      error: buildEmptyClipboardMessage(),
      sourceText: rawText,
      cleanedText: ""
    });
    return;
  }

  lastCleanedText = cleanedText;
  try {
    await translateText(cleanedText, { bypassCache: false, mode: settings.mode });
  } catch (error) {
    updatePopupState({
      status: "error",
      pinned: currentPopupState.pinned,
      error: toUserMessage(error),
      cleanedText
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

function buildEmptyClipboardMessage(): string {
  return "剪贴板中没有可翻译文本。请确认复制的是文字内容，然后快速按两次 Cmd+C。";
}

function startMacDoubleCopyWatcher(): void {
  if (process.platform !== "darwin") {
    macDoubleCopyStatus = {
      available: false,
      running: false,
      message: "双复制触发目前仅支持 macOS。"
    };
    return;
  }

  if (macDoubleCopyWatcher) {
    macDoubleCopyStatus = {
      available: true,
      running: true,
      message: "双复制监听已启用。"
    };
    return;
  }

  try {
    macDoubleCopyBuffer = "";
    lastMacCopy = null;
    lastDoubleCopyTrigger = null;
    macDoubleCopyWatcher = spawn("/usr/bin/swift", ["-e", MAC_PASTEBOARD_WATCHER_SCRIPT], {
      stdio: "pipe"
    }) as ChildProcessWithoutNullStreams;
    macDoubleCopyStatus = {
      available: true,
      running: true,
      message: "双复制监听已启用。"
    };

    macDoubleCopyWatcher.stdout.setEncoding("utf8");
    macDoubleCopyWatcher.stdout.on("data", (chunk: string) => {
      macDoubleCopyBuffer += chunk;
      const lines = macDoubleCopyBuffer.split(/\r?\n/);
      macDoubleCopyBuffer = lines.pop() ?? "";

      for (const line of lines) {
        handleMacPasteboardChange(line);
      }
    });

    macDoubleCopyWatcher.stderr.setEncoding("utf8");
    macDoubleCopyWatcher.stderr.on("data", (chunk: string) => {
      devLog("macOS double-copy watcher stderr.", chunk.slice(0, 240));
    });

    macDoubleCopyWatcher.on("error", (error) => {
      macDoubleCopyStatus = {
        available: false,
        running: false,
        message: `双复制监听启动失败：${error.message}`
      };
      macDoubleCopyWatcher = null;
    });

    macDoubleCopyWatcher.on("close", (code) => {
      macDoubleCopyStatus = {
        available: false,
        running: false,
        message: code === 0 ? "双复制监听已停止。" : "双复制监听已退出，请重启应用。"
      };

      macDoubleCopyWatcher = null;
      macDoubleCopyBuffer = "";
      lastMacCopy = null;
      lastDoubleCopyTrigger = null;
    });
  } catch (error) {
    macDoubleCopyStatus = {
      available: false,
      running: false,
      message: error instanceof Error ? `双复制监听启动失败：${error.message}` : "双复制监听启动失败。"
    };
  }
}

function stopMacDoubleCopyWatcher(): void {
  if (!macDoubleCopyWatcher) {
    return;
  }

  const watcher = macDoubleCopyWatcher;
  macDoubleCopyWatcher = null;
  watcher.kill();
  macDoubleCopyBuffer = "";
  lastMacCopy = null;
  lastDoubleCopyTrigger = null;
}

function handleMacPasteboardChange(line: string): void {
  const [, encodedText] = line.split("\t");

  if (encodedText === undefined) {
    return;
  }

  const text = Buffer.from(encodedText, "base64").toString("utf8");
  const cleaned = cleanSelectedText(text, { enabled: settings.cleanPdfText });

  const copiedAt = Date.now();
  const previousCopy = lastMacCopy;
  lastMacCopy = { text: cleaned, copiedAt };

  if (!previousCopy) {
    return;
  }

  const isRepeatedCopy = previousCopy.text === cleaned;
  const isWithinWindow = copiedAt - previousCopy.copiedAt <= DOUBLE_COPY_WINDOW_MS;

  if (!isRepeatedCopy || !isWithinWindow) {
    return;
  }

  lastMacCopy = null;
  lastCursorPoint = screen.getCursorScreenPoint();

  if (!cleaned) {
    void showPopup({
      status: "error",
      pinned: currentPopupState.pinned,
      error: buildEmptyClipboardMessage(),
      sourceText: text,
      cleanedText: ""
    });
    return;
  }

  if (wasRecentlyTriggered(cleaned, copiedAt)) {
    return;
  }

  lastDoubleCopyTrigger = { text: cleaned, triggeredAt: copiedAt };
  void translateCopiedText(text);
}

function wasRecentlyTriggered(text: string, copiedAt: number): boolean {
  return Boolean(
    lastDoubleCopyTrigger &&
      lastDoubleCopyTrigger.text === text &&
      copiedAt - lastDoubleCopyTrigger.triggeredAt <= DOUBLE_COPY_COOLDOWN_MS
  );
}

function buildInitialDoubleCopyStatus(): DoubleCopyStatus {
  if (process.platform !== "darwin") {
    return {
      available: false,
      running: false,
      message: "双复制触发目前仅支持 macOS。"
    };
  }

  return {
    available: true,
    running: false,
    message: "双复制监听未启用。"
  };
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

  popupWindow.on("closed", () => {
    popupWindow = null;
  });

  popupWindow.webContents.on("did-finish-load", () => {
    sendPopupState();
  });

  await loadRenderer(popupWindow, "popup");
  return popupWindow;
}

async function prewarmPopupWindow(): Promise<void> {
  try {
    await ensurePopupWindow();
  } catch (error) {
    devLog("Popup prewarm failed.", toUserMessage(error));
  }
}

async function showPopup(state: PopupState): Promise<void> {
  updatePopupState(state);
  const popup = await ensurePopupWindow();
  resizeExistingPopupToSettings();
  const [width, height] = popup.getSize();
  positionPopup(lastCursorPoint ?? screen.getCursorScreenPoint(), width, height);
  popup.showInactive();
  popup.moveTop();
  devLog("Popup shown.", { visible: popup.isVisible(), bounds: popup.getBounds() });
  sendPopupState();
}

function resizeExistingPopupToSettings(): void {
  if (!popupWindow || popupWindow.isDestroyed()) {
    return;
  }

  const [, currentHeight] = popupWindow.getSize();
  popupWindow.setSize(settings.popupWidth, currentHeight, false);

  if (lastCursorPoint) {
    positionPopup(lastCursorPoint, settings.popupWidth, currentHeight);
  }
}

function updatePopupState(state: PopupState): void {
  currentPopupState = state;
  devLog("Popup state updated.", {
    status: state.status,
    cached: state.cached,
    error: state.error ? state.error.slice(0, 120) : undefined
  });
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

function toUserMessage(error: unknown): string {
  if (error instanceof Error) {
    return error.message;
  }

  return "发生未知错误。";
}

function devLog(message: string, metadata?: unknown): void {
  if (app.isPackaged) {
    return;
  }

  if (metadata === undefined) {
    console.info(message);
    return;
  }

  console.info(message, metadata);
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

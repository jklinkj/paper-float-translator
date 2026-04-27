import { execFile, spawn, type ChildProcessWithoutNullStreams } from "node:child_process";
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
  systemPreferences,
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
  type TriggerMode,
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
const COPY_TIMEOUT_MS = 1600;
const COPY_POLL_INTERVAL_MS = 50;
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

interface ClipboardSnapshot {
  text: string;
  html: string;
  rtf: string;
  image?: NativeImage;
}

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

  Thread.sleep(forTimeInterval: 0.25)
}
`;

app.whenReady().then(async () => {
  createStores();
  settings = await settingsStore.load();
  setupApplicationMenu();
  setupIpcHandlers();
  registerTranslationShortcut();
  configureMacDoubleCopyWatcher();
  await createSettingsWindow();
});

app.on("activate", () => {
  void createSettingsWindow();
});

app.on("will-quit", () => {
  globalShortcut.unregisterAll();
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
            label: "Translate Selection",
            click: () => void handleTranslateShortcut()
          },
          { type: "separator" },
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
    registerTranslationShortcut();
    resizeExistingPopupToSettings();
    configureMacDoubleCopyWatcher();
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
  devLog("Translate selection requested.", { triggerMode: settings.triggerMode });
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
    if (settings.triggerMode === "auto_copy_shortcut") {
      await translateAutoCopiedSelection();
      return;
    }

    await translateClipboardText("shortcut");
  } catch (error) {
    updatePopupState({
      status: "error",
      pinned: currentPopupState.pinned,
      error: toUserMessage(error),
      cleanedText: lastCleanedText
    });
  }
}

async function translateAutoCopiedSelection(): Promise<void> {
  if (!hasRequiredAccessibilityPermission()) {
    updatePopupState({
      status: "error",
      pinned: currentPopupState.pinned,
      error: buildAccessibilityPermissionMessage(),
      sourceText: "",
      cleanedText: ""
    });
    return;
  }

  const selectedText = await readSelectedTextFromClipboardCopy();
  await translateRawText(selectedText, "auto_copy_shortcut");
}

async function translateClipboardText(triggerSource: "shortcut" | "mac_double_copy"): Promise<void> {
  const clipboardText = readTextFromClipboard();
  await translateRawText(clipboardText, triggerSource);
}

async function translateRawText(rawText: string, triggerSource: TriggerMode | "shortcut"): Promise<void> {
  const cleanedText = cleanSelectedText(rawText, { enabled: settings.cleanPdfText });

  if (!cleanedText) {
    updatePopupState({
      status: "error",
      pinned: currentPopupState.pinned,
      error: buildEmptyClipboardMessage(triggerSource),
      sourceText: rawText,
      cleanedText: ""
    });
    return;
  }

  lastCleanedText = cleanedText;
  await translateText(cleanedText, { bypassCache: false, mode: settings.mode });
}

function readTextFromClipboard(): string {
  return clipboard.readText();
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
  const sentinel = createClipboardSentinel();

  try {
    clipboard.writeText(sentinel);
    await simulateCopyShortcut();
    return waitForCopiedText(sentinel);
  } finally {
    restoreClipboard(snapshot);
  }
}

async function waitForCopiedText(sentinel: string): Promise<string> {
  const startedAt = Date.now();

  while (Date.now() - startedAt < COPY_TIMEOUT_MS) {
    const copiedText = clipboard.readText();

    if (copiedText && copiedText !== sentinel) {
      return copiedText;
    }

    await delay(COPY_POLL_INTERVAL_MS);
  }

  const finalText = clipboard.readText();
  return finalText === sentinel ? "" : finalText;
}

function createClipboardSentinel(): string {
  return `__paper_float_translator_copy_${Date.now()}_${Math.random().toString(16).slice(2)}__`;
}

function buildEmptyClipboardMessage(triggerSource: TriggerMode | "shortcut"): string {
  if (triggerSource === "auto_copy_shortcut") {
    return "没有读取到选中文本。自动复制可能被 PDF 阅读器或 macOS 权限拦截，请改用“先复制，再翻译”模式。";
  }

  if (triggerSource === "mac_double_copy") {
    return "双复制触发成功，但剪贴板中没有可翻译文本。请确认复制的是文字内容。";
  }

  return "剪贴板为空或不是文本。请先选中文本并按 Cmd+C 复制，再按翻译快捷键。";
}

function configureMacDoubleCopyWatcher(): void {
  if (settings.triggerMode !== "mac_double_copy") {
    stopMacDoubleCopyWatcher();
    macDoubleCopyStatus = buildInitialDoubleCopyStatus();
    return;
  }

  startMacDoubleCopyWatcher();
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
      if (settings.triggerMode === "mac_double_copy") {
        macDoubleCopyStatus = {
          available: false,
          running: false,
          message: code === 0 ? "双复制监听已停止。" : "双复制监听已退出，请重启应用或切换取词方式。"
        };
      }

      macDoubleCopyWatcher = null;
      macDoubleCopyBuffer = "";
      lastMacCopy = null;
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
}

function handleMacPasteboardChange(line: string): void {
  const [, encodedText] = line.split("\t");

  if (encodedText === undefined) {
    return;
  }

  const text = Buffer.from(encodedText, "base64").toString("utf8");
  const cleaned = cleanSelectedText(text, { enabled: settings.cleanPdfText });

  if (!cleaned) {
    lastMacCopy = null;
    return;
  }

  const copiedAt = Date.now();
  const previousCopy = lastMacCopy;
  lastMacCopy = { text: cleaned, copiedAt };

  if (!previousCopy) {
    return;
  }

  const isRepeatedCopy = previousCopy.text === cleaned;
  const isWithinWindow = copiedAt - previousCopy.copiedAt <= settings.doubleCopyWindowMs;

  if (!isRepeatedCopy || !isWithinWindow) {
    return;
  }

  lastMacCopy = null;
  lastCursorPoint = screen.getCursorScreenPoint();
  void translateRawText(text, "mac_double_copy");
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

function hasRequiredAccessibilityPermission(): boolean {
  if (process.platform !== "darwin") {
    return true;
  }

  return systemPreferences.isTrustedAccessibilityClient(false);
}

function buildAccessibilityPermissionMessage(): string {
  return [
    "需要授予 macOS 辅助功能权限后，才能复制其他 App 中的选中文本。",
    "请打开 System Settings -> Privacy & Security -> Accessibility，允许 Electron、Terminal/Codex 或最终打包后的 Paper Float Translator，然后重启应用。"
  ].join(" ");
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

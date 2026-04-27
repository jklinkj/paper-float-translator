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
  normalizeTargetLanguage,
  type AppSettings,
  type PopupState,
  type TranslateMode,
  type WatcherStatus
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
const SELECTION_POPUP_WIDTH = 320;
const SELECTION_POPUP_HEIGHT = 56;
const POPUP_OFFSET = 18;
const DOUBLE_COPY_POLL_INTERVAL_SECONDS = 0.05;
const DOUBLE_COPY_WINDOW_MS = 900;
const DOUBLE_COPY_COOLDOWN_MS = 1000;
const SELECTION_READ_DELAY_SECONDS = 0.1;
const SELECTION_EMPTY_STATUS_COOLDOWN_SECONDS = 1.5;
const SELECTION_POPUP_COOLDOWN_MS = 800;
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
let macSelectionWatcher: ChildProcessWithoutNullStreams | null = null;
let macSelectionStatus = buildInitialSelectionStatus();
let macSelectionBuffer = "";
let lastMacCopy: { text: string; copiedAt: number } | null = null;
let lastDoubleCopyTrigger: { text: string; triggeredAt: number } | null = null;
let lastSelectionPopup: { text: string; shownAt: number } | null = null;
let lastSelectionEmptyAt = 0;
let suppressedSelectionText: string | null = null;
let activeSelectionText: string | null = null;

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

const MAC_SELECTION_WATCHER_SCRIPT = `
import AppKit
import ApplicationServices
import CoreGraphics
import Foundation

var didDrag = false
var mouseDownLocation: CGPoint?
var lastEmittedSelection = ""
var lastEmptyStatusAt = 0.0
let dragDistanceThreshold = 4.0

func encode(_ text: String) -> String {
  return text.data(using: .utf8)?.base64EncodedString() ?? ""
}

func emitStatus(_ status: String) {
  print("status\\t\\(status)")
  fflush(stdout)
}

func emitSelection(_ text: String, anchor: CGPoint) {
  print("selection\\t\\(encode(text))\\t\\(anchor.x)\\t\\(anchor.y)")
  fflush(stdout)
}

func emitMouseDown(_ location: CGPoint) {
  print("mouse_down\\t\\(location.x)\\t\\(location.y)")
  fflush(stdout)
}

func emitEmptyStatusIfNeeded() {
  let now = Date().timeIntervalSince1970

  if now - lastEmptyStatusAt >= ${SELECTION_EMPTY_STATUS_COOLDOWN_SECONDS} {
    emitStatus("selection_empty")
    lastEmptyStatusAt = now
  }
}

func trimmed(_ text: String) -> String {
  return text.trimmingCharacters(in: .whitespacesAndNewlines)
}

func copyAttribute(_ element: AXUIElement, _ attribute: CFString) -> AnyObject? {
  var value: AnyObject?
  let error = AXUIElementCopyAttributeValue(element, attribute, &value)
  return error == .success ? value : nil
}

func stringForRange(_ element: AXUIElement, _ rangeValue: AXValue) -> String? {
  var range = CFRange()

  guard AXValueGetType(rangeValue) == .cfRange,
        AXValueGetValue(rangeValue, .cfRange, &range),
        range.length > 0 else {
    return nil
  }

  var result: AnyObject?
  let error = AXUIElementCopyParameterizedAttributeValue(
    element,
    kAXStringForRangeParameterizedAttribute as CFString,
    rangeValue,
    &result
  )

  guard error == .success, let text = result as? String, !trimmed(text).isEmpty else {
    return nil
  }

  return text
}

func selectedText(from element: AXUIElement) -> String? {
  if let direct = copyAttribute(element, kAXSelectedTextAttribute as CFString) as? String,
     !trimmed(direct).isEmpty {
    return direct
  }

  if let rawRangeValue = copyAttribute(element, kAXSelectedTextRangeAttribute as CFString),
     CFGetTypeID(rawRangeValue) == AXValueGetTypeID() {
    let rangeValue = rawRangeValue as! AXValue

    if let text = stringForRange(element, rangeValue) {
      return text
    }
  }

  if let rawRangeValues = copyAttribute(element, kAXSelectedTextRangesAttribute as CFString) as? [AnyObject] {
    let parts = rawRangeValues.compactMap { rawRangeValue -> String? in
      guard CFGetTypeID(rawRangeValue) == AXValueGetTypeID() else {
        return nil
      }

      return stringForRange(element, rawRangeValue as! AXValue)
    }
    let combined = parts.joined(separator: "\\n")

    if !trimmed(combined).isEmpty {
      return combined
    }
  }

  return nil
}

func parent(of element: AXUIElement) -> AXUIElement? {
  guard let value = copyAttribute(element, kAXParentAttribute as CFString),
        CFGetTypeID(value) == AXUIElementGetTypeID() else {
    return nil
  }

  return (value as! AXUIElement)
}

func selectedTextFromElementOrParents(_ element: AXUIElement) -> String? {
  var current: AXUIElement? = element
  var depth = 0

  while let candidate = current, depth < 8 {
    if let text = selectedText(from: candidate) {
      return text
    }

    current = parent(of: candidate)
    depth += 1
  }

  return nil
}

func focusedElement() -> AXUIElement? {
  let systemWide = AXUIElementCreateSystemWide()
  var value: AnyObject?
  let error = AXUIElementCopyAttributeValue(
    systemWide,
    kAXFocusedUIElementAttribute as CFString,
    &value
  )

  guard error == .success,
        let element = value,
        CFGetTypeID(element) == AXUIElementGetTypeID() else {
    return nil
  }

  return (element as! AXUIElement)
}

func focusedWindow(from appElement: AXUIElement) -> AXUIElement? {
  guard let value = copyAttribute(appElement, kAXFocusedWindowAttribute as CFString),
        CFGetTypeID(value) == AXUIElementGetTypeID() else {
    return nil
  }

  return (value as! AXUIElement)
}

func frontmostAppElement() -> AXUIElement? {
  guard let app = NSWorkspace.shared.frontmostApplication else {
    return nil
  }

  return AXUIElementCreateApplication(app.processIdentifier)
}

func elementAtMouseLocation() -> AXUIElement? {
  let systemWide = AXUIElementCreateSystemWide()
  let location = NSEvent.mouseLocation
  let screenMaxY = NSScreen.screens.map { $0.frame.maxY }.max() ?? 0
  let points = [
    CGPoint(x: location.x, y: location.y),
    CGPoint(x: location.x, y: screenMaxY - location.y)
  ]

  for point in points {
    var value: AXUIElement?
    let error = AXUIElementCopyElementAtPosition(systemWide, Float(point.x), Float(point.y), &value)

    if error == .success, let element = value {
      return element
    }
  }

  return nil
}

func childElements(of element: AXUIElement) -> [AXUIElement] {
  guard let children = copyAttribute(element, kAXChildrenAttribute as CFString) as? [AnyObject] else {
    return []
  }

  return children.compactMap { child in
    guard CFGetTypeID(child) == AXUIElementGetTypeID() else {
      return nil
    }

    return (child as! AXUIElement)
  }
}

func selectedTextBySearchingChildren(from root: AXUIElement) -> String? {
  var queue: [(AXUIElement, Int)] = [(root, 0)]
  var visited = 0

  while !queue.isEmpty && visited < 160 {
    let (element, depth) = queue.removeFirst()
    visited += 1

    if let text = selectedText(from: element) {
      return text
    }

    if depth < 4 {
      for child in childElements(of: element) {
        queue.append((child, depth + 1))
      }
    }
  }

  return nil
}

func currentSelectedText() -> String? {
  var candidates: [AXUIElement] = []

  if let element = elementAtMouseLocation() {
    candidates.append(element)
  }

  if let element = focusedElement() {
    candidates.append(element)
  }

  if let appElement = frontmostAppElement() {
    candidates.append(appElement)

    if let window = focusedWindow(from: appElement) {
      candidates.append(window)
    }
  }

  for candidate in candidates {
    if let text = selectedTextFromElementOrParents(candidate) {
      return text
    }
  }

  for candidate in candidates {
    if let text = selectedTextBySearchingChildren(from: candidate) {
      return text
    }
  }

  return nil
}

func readCurrentSelection(anchor: CGPoint) {
  guard AXIsProcessTrusted() else {
    emitStatus("accessibility_denied")
    return
  }

  guard let text = currentSelectedText(),
        !trimmed(text).isEmpty else {
    lastEmittedSelection = ""
    emitEmptyStatusIfNeeded()
    return
  }

  let normalized = trimmed(text)

  if normalized != lastEmittedSelection {
    lastEmittedSelection = normalized
    emitSelection(text, anchor: anchor)
  }
}

let trustOptions = [kAXTrustedCheckOptionPrompt.takeUnretainedValue() as String: true] as CFDictionary

if !AXIsProcessTrustedWithOptions(trustOptions) {
  emitStatus("accessibility_denied")
} else {
  emitStatus("ready")
}

let eventMask =
  (1 << CGEventType.leftMouseDown.rawValue) |
  (1 << CGEventType.leftMouseDragged.rawValue) |
  (1 << CGEventType.leftMouseUp.rawValue)

let eventTap = CGEvent.tapCreate(
  tap: .cgSessionEventTap,
  place: .headInsertEventTap,
  options: .listenOnly,
  eventsOfInterest: CGEventMask(eventMask),
  callback: { _, type, event, _ in
    let location = event.location

    switch type {
    case .leftMouseDown:
      emitMouseDown(location)
      mouseDownLocation = location
      didDrag = false
    case .leftMouseDragged:
      didDrag = true
    case .leftMouseUp:
      let anchor = location
      let movedEnough: Bool

      if let start = mouseDownLocation {
        let dx = anchor.x - start.x
        let dy = anchor.y - start.y
        movedEnough = sqrt(dx * dx + dy * dy) >= dragDistanceThreshold
      } else {
        movedEnough = false
      }

      let shouldRead = didDrag || movedEnough
      didDrag = false
      mouseDownLocation = nil

      if shouldRead {
        DispatchQueue.main.asyncAfter(deadline: .now() + ${SELECTION_READ_DELAY_SECONDS}) {
          readCurrentSelection(anchor: anchor)
        }
      }
    default:
      break
    }

    return Unmanaged.passUnretained(event)
  },
  userInfo: nil
)

if let eventTap = eventTap {
  let runLoopSource = CFMachPortCreateRunLoopSource(kCFAllocatorDefault, eventTap, 0)
  CFRunLoopAddSource(CFRunLoopGetCurrent(), runLoopSource, .commonModes)
  CGEvent.tapEnable(tap: eventTap, enable: true)
} else {
  emitStatus("mouse_tap_unavailable")
}

RunLoop.current.run()
`;

app.whenReady().then(async () => {
  createStores();
  settings = await settingsStore.load();
  setupApplicationMenu();
  setupIpcHandlers();
  startMacDoubleCopyWatcher();
  startMacSelectionWatcher();
  await createSettingsWindow();
  await prewarmPopupWindow();
});

app.on("activate", () => {
  void createSettingsWindow();
});

app.on("will-quit", () => {
  stopMacDoubleCopyWatcher();
  stopMacSelectionWatcher();
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
    refreshMacSelectionWatcherStatus();
    return {
      settings,
      hasApiKey: Boolean(await getApiKeySafely()),
      doubleCopyStatus: macDoubleCopyStatus,
      selectionStatus: macSelectionStatus
    };
  });

  ipcMain.handle("settings:save", async (_event, nextSettings: AppSettings) => {
    settings = await settingsStore.save(nextSettings);

    if (!settings.enableSelectionPopup) {
      if (currentPopupState.status === "selection") {
        suppressCurrentSelection();
        popupWindow?.hide();
      }
      clearSuppressedSelection();
    }

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

  ipcMain.handle("app:openAccessibilitySettings", async () => {
    if (process.platform === "darwin") {
      await shell.openExternal("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility");
    }
  });

  ipcMain.handle("popup:copyTranslation", () => {
    if (currentPopupState.translation) {
      clipboard.writeText(currentPopupState.translation);
    }
  });

  ipcMain.handle("popup:copySource", () => {
    const text = currentPopupState.sourceText ?? currentPopupState.selectedText ?? currentPopupState.cleanedText;

    if (text) {
      clipboard.writeText(text);
      suppressCurrentSelection();
    }
  });

  ipcMain.handle("popup:close", () => {
    suppressCurrentSelection();
    popupWindow?.hide();
  });

  ipcMain.handle("popup:togglePin", () => {
    updatePopupState({ ...currentPopupState, pinned: !currentPopupState.pinned });
  });

  ipcMain.handle("popup:translateSelection", async () => {
    const cleanedText = currentPopupState.cleanedText;
    const targetLanguage = normalizeTargetLanguage(settings.targetLanguage);

    if (cleanedText) {
      suppressCurrentSelection();
      lastCleanedText = cleanedText;
      try {
        await translateText(cleanedText, { bypassCache: false, mode: settings.mode, targetLanguage });
      } catch (error) {
        updatePopupState({
          status: "error",
          pinned: currentPopupState.pinned,
          cleanedText,
          targetLanguage,
          error: toUserMessage(error)
        });
      }
    }
  });

  ipcMain.handle("popup:retry", async (_event, targetLanguage?: string) => {
    if (lastCleanedText) {
      await translateText(lastCleanedText, {
        bypassCache: true,
        mode: settings.mode,
        targetLanguage: resolvePopupTargetLanguage(targetLanguage)
      });
    }
  });

  ipcMain.handle("popup:explainTerms", async (_event, targetLanguage?: string) => {
    if (lastCleanedText) {
      await translateText(lastCleanedText, {
        bypassCache: true,
        mode: "terminology",
        targetLanguage: resolvePopupTargetLanguage(targetLanguage)
      });
    }
  });

  ipcMain.handle("popup:resize", (_event, requestedHeight: number) => {
    if (!popupWindow) {
      return;
    }

    if (currentPopupState.status === "selection") {
      popupWindow.setSize(SELECTION_POPUP_WIDTH, SELECTION_POPUP_HEIGHT, false);

      if (lastCursorPoint) {
        positionPopup(lastCursorPoint, SELECTION_POPUP_WIDTH, SELECTION_POPUP_HEIGHT);
      }

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
  const targetLanguage = normalizeTargetLanguage(settings.targetLanguage);

  if (!cleanedText) {
    await showPopup({
      status: "error",
      pinned: currentPopupState.pinned,
      error: buildEmptyClipboardMessage(),
      sourceText: rawText,
      cleanedText: "",
      targetLanguage
    });
    return;
  }

  lastCleanedText = cleanedText;
  activeSelectionText = null;
  try {
    await translateText(cleanedText, { bypassCache: false, mode: settings.mode, targetLanguage });
  } catch (error) {
    updatePopupState({
      status: "error",
      pinned: currentPopupState.pinned,
      error: toUserMessage(error),
      cleanedText,
      targetLanguage
    });
  }
}

async function showSelectionPopup(rawText: string, anchorPoint: Point): Promise<void> {
  if (!settings.enableSelectionPopup) {
    return;
  }

  const cleanedText = cleanSelectedText(rawText, { enabled: settings.cleanPdfText });

  if (!cleanedText) {
    return;
  }

  if (shouldSuppressSelection(cleanedText)) {
    return;
  }

  if (suppressedSelectionText && suppressedSelectionText !== cleanedText) {
    clearSuppressedSelection();
  }

  const shownAt = Date.now();

  if (
    lastSelectionPopup &&
    lastSelectionPopup.text === cleanedText &&
    shownAt - lastSelectionPopup.shownAt <= SELECTION_POPUP_COOLDOWN_MS
  ) {
    return;
  }

  lastSelectionPopup = { text: cleanedText, shownAt };
  lastCleanedText = cleanedText;
  activeSelectionText = cleanedText;
  lastCursorPoint = anchorPoint;

  await showPopup({
    status: "selection",
    pinned: false,
    sourceText: rawText,
    selectedText: cleanedText,
    cleanedText
  });
}

async function translateText(
  cleanedText: string,
  options: { bypassCache: boolean; mode: TranslateMode; targetLanguage: string }
): Promise<void> {
  const targetLanguage = normalizeTargetLanguage(options.targetLanguage, settings.targetLanguage);
  lastCursorPoint = lastCursorPoint ?? screen.getCursorScreenPoint();
  await showPopup({
    status: "loading",
    pinned: currentPopupState.pinned,
    cleanedText,
    sourceText: cleanedText,
    translation: "",
    cached: false,
    targetLanguage
  });

  const apiKey = await getApiKeySafely();

  if (!apiKey) {
    updatePopupState({
      status: "error",
      pinned: currentPopupState.pinned,
      cleanedText,
      targetLanguage,
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
    targetLanguage,
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
        cached: true,
        targetLanguage
      });
      return;
    }
  }

  const client = new DeepSeekClient({ apiKey });
  const result = await client.translate({
    text: cleanedText,
    model: settings.model,
    mode: options.mode,
    targetLanguage,
    glossary
  });

  if (settings.enableCache && result.translation) {
    await cacheStore.set({
      key: cacheKey,
      cleanedText,
      translation: result.translation,
      model: settings.model,
      mode: options.mode,
      targetLanguage,
      glossaryVersion,
      createdAt: new Date().toISOString()
    });
  }

  updatePopupState({
    status: "success",
    pinned: currentPopupState.pinned,
    cleanedText,
    translation: result.translation,
    cached: false,
    targetLanguage
  });
}

function resolvePopupTargetLanguage(value: unknown): string {
  return normalizeTargetLanguage(value, currentPopupState.targetLanguage ?? settings.targetLanguage);
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

function startMacSelectionWatcher(): void {
  if (process.platform !== "darwin") {
    macSelectionStatus = {
      available: false,
      running: false,
      message: "自动选区浮窗目前仅支持 macOS。"
    };
    return;
  }

  if (macSelectionWatcher) {
    macSelectionStatus = {
      available: true,
      running: true,
      message: "自动选区浮窗监听已启用。"
    };
    return;
  }

  try {
    macSelectionBuffer = "";
    lastSelectionPopup = null;
    macSelectionWatcher = spawn("/usr/bin/swift", ["-e", MAC_SELECTION_WATCHER_SCRIPT], {
      stdio: "pipe"
    }) as ChildProcessWithoutNullStreams;
    macSelectionStatus = {
      available: true,
      running: true,
      message: "自动选区浮窗监听已启用。"
    };

    macSelectionWatcher.stdout.setEncoding("utf8");
    macSelectionWatcher.stdout.on("data", (chunk: string) => {
      macSelectionBuffer += chunk;
      const lines = macSelectionBuffer.split(/\r?\n/);
      macSelectionBuffer = lines.pop() ?? "";

      for (const line of lines) {
        handleMacSelectionWatcherLine(line);
      }
    });

    macSelectionWatcher.stderr.setEncoding("utf8");
    macSelectionWatcher.stderr.on("data", (chunk: string) => {
      devLog("macOS selection watcher stderr.", chunk.slice(0, 240));
    });

    macSelectionWatcher.on("error", (error) => {
      macSelectionStatus = {
        available: false,
        running: false,
        message: `自动选区浮窗监听启动失败：${error.message}`
      };
      macSelectionWatcher = null;
    });

    macSelectionWatcher.on("close", (code) => {
      macSelectionStatus = {
        available: false,
        running: false,
        message: code === 0 ? "自动选区浮窗监听已停止。" : "自动选区浮窗监听已退出，请重启应用。"
      };

      macSelectionWatcher = null;
      macSelectionBuffer = "";
      lastSelectionPopup = null;
    });
  } catch (error) {
    macSelectionStatus = {
      available: false,
      running: false,
      message: error instanceof Error ? `自动选区浮窗监听启动失败：${error.message}` : "自动选区浮窗监听启动失败。"
    };
  }
}

function stopMacSelectionWatcher(): void {
  if (!macSelectionWatcher) {
    return;
  }

  const watcher = macSelectionWatcher;
  macSelectionWatcher = null;
  watcher.kill();
  macSelectionBuffer = "";
  lastSelectionPopup = null;
}

function refreshMacSelectionWatcherStatus(): void {
  if (process.platform !== "darwin") {
    macSelectionStatus = {
      available: false,
      running: false,
      message: "自动选区浮窗目前仅支持 macOS。"
    };
    return;
  }

  if (!macSelectionWatcher) {
    startMacSelectionWatcher();
    return;
  }

  if (macSelectionStatus.available) {
    macSelectionStatus = {
      available: true,
      running: true,
      message:
        lastSelectionEmptyAt > 0
          ? "自动选区监听已启用；当前前台应用暂未通过辅助功能暴露选中文本，可用 Cmd+C+C 兜底。"
          : "自动选区浮窗监听已启用。"
    };
  }
}

function handleMacSelectionWatcherLine(line: string): void {
  const [kind, payload, rawX, rawY] = line.split("\t");

  if (kind === "mouse_down") {
    hidePopupIfUnpinned(parseSelectionAnchor(payload, rawX));
    return;
  }

  if (kind === "status") {
    handleMacSelectionStatus(payload);
    return;
  }

  if (kind !== "selection" || !payload || BrowserWindow.getFocusedWindow()) {
    return;
  }

  if (!settings.enableSelectionPopup) {
    clearSuppressedSelection();
    return;
  }

  const text = Buffer.from(payload, "base64").toString("utf8");
  const anchorPoint = parseSelectionAnchor(rawX, rawY);
  lastSelectionEmptyAt = 0;
  macSelectionStatus = {
    available: true,
    running: Boolean(macSelectionWatcher),
    message: "自动选区浮窗监听已启用。"
  };
  void handleMacSelectionDetected(text, anchorPoint);
}

function parseSelectionAnchor(rawX: string | undefined, rawY: string | undefined): Point {
  const x = rawX ? Number(rawX) : Number.NaN;
  const y = rawY ? Number(rawY) : Number.NaN;

  if (!Number.isFinite(x) || !Number.isFinite(y)) {
    return screen.getCursorScreenPoint();
  }

  return { x: Math.round(x), y: Math.round(y) };
}

function hidePopupIfUnpinned(mousePoint?: Point): void {
  if (!popupWindow || popupWindow.isDestroyed() || !popupWindow.isVisible() || currentPopupState.pinned) {
    return;
  }

  if (mousePoint && isPointInsidePopup(mousePoint)) {
    return;
  }

  suppressCurrentSelection();
  popupWindow.hide();
}

function suppressCurrentSelection(): void {
  const cleanedText = currentPopupState.cleanedText ?? activeSelectionText;

  if (!cleanedText) {
    return;
  }

  if (currentPopupState.status === "selection" || activeSelectionText === cleanedText) {
    suppressedSelectionText = cleanedText;
  }
}

function clearSuppressedSelection(): void {
  suppressedSelectionText = null;
  activeSelectionText = null;
}

function shouldSuppressSelection(cleanedText: string): boolean {
  return suppressedSelectionText === cleanedText;
}

function isPointInsidePopup(point: Point): boolean {
  if (!popupWindow || popupWindow.isDestroyed()) {
    return false;
  }

  const bounds = popupWindow.getBounds();
  return (
    point.x >= bounds.x &&
    point.x <= bounds.x + bounds.width &&
    point.y >= bounds.y &&
    point.y <= bounds.y + bounds.height
  );
}

function handleMacSelectionStatus(status: string | undefined): void {
  if (status === "accessibility_denied") {
    macSelectionStatus = {
      available: false,
      running: false,
      message: "自动选区浮窗需要 macOS 辅助功能权限；未授权时仍可使用 Cmd+C+C。"
    };
    return;
  }

  if (status === "ready") {
    macSelectionStatus = {
      available: true,
      running: Boolean(macSelectionWatcher),
      message: "自动选区浮窗监听已启用。"
    };
    return;
  }

  if (status === "selection_empty") {
    lastSelectionEmptyAt = Date.now();
    clearSuppressedSelection();
    macSelectionStatus = {
      available: true,
      running: Boolean(macSelectionWatcher),
      message: "自动选区监听已启用；当前前台应用暂未通过辅助功能暴露选中文本，可用 Cmd+C+C 兜底。"
    };
    return;
  }

  if (status === "mouse_tap_unavailable") {
    macSelectionStatus = {
      available: false,
      running: false,
      message: "自动选区鼠标监听启动失败；请检查 macOS 辅助功能权限，或继续使用 Cmd+C+C。"
    };
  }
}

async function handleMacSelectionDetected(rawText: string, anchorPoint: Point): Promise<void> {
  if (!settings.enableSelectionPopup) {
    return;
  }

  try {
    await showSelectionPopup(rawText, anchorPoint);
  } catch (error) {
    devLog("Selection popup failed.", toUserMessage(error));
  }
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

function buildInitialDoubleCopyStatus(): WatcherStatus {
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

function buildInitialSelectionStatus(): WatcherStatus {
  if (process.platform !== "darwin") {
    return {
      available: false,
      running: false,
      message: "自动选区浮窗目前仅支持 macOS。"
    };
  }

  return {
    available: true,
    running: false,
    message: "自动选区浮窗监听未启用。"
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

  popupWindow.on("blur", () => {
    hidePopupIfUnpinned();
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
  resizePopupForState(state);
  const [width, height] = popup.getSize();
  positionPopup(lastCursorPoint ?? screen.getCursorScreenPoint(), width, height);
  popup.show();
  popup.moveTop();
  devLog("Popup shown.", { visible: popup.isVisible(), bounds: popup.getBounds() });
  sendPopupState();
}

function resizePopupForState(state: PopupState): void {
  if (!popupWindow || popupWindow.isDestroyed()) {
    return;
  }

  if (state.status === "selection") {
    popupWindow.setSize(SELECTION_POPUP_WIDTH, SELECTION_POPUP_HEIGHT, false);
    return;
  }

  resizeExistingPopupToSettings();
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

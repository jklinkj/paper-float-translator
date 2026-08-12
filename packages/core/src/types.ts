export const DEFAULT_DEEPSEEK_MODEL = "deepseek-v4-flash" as const;
export const HIGH_QUALITY_DEEPSEEK_MODEL = "deepseek-v4-pro" as const;
export const DEFAULT_TARGET_LANGUAGE = "中文" as const;

export type DeepSeekModel =
  | typeof DEFAULT_DEEPSEEK_MODEL
  | typeof HIGH_QUALITY_DEEPSEEK_MODEL;

export type TranslateMode =
  | "academic_zh"
  | "bilingual"
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
  enableAutomaticSelection: boolean;
  targetLanguage: string;
  popupWidth: number;
}

export const DEFAULT_SETTINGS: AppSettings = {
  model: DEFAULT_DEEPSEEK_MODEL,
  mode: "academic_zh",
  cleanPdfText: true,
  enableCache: true,
  enableSelectionPopup: true,
  enableAutomaticSelection: true,
  targetLanguage: DEFAULT_TARGET_LANGUAGE,
  popupWidth: 420
};

export const POPUP_PROTOCOL_VERSION = 1 as const;

export type PopupStatus =
  | "hidden"
  | "selection_pending"
  | "selection_ready"
  | "translating"
  | "translated"
  | "error";

export type PopupErrorKind =
  | "configuration"
  | "permission"
  | "selection"
  | "translation"
  | "protocol"
  | "unknown";

export type PopupRecoveryAction =
  | "retry_translation"
  | "open_settings"
  | "open_accessibility"
  | "open_input_monitoring"
  | "reselect";

export interface PopupState {
  protocolVersion: typeof POPUP_PROTOCOL_VERSION;
  revision: number;
  selectionRevision: number;
  visible: boolean;
  status: PopupStatus;
  sourceText?: string;
  selectedText?: string;
  cleanedText?: string;
  translation?: string;
  error?: string;
  errorKind?: PopupErrorKind;
  retryable?: boolean;
  recoveryAction?: PopupRecoveryAction;
  cached?: boolean;
  targetLanguage?: string;
  pinned: boolean;
}

export const HIDDEN_POPUP_STATE: PopupState = {
  protocolVersion: POPUP_PROTOCOL_VERSION,
  revision: 0,
  selectionRevision: 0,
  visible: false,
  status: "hidden",
  pinned: false
};

/**
 * Merge a snapshot or event into the renderer's last accepted state. Events may
 * arrive before the post-subscription snapshot, so only a strictly newer
 * revision is allowed to replace an existing state.
 */
export function reconcilePopupState(current: PopupState | null, incoming: PopupState): PopupState {
  assertPopupProtocol(incoming);
  if (current && incoming.revision <= current.revision) {
    return current;
  }
  return incoming;
}

export function assertPopupProtocol(state: PopupState): void {
  if (state.protocolVersion !== POPUP_PROTOCOL_VERSION) {
    throw new Error(
      `Unsupported popup protocol version ${String(state.protocolVersion)}; expected ${POPUP_PROTOCOL_VERSION}.`
    );
  }
  if (!Number.isSafeInteger(state.revision) || state.revision < 0) {
    throw new Error(`Invalid popup revision ${String(state.revision)}.`);
  }
  if (!Number.isSafeInteger(state.selectionRevision) || state.selectionRevision < 0) {
    throw new Error(`Invalid selection revision ${String(state.selectionRevision)}.`);
  }
  const statuses: readonly PopupStatus[] = [
    "hidden",
    "selection_pending",
    "selection_ready",
    "translating",
    "translated",
    "error"
  ];
  if (!statuses.includes(state.status)) {
    throw new Error(`Invalid popup status ${String(state.status)}.`);
  }
  if (state.visible !== (state.status !== "hidden")) {
    throw new Error(`Popup visibility does not match status ${state.status}.`);
  }
  if (state.status === "error") {
    const recoveryActions: readonly PopupRecoveryAction[] = [
      "retry_translation",
      "open_settings",
      "open_accessibility",
      "open_input_monitoring",
      "reselect"
    ];
    if (!state.error?.trim() || !state.errorKind || typeof state.retryable !== "boolean") {
      throw new Error("Popup error state is missing typed error metadata.");
    }
    if (!state.recoveryAction || !recoveryActions.includes(state.recoveryAction)) {
      throw new Error(`Invalid popup recovery action ${String(state.recoveryAction)}.`);
    }
    if (state.retryable !== (state.recoveryAction === "retry_translation")) {
      throw new Error("Popup retryability does not match its recovery action.");
    }
    if (state.errorKind === "configuration" && state.recoveryAction !== "open_settings") {
      throw new Error("Configuration errors must open settings.");
    }
    if (
      state.errorKind === "permission" &&
      state.recoveryAction !== "open_accessibility" &&
      state.recoveryAction !== "open_input_monitoring"
    ) {
      throw new Error("Permission errors must identify the exact System Settings pane.");
    }
    if (state.errorKind === "selection" && state.recoveryAction !== "reselect") {
      throw new Error("Selection errors must ask for a new selection.");
    }
    if (state.errorKind === "translation" && state.recoveryAction !== "retry_translation") {
      throw new Error("Translation errors must offer a retry.");
    }
  }
}

export interface WatcherStatus {
  available: boolean;
  running: boolean;
  message: string;
  code?: string;
}

export type PermissionGrant = "granted" | "denied" | "unknown" | "unsupported";

export type CapabilityHealth = "ready" | "degraded" | "disabled" | "unavailable" | "unknown";

export interface PermissionCapabilityState {
  grant: PermissionGrant;
  health: CapabilityHealth;
  statusCode?: string;
}

export interface RuntimeCapabilityState {
  health: CapabilityHealth;
  statusCode?: string;
}

/**
 * A point-in-time view of macOS grants and the independent runtime sources that
 * implement selection and Cmd+C+C. Permissions intentionally do not imply that
 * an event tap or AX observer is healthy.
 */
export interface CapabilitySnapshot {
  accessibility: PermissionCapabilityState;
  listenEvent: PermissionCapabilityState;
  mouseTap: RuntimeCapabilityState;
  keyTap: RuntimeCapabilityState;
  axSelectedTextObserver: RuntimeCapabilityState;
  directSelectionRead: RuntimeCapabilityState;
  clipboardDoubleCopyFallback: RuntimeCapabilityState;
}

export type CapabilityPermissionAction = "open_accessibility_settings" | "open_input_monitoring_settings";

export interface CapabilityPresentation {
  value: string;
  tone: "ok" | "warning" | "neutral";
  detail: string;
}

export const UNKNOWN_CAPABILITY_SNAPSHOT: CapabilitySnapshot = {
  accessibility: { grant: "unknown", health: "unknown" },
  listenEvent: { grant: "unknown", health: "unknown" },
  mouseTap: { health: "unknown" },
  keyTap: { health: "unknown" },
  axSelectedTextObserver: { health: "unknown" },
  directSelectionRead: { health: "unknown" },
  clipboardDoubleCopyFallback: { health: "unknown" }
};

export function getCapabilityPermissionActions(snapshot: CapabilitySnapshot): CapabilityPermissionAction[] {
  const actions: CapabilityPermissionAction[] = [];

  if (snapshot.accessibility.grant === "denied") {
    actions.push("open_accessibility_settings");
  }
  if (snapshot.listenEvent.grant === "denied") {
    actions.push("open_input_monitoring_settings");
  }

  return actions;
}

export function getSelectionCapabilityPresentation(snapshot: CapabilitySnapshot): CapabilityPresentation {
  const directReadReady = snapshot.directSelectionRead.health === "ready";
  const mouseReady = snapshot.mouseTap.health === "ready";
  const observerReady = snapshot.axSelectedTextObserver.health === "ready";

  if (isWindowsCapabilitySnapshot(snapshot)) {
    if (snapshot.directSelectionRead.statusCode === "windows_shortcut_registration_failed") {
      return {
        value: "快捷键冲突",
        tone: "warning",
        detail: "Ctrl+Alt+T 无法注册，可能已被其他应用占用。"
      };
    }

    if (
      snapshot.directSelectionRead.health === "disabled" &&
      snapshot.directSelectionRead.statusCode === "selection_disabled"
    ) {
      return {
        value: "已关闭",
        tone: "neutral",
        detail: "Windows 选区取词已在设置中关闭。"
      };
    }

    const shortcutAvailable =
      directReadReady ||
      (snapshot.directSelectionRead.health === "unknown" &&
        snapshot.directSelectionRead.statusCode === "windows_uia_not_probed");
    const automaticDisabledBySetting =
      snapshot.mouseTap.statusCode === "windows_auto_selection_disabled_by_setting" &&
      snapshot.axSelectedTextObserver.statusCode === "windows_auto_selection_disabled_by_setting";

    if (shortcutAvailable && mouseReady && observerReady) {
      return {
        value: "鼠标划词 + Ctrl+Alt+T 可用",
        tone: "ok",
        detail: "仅由完整鼠标选择手势触发，UI Automation 只负责读取；复制粘贴与键盘选择不会自动弹出，也不会改写剪贴板。"
      };
    }

    if (shortcutAvailable && (mouseReady || observerReady)) {
      return {
        value: "自动划词降级可用",
        tone: "warning",
        detail: mouseReady
          ? "鼠标手势门控可用，UI Automation 选区读取辅助当前受限；Ctrl+Alt+T 仍可使用。"
          : "UI Automation 读取可用，鼠标手势门控当前受限；Ctrl+Alt+T 仍可使用。"
      };
    }

    if (shortcutAvailable) {
      const automaticUnavailable =
        !automaticDisabledBySetting &&
        [snapshot.mouseTap.health, snapshot.axSelectedTextObserver.health].some(
          (health) => health === "degraded" || health === "unavailable"
        );
      return {
        value: directReadReady ? "Ctrl+Alt+T 已验证" : "Ctrl+Alt+T 可用",
        tone: automaticUnavailable ? "warning" : "ok",
        detail: automaticUnavailable
          ? "自动划词监听当前受限；Ctrl+Alt+T 与 Ctrl+C+C 仍可使用。"
          : directReadReady
            ? "已通过 Windows UI Automation 读取选区，不会改写剪贴板。"
            : "全局快捷键已注册；选中文本后按 Ctrl+Alt+T 即可读取。"
      };
    }

    return {
      value: snapshot.directSelectionRead.health === "unavailable" ? "取词不可用" : "取词需重试",
      tone: "warning",
      detail:
        snapshot.directSelectionRead.health === "unavailable"
          ? "Windows UI Automation 取词当前不可用；Ctrl+C+C 仍可作为兜底。"
          : "最近一次 Windows 选区读取没有成功；请重新选择文本后再按 Ctrl+Alt+T。"
    };
  }

  if (directReadReady && mouseReady && observerReady) {
    return {
      value: "完整可用",
      tone: "ok",
      detail: "鼠标选区与辅助功能选区通知均已就绪。"
    };
  }

  if (
    directReadReady &&
    mouseReady &&
    snapshot.axSelectedTextObserver.health === "degraded" &&
    snapshot.axSelectedTextObserver.statusCode === "selection_mouse_ready_ax_observer_limited"
  ) {
    return {
      value: "划词就绪",
      tone: "ok",
      detail: "鼠标划词与直接读取已就绪；切换到文档后会自动重新绑定 AX 选区通知。"
    };
  }

  if (directReadReady && (mouseReady || observerReady)) {
    return {
      value: "降级可用",
      tone: "warning",
      detail: mouseReady
        ? "鼠标选区可用，AX selectedText 通知当前受限。"
        : "AX selectedText 通知可用，鼠标事件监听当前受限。"
    };
  }

  if (snapshot.keyTap.health === "ready" || snapshot.clipboardDoubleCopyFallback.health === "ready") {
    return {
      value: "Cmd+C+C 可用",
      tone: "warning",
      detail:
        snapshot.keyTap.health === "ready"
          ? "自动选区路径当前不可用，Cmd+C+C 按键监听仍可使用。"
          : "自动选区路径当前不可用，可使用 Cmd+C+C 剪贴板回退。"
    };
  }

  const unknown = [
    snapshot.mouseTap.health,
    snapshot.axSelectedTextObserver.health,
    snapshot.directSelectionRead.health
  ].some((health) => health === "unknown");

  return unknown
    ? {
        value: "状态未知",
        tone: "warning",
        detail: "尚未收到完整的原生能力状态，请刷新诊断。"
      }
    : {
        value: "不可用",
        tone: "warning",
        detail: "自动选区与剪贴板回退当前均不可用。"
      };
}

export function getDoubleCopyCapabilityPresentation(snapshot: CapabilitySnapshot): CapabilityPresentation {
  if (isWindowsCapabilitySnapshot(snapshot)) {
    if (
      snapshot.keyTap.health === "disabled" &&
      snapshot.clipboardDoubleCopyFallback.health === "disabled"
    ) {
      return {
        value: "已关闭",
        tone: "neutral",
        detail: "Ctrl+C+C 双复制取词已在设置中关闭。"
      };
    }

    if (
      snapshot.keyTap.health === "ready" &&
      snapshot.clipboardDoubleCopyFallback.health === "ready"
    ) {
      return {
        value: "Ctrl+C+C 可用",
        tone: "ok",
        detail: "只观察用户主动执行的复制，不会模拟按键或轮询剪贴板。"
      };
    }

    return {
      value: "Ctrl+C+C 不可用",
      tone: "warning",
      detail: "Windows 后台复制监听未能启动；仍可使用 Ctrl+Alt+T 取词。"
    };
  }

  if (snapshot.keyTap.health === "ready") {
    return {
      value: "按键监听可用",
      tone: "ok",
      detail: "Cmd+C+C 按键监听已就绪；剪贴板轮询回退当前无需启用。"
    };
  }

  switch (snapshot.clipboardDoubleCopyFallback.health) {
    case "ready":
      return {
        value: "回退可用",
        tone: "ok",
        detail: "剪贴板双复制回退已就绪。"
      };
    case "degraded":
      return {
        value: "需再次复制",
        tone: "warning",
        detail: "一次轮询跨过了多个剪贴板版本，无法证明发生了两次相同复制；请重新按两次 Cmd+C。"
      };
    case "unknown":
      return {
        value: "状态未知",
        tone: "warning",
        detail: "尚未收到剪贴板回退能力状态。"
      };
    default:
      return {
        value: "不可用",
        tone: "warning",
        detail: "剪贴板双复制回退当前不可用。"
      };
  }
}

function isWindowsCapabilitySnapshot(snapshot: CapabilitySnapshot): boolean {
  return [
    snapshot.accessibility.statusCode,
    snapshot.listenEvent.statusCode,
    snapshot.mouseTap.statusCode,
    snapshot.keyTap.statusCode,
    snapshot.axSelectedTextObserver.statusCode,
    snapshot.directSelectionRead.statusCode,
    snapshot.clipboardDoubleCopyFallback.statusCode
  ].some((statusCode) => statusCode?.startsWith("windows_") === true);
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

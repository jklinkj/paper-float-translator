import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  DEFAULT_SETTINGS,
  HIDDEN_POPUP_STATE,
  POPUP_PROTOCOL_VERSION,
  reconcilePopupState,
  type AppSettings,
  type CapabilitySnapshot,
  type PopupState,
  type WatcherStatus
} from "@paper-float-translator/core/renderer";

export type SignatureKind = "adhoc" | "apple_development" | "developer_id" | "unknown";

export interface PermissionDiagnostics {
  bundleIdentifier: string;
  appVersion: string;
  bundlePath: string;
  executablePath: string;
  teamIdentifier?: string;
  cdHash?: string;
  signatureKind: SignatureKind;
  accessibilityTrusted: boolean;
  selectionRead?: SelectionReadDiagnostics;
}

export interface SelectionReadDiagnostics {
  status: string;
  reason: string;
  foundText: boolean;
  candidateCount: number;
  durationMs: number;
  sourceBundleId: string;
  axError: string;
  generation?: number;
  attempt?: number;
  triggerToReadMs?: number;
  terminal?: boolean;
}

export interface SettingsPayload {
  settings: AppSettings;
  hasApiKey: boolean;
  apiKeyStatus: ApiKeyStatus;
  apiKeyStorage: ApiKeyStorage;
  runtimePlatform: RuntimePlatform;
  doubleCopyStatus: WatcherStatus;
  selectionStatus: WatcherStatus;
  capabilitySnapshot: CapabilitySnapshot;
  permissionDiagnostics?: PermissionDiagnostics;
  runtimeWarning?: string;
}

export type ApiKeyStatus = "configured" | "missing" | "unavailable";
export type ApiKeyStorage = "local_file" | "system_keychain" | "windows_credential_manager";
export type RuntimePlatform = "macos" | "windows" | "other";

export interface PopupKeyboardEntry {
  revision: number;
  selectionRevision: number;
  direction: "forward" | "backward";
}

export type AcceptanceScenario =
  | "textEditDrag"
  | "textEditDoubleClick"
  | "textEditTripleClick"
  | "textEditShiftExtend"
  | "textEditKeyboardSelection"
  | "sameTextSameLocation"
  | "sameTextDifferentLocation"
  | "safariFixture"
  | "previewFixture"
  | "rapidAThenB"
  | "selfWindowIsolation"
  | "tapDisabledRecovery"
  | "watcherRestart";

export interface AcceptanceRuntimeStatus {
  enabled: boolean;
  tapInjectionAvailable: boolean;
  preset: string;
  phase: "idle" | "running" | "ended";
  armed: { scenario: AcceptanceScenario; ordinal: number } | null;
  selfIsolationActive: boolean;
  pendingGenerations: number;
  pendingRapidProbes: number;
  recordedGenerations: number;
  duplicateGenerationsSuppressed: number;
}

export interface AcceptanceScenarioOrdinal {
  scenario: AcceptanceScenario;
  ordinal: number;
}

export interface AcceptanceLatencyThreshold {
  p95LimitMs: number;
  maxLimitMs: number;
}

export interface AcceptanceLatencySummary {
  sampleCount: number;
  p95Ms: number | null;
  maxMs: number | null;
}

export interface AcceptanceScenarioLatencySummary {
  scenario: AcceptanceScenario;
  latency: AcceptanceLatencySummary;
  threshold: AcceptanceLatencyThreshold | null;
  thresholdPassed: boolean;
}

export interface AcceptanceSelectionSummary {
  expectedCount: number;
  popupCommitCount: number;
  missing: AcceptanceScenarioOrdinal[];
  duplicates: AcceptanceScenarioOrdinal[];
  unexpected: AcceptanceScenarioOrdinal[];
  wrongClassification: AcceptanceScenarioOrdinal[];
  staleWriteCommittedCount: number;
}

export interface AcceptanceRapidTransitionSummary {
  expectedPairs: number;
  maxTriggerGapMs: number;
  finalBPairs: number;
  missingPairs: number[];
  missingAPairs: number[];
  missingBPairs: number[];
  duplicateAPairs: number[];
  duplicateBPairs: number[];
  outOfOrderPairs: number[];
  gapExceededPairs: number[];
  wrongFinalPairs: number[];
  unexpectedPairs: number[];
  unexpectedEvidencePairs: number[];
  unexpectedEvidenceCount: number;
  staleARejectedCount: number;
  missingStaleRejectionPairs: number[];
  duplicateStaleRejectionPairs: number[];
  invalidStaleRejectionPairs: number[];
  staleAWriteCount: number;
  passed: boolean;
}

export interface AcceptanceTapRecoverySummary {
  expectedAttempts: number;
  resolutionLimitMs: number;
  readyWithinLimit: number;
  degradedWithinLimit: number;
  late: number[];
  unresolved: number[];
  unexpectedAttempts: number[];
  duplicateDisabledEvents: number;
  duplicateResolutionEvents: number;
  maxResolutionMs: number | null;
  passed: boolean;
}

export interface AcceptanceWatcherResourceCounts {
  snapshotVersion: number;
  selectionObservers: number;
  selectionObserverSources: number;
  keyEventTaps: number;
  keyEventTapSources: number;
  mouseEventTaps: number;
  mouseEventTapSources: number;
  pasteboardTimers: number;
  workspaceActivationObservers: number;
  callbacks: number;
  contexts: number;
  effectiveSourceSets: number;
  totalSources: number;
}

export interface AcceptanceWatcherResourceSummary {
  expectedRestarts: number;
  sampleCount: number;
  missingRestarts: number[];
  missingReleaseRestarts: number[];
  missingStartedRestarts: number[];
  unexpectedRestarts: number[];
  duplicateReleaseRestarts: number[];
  duplicateStartedRestarts: number[];
  duplicateActiveRestarts: number[];
  generationMismatchRestarts: number[];
  outOfOrderRestarts: number[];
  releasedResourceLeaks: number[];
  peak: AcceptanceWatcherResourceCounts;
  finalActive: AcceptanceWatcherResourceCounts | null;
  growthOrDuplicateDetected: boolean;
  finalSingleEffectiveSource: boolean;
  passed: boolean;
}

export interface AcceptanceSelfIsolationSummary {
  configuredMinDurationMs: number | null;
  observedDurationMs: number | null;
  selectionTriggerCount: number;
  startCount: number;
  endCount: number;
  lifecycleGenerationMatched: boolean;
  lifecycleOrderValid: boolean;
  interactionRecordCount: number;
  interactionGenerationMatched: boolean;
  totalInteractionCount: number;
  settingsInteractionCount: number;
  popupInteractionCount: number;
  pointerInteractionCount: number;
  keyboardInteractionCount: number;
  populatedBuckets: number[];
  settingsBuckets: number[];
  popupBuckets: number[];
  everyBucketCovered: boolean;
  windowBucketCoverageValid: boolean;
  passed: boolean;
}

export interface AcceptanceLocationEvidence {
  category: "referenceA" | "referenceB";
  result:
    | "referenceEstablished"
    | "matchedExpectedCluster"
    | "mismatchedExpectedCluster"
    | "insufficientClusterSeparation";
}

export interface AcceptanceSelectionRecord {
  recordType: "selection";
  scenario: AcceptanceScenario;
  ordinal: number;
  generation: number;
  selectionRevision: number;
  popupRevision: number | null;
  controllerSnapshotSelectionRevision: number | null;
  controllerSnapshotPopupRevision: number | null;
  attempt: number;
  reason:
    | "mouseDrag"
    | "mouseDoubleClick"
    | "mouseTripleClick"
    | "mouseShiftExtend"
    | "accessibilityNotification";
  sourceApp: {
    kind: "textEdit" | "safari" | "preview" | "selfApplication" | "unknown" | "otherBundleIdentifier";
    bundleIdentifier?: string;
  };
  eventOffsetMicros: number;
  controllerCommitOffsetMicros: number | null;
  endToEndLatencyMs: number;
  outcome:
    | "popupCommitted"
    | "noSelection"
    | "readFailed"
    | "staleWriteRejected"
    | "staleWriteCommitted"
    | "cancelled";
  expectedMatch: "match" | "a" | "b" | "mismatch" | "notApplicable";
  locationEvidence?: AcceptanceLocationEvidence;
}

export interface AcceptanceLifecycleRecord {
  recordType: "lifecycle";
  scenario: AcceptanceScenario;
  ordinal: number;
  generation: number;
  eventOffsetMicros: number;
  event:
    | "scenarioStarted"
    | "scenarioEnded"
    | "tapDisabled"
    | "tapReady"
    | "tapDegraded"
    | "watcherStarted"
    | "watcherReady"
    | "watcherDegraded"
    | "watcherReleased";
}

export interface AcceptanceWatcherResourceRecord {
  recordType: "watcherResources";
  scenario: AcceptanceScenario;
  ordinal: number;
  generation: number;
  eventOffsetMicros: number;
  event: AcceptanceLifecycleRecord["event"];
  resources: AcceptanceWatcherResourceCounts;
}

export interface AcceptanceSelfInteractionRecord {
  recordType: "selfInteraction";
  scenario: AcceptanceScenario;
  ordinal: number;
  generation: number;
  aggregates: Array<{
    window: "settings" | "popup";
    kind: "pointer" | "keyboard";
    count: number;
    bucketCoverage: number[];
  }>;
}

export type AcceptanceRecord =
  | AcceptanceSelectionRecord
  | AcceptanceLifecycleRecord
  | AcceptanceWatcherResourceRecord
  | AcceptanceSelfInteractionRecord;

export interface AcceptanceReport {
  schemaVersion: number;
  reportVersion: number;
  clock: "monotonic";
  app: {
    version: string;
    bundleIdentifier: string;
    buildKind: "adHocAcceptance" | "development" | "developerId" | "unknown";
  };
  system: {
    operatingSystem: "macOs";
    version: string;
    architecture: "arm64" | "x86_64";
  };
  baselineApplications: Array<{
    application: "textEdit" | "safari" | "preview";
    version: string;
  }>;
  fixtures: Array<{
    kind: "textEditPlainText" | "safariHtml" | "previewPdf";
    fixtureId: string;
    sha256: string;
  }>;
  multiClickQuietWindowMs: number;
  capacity: number;
  recordedRecords: number;
  overflowedRecords: number;
  rejectedRecords: number;
  status: {
    integrityValid: boolean;
    acceptancePassed: boolean;
    valid: boolean;
    invalidReasons: string[];
  };
  summary: {
    totalRecords: number;
    selectionRecords: number;
    uniqueSelectionRevisions: number;
    uniquePopupRevisions: number;
    duplicateSelectionRevisions: number[];
    duplicatePopupRevisions: number[];
    duplicateEvidence: {
      nativeReadReplays: number;
      popupOutcomeReplays: number;
      tapStatusReplays: number;
      watcherPhaseReplays: number;
      rapidProbeReplays: number;
    };
    selections: AcceptanceSelectionSummary;
    latency: AcceptanceLatencySummary;
    scenarioLatency: AcceptanceScenarioLatencySummary[];
    rapidAToB: AcceptanceRapidTransitionSummary;
    tapRecovery: AcceptanceTapRecoverySummary;
    watcherResources: AcceptanceWatcherResourceSummary;
    selfIsolation: AcceptanceSelfIsolationSummary;
  };
  records: AcceptanceRecord[];
}

export interface AcceptanceExportReceipt {
  path: string;
  recordedRecords: number;
  valid: boolean;
}

const browserRuntimePlatform: RuntimePlatform =
  new URLSearchParams(window.location.search).get("platform") === "windows"
    ? "windows"
    : "macos";
let browserSettings: AppSettings = {
  ...DEFAULT_SETTINGS,
  enableAutomaticSelection: browserRuntimePlatform !== "windows"
};
let browserHasApiKey = false;
let browserPopupState: PopupState = getInitialBrowserPopupState();
const browserPopupListeners = new Set<(state: PopupState) => void>();
let browserTranslationToken = 0;
let browserSelectionRevision = browserPopupState.selectionRevision;
const browserFailureAttempts = new Map<string, number>();
let browserAcceptanceStatus: AcceptanceRuntimeStatus = {
  enabled: new URLSearchParams(window.location.search).get("acceptance") === "1",
  tapInjectionAvailable: new URLSearchParams(window.location.search).get("acceptance") === "1",
  preset: "full_baseline",
  phase: "idle",
  armed: null,
  selfIsolationActive: false,
  pendingGenerations: 0,
  pendingRapidProbes: 0,
  recordedGenerations: 0,
  duplicateGenerationsSuppressed: 0
};
let browserAcceptanceReport: AcceptanceReport | null = null;
const browserCompletedAcceptanceArms = new Set<string>();

type PopupStateUpdate = Pick<PopupState, "status"> & Partial<Omit<PopupState, "status">>;

export const desktopApi = {
  getSettings: (): Promise<SettingsPayload> =>
    isTauriRuntime() ? invoke("get_settings") : browserResult("load", getBrowserSettingsPayload()),
  saveSettings: async (settings: AppSettings): Promise<SettingsPayload> => {
    if (isTauriRuntime()) {
      return invoke("save_settings", { settings });
    }

    if (browserShouldFail("save")) {
      return Promise.reject(new Error("固定测试：设置写入失败。"));
    }
    await browserDelay("save");
    browserSettings = settings;
    if (!browserSettings.enableSelectionPopup) {
      browserTranslationToken += 1;
      emitBrowserPopupState({
        ...HIDDEN_POPUP_STATE,
        selectionRevision: browserPopupState.selectionRevision,
        pinned: browserPopupState.pinned
      });
    }
    const payload = getBrowserSettingsPayload();
    if (browserShouldFail("save-runtime")) {
      payload.runtimeWarning = "自动划词监听切换没有在 2 秒内完成，已保留降级诊断。";
      payload.selectionStatus = {
        available: true,
        running: false,
        code: "selection_watcher_refresh_timeout",
        message: "自动选区监听刷新超时，原生监听器尚未返回启动状态；请再次刷新诊断。"
      };
      payload.capabilitySnapshot.mouseTap = {
        health: "degraded",
        statusCode: "selection_watcher_refresh_timeout"
      };
      payload.capabilitySnapshot.axSelectedTextObserver = {
        health: "degraded",
        statusCode: "selection_watcher_refresh_timeout"
      };
      payload.capabilitySnapshot.directSelectionRead = {
        health: "degraded",
        statusCode: "selection_watcher_refresh_timeout"
      };
    }
    return Promise.resolve(payload);
  },
  saveApiKey: (apiKey: string): Promise<SettingsPayload> => {
    if (isTauriRuntime()) {
      return invoke("save_api_key", { apiKey });
    }

    if (browserShouldFail("key")) {
      return Promise.reject(new Error("固定测试：API Key 写入失败。"));
    }
    browserHasApiKey = Boolean(apiKey.trim());
    return Promise.resolve(getBrowserSettingsPayload());
  },
  clearCache: (): Promise<{ ok: true }> =>
    isTauriRuntime() ? invoke("clear_cache") : browserResult("cache", { ok: true }),
  refreshWatcherStatus: (): Promise<SettingsPayload> => {
    if (isTauriRuntime()) {
      return invoke("refresh_watcher_status");
    }
    if (browserAcceptanceStatus.armed?.scenario === "watcherRestart") {
      browserCompletedAcceptanceArms.add(
        browserAcceptanceArmKey(
          browserAcceptanceStatus.armed.scenario,
          browserAcceptanceStatus.armed.ordinal
        )
      );
      browserAcceptanceStatus = {
        ...browserAcceptanceStatus,
        armed: null,
        recordedGenerations: browserAcceptanceStatus.recordedGenerations + 1
      };
    }
    if (
      new URLSearchParams(window.location.search).get("runtime") ===
      "external-settings-on-refresh"
    ) {
      browserSettings = {
        ...browserSettings,
        enableSelectionPopup: false,
        popupWidth: 500
      };
    }
    return browserResult("refresh", getBrowserSettingsPayload());
  },
  getAcceptanceStatus: (): Promise<AcceptanceRuntimeStatus> =>
    isTauriRuntime()
      ? invoke("acceptance_diagnostics_status")
      : browserResult("acceptance-status", { ...browserAcceptanceStatus }),
  startAcceptanceDiagnostics: (): Promise<AcceptanceRuntimeStatus> => {
    if (isTauriRuntime()) {
      return invoke("start_acceptance_diagnostics");
    }
    if (!browserAcceptanceStatus.enabled) {
      return Promise.reject(new Error("验收诊断未显式启用。"));
    }
    browserAcceptanceStatus = {
      ...browserAcceptanceStatus,
      phase: "running",
      armed: null,
      selfIsolationActive: false,
      pendingGenerations: 0,
      pendingRapidProbes: 0,
      recordedGenerations: 0,
      duplicateGenerationsSuppressed: 0
    };
    browserCompletedAcceptanceArms.clear();
    browserAcceptanceReport = null;
    return browserResult("acceptance-start", { ...browserAcceptanceStatus });
  },
  armAcceptanceScenario: (
    scenario: AcceptanceScenario,
    ordinal: number
  ): Promise<AcceptanceRuntimeStatus> => {
    if (isTauriRuntime()) {
      return invoke("arm_acceptance_scenario", { scenario, ordinal });
    }
    if (browserAcceptanceStatus.phase !== "running") {
      return Promise.reject(new Error("验收会话尚未开始。"));
    }
    const armKey = browserAcceptanceArmKey(scenario, ordinal);
    if (browserCompletedAcceptanceArms.has(armKey)) {
      return Promise.reject(new Error("该验收场景与序号已经完成，不能重复准备。"));
    }
    browserAcceptanceStatus = {
      ...browserAcceptanceStatus,
      armed: { scenario, ordinal }
    };
    return browserResult("acceptance-arm", { ...browserAcceptanceStatus });
  },
  setAcceptanceSelfIsolation: (
    action: "start" | "end"
  ): Promise<AcceptanceRuntimeStatus> => {
    if (isTauriRuntime()) {
      return invoke("set_acceptance_self_isolation", { action });
    }
    if (browserAcceptanceStatus.phase !== "running") {
      return Promise.reject(new Error("验收会话尚未开始。"));
    }
    browserAcceptanceStatus = {
      ...browserAcceptanceStatus,
      armed: null,
      selfIsolationActive: action === "start"
    };
    return browserResult("acceptance-isolation", { ...browserAcceptanceStatus });
  },
  recordAcceptanceSelfInteraction: (
    eventKind: "pointer" | "keyboard"
  ): Promise<void> =>
    isTauriRuntime()
      ? invoke("record_acceptance_self_interaction", { eventKind })
      : Promise.resolve(),
  injectAcceptanceTapDisabled: (): Promise<void> => {
    if (isTauriRuntime()) {
      return invoke("inject_acceptance_tap_disabled");
    }
    if (
      !browserAcceptanceStatus.tapInjectionAvailable ||
      browserAcceptanceStatus.armed?.scenario !== "tapDisabledRecovery"
    ) {
      return Promise.reject(new Error("当前验收构建或场景不允许 tap-disabled 注入。"));
    }
    const keepArmed = (new URLSearchParams(window.location.search).get("fail") ?? "")
      .split(",")
      .map((value) => value.trim())
      .includes("acceptance-tap-stuck");
    if (!keepArmed) {
      const completedArm = browserAcceptanceStatus.armed;
      if (completedArm) {
        browserCompletedAcceptanceArms.add(
          browserAcceptanceArmKey(completedArm.scenario, completedArm.ordinal)
        );
      }
      browserAcceptanceStatus = {
        ...browserAcceptanceStatus,
        armed: null,
        recordedGenerations: browserAcceptanceStatus.recordedGenerations + 1
      };
    }
    return browserResult("acceptance-tap", undefined);
  },
  endAcceptanceDiagnostics: (): Promise<AcceptanceReport> => {
    if (isTauriRuntime()) {
      return invoke("end_acceptance_diagnostics");
    }
    if (browserAcceptanceStatus.phase !== "running") {
      return Promise.reject(new Error("验收会话尚未开始。"));
    }
    browserAcceptanceStatus = {
      ...browserAcceptanceStatus,
      phase: "ended",
      armed: null,
      selfIsolationActive: false
    };
    browserAcceptanceReport = getBrowserAcceptanceReport();
    return browserResult("acceptance-end", browserAcceptanceReport);
  },
  exportAcceptanceDiagnostics: (): Promise<AcceptanceExportReceipt> => {
    if (isTauriRuntime()) {
      return invoke("export_acceptance_diagnostics");
    }
    if (!browserAcceptanceReport) {
      return Promise.reject(new Error("请先结束验收会话。"));
    }
    return browserResult("acceptance-export", {
      path: "/test-data/acceptance/selection-acceptance-report.json",
      recordedRecords: browserAcceptanceReport.recordedRecords,
      valid: browserAcceptanceReport.status.valid
    });
  },
  clearAcceptanceDiagnostics: (): Promise<AcceptanceRuntimeStatus> => {
    if (isTauriRuntime()) {
      return invoke("clear_acceptance_diagnostics");
    }
    browserAcceptanceStatus = {
      ...browserAcceptanceStatus,
      phase: "idle",
      armed: null,
      selfIsolationActive: false,
      pendingGenerations: 0,
      pendingRapidProbes: 0,
      recordedGenerations: 0,
      duplicateGenerationsSuppressed: 0
    };
    browserCompletedAcceptanceArms.clear();
    browserAcceptanceReport = null;
    return browserResult("acceptance-clear", { ...browserAcceptanceStatus });
  },
  openSettings: (): Promise<void> =>
    isTauriRuntime() ? invoke("open_settings") : browserResult("open-settings", undefined),
  openAccessibilitySettings: (): Promise<void> =>
    isTauriRuntime() ? invoke("open_accessibility_settings") : browserResult("open-accessibility", undefined),
  openInputMonitoringSettings: (): Promise<void> =>
    isTauriRuntime() ? invoke("open_input_monitoring_settings") : browserResult("open-input", undefined),
  copyTranslation: (): Promise<void> =>
    isTauriRuntime() ? invoke("copy_translation") : browserResult("copy", undefined),
  copySource: (): Promise<void> =>
    isTauriRuntime() ? invoke("copy_source") : browserResult("copy", undefined),
  copyText: (text: string): Promise<void> => {
    if (isTauriRuntime()) {
      return invoke("copy_text", { text });
    }

    if (browserShouldFail("copy")) {
      return Promise.reject(new Error("固定测试：复制失败。"));
    }
    return navigator.clipboard?.writeText(text) ?? Promise.resolve();
  },
  closePopup: (): Promise<void> => {
    if (isTauriRuntime()) {
      return invoke("close_popup");
    }

    if (browserShouldFail("close")) {
      return Promise.reject(new Error("固定测试：关闭浮窗失败。"));
    }
    browserTranslationToken += 1;
    emitBrowserPopupState({
      ...HIDDEN_POPUP_STATE,
      selectionRevision: browserPopupState.selectionRevision,
      pinned: browserPopupState.pinned
    });
    return Promise.resolve();
  },
  togglePin: (): Promise<void> => {
    if (isTauriRuntime()) {
      return invoke("toggle_pin");
    }

    if (browserShouldFail("pin")) {
      return Promise.reject(new Error("固定测试：固定状态更新失败。"));
    }
    emitBrowserPopupState({ ...browserPopupState, pinned: !browserPopupState.pinned });
    return Promise.resolve();
  },
  translateSelection: (): Promise<void> => {
    if (isTauriRuntime()) {
      return invoke("translate_selection");
    }

    if (browserShouldFail("translate")) {
      return Promise.reject(new Error("固定测试：启动翻译失败。"));
    }
    emitBrowserTranslation(browserSettings.targetLanguage);
    return Promise.resolve();
  },
  retryTranslation: (targetLanguage?: string): Promise<void> => {
    if (isTauriRuntime()) {
      return invoke("retry_translation", { targetLanguage });
    }

    if (browserShouldFail("retry")) {
      return Promise.reject(new Error("固定测试：重新翻译失败。"));
    }
    emitBrowserTranslation(targetLanguage ?? browserSettings.targetLanguage);
    return Promise.resolve();
  },
  explainTerms: (targetLanguage?: string): Promise<void> => {
    if (isTauriRuntime()) {
      return invoke("explain_terms", { targetLanguage });
    }

    if (browserShouldFail("explain")) {
      return Promise.reject(new Error("固定测试：术语解释失败。"));
    }
    if (browserPopupState.visible && browserPopupState.selectionRevision > 0) {
      browserTranslationToken += 1;
      emitBrowserPopupState({
        ...browserPopupState,
        status: "translated",
        targetLanguage,
        translation: "术语说明\n\nTransformer：基于自注意力机制的神经网络架构。\nContext window：模型一次可处理的上下文长度。",
        cached: false
      });
    }
    return Promise.resolve();
  },
  resizePopup: (height: number): Promise<void> => {
    if (isTauriRuntime()) {
      return invoke("resize_popup", { height });
    }
    document.documentElement.dataset.popupHeight = String(height);
    return browserResult("resize", undefined);
  },
  startWindowDrag: (): Promise<void> =>
    isTauriRuntime() ? getCurrentWindow().startDragging() : browserResult("drag", undefined),
  setPopupDragging: (isDragging: boolean): Promise<void> =>
    isTauriRuntime() ? invoke("set_popup_dragging", { isDragging }) : browserResult("drag-state", undefined),
  openExternal: (url: string): Promise<void> => {
    if (isTauriRuntime()) {
      return invoke("open_external", { url });
    }

    if (browserShouldFail("external")) {
      return Promise.reject(new Error("固定测试：外部链接打开失败。"));
    }

    window.open(url, "_blank", "noopener,noreferrer");
    return Promise.resolve();
  },
  getPopupSnapshot: (): Promise<PopupState> =>
    isTauriRuntime() ? invoke("get_popup_snapshot") : Promise.resolve(browserPopupState),
  revealPopupProtocolError: (): Promise<void> =>
    isTauriRuntime() ? getCurrentWindow().show() : Promise.resolve(),
  onPopupState: (
    callback: (state: PopupState) => void,
    onError: (error: Error) => void = () => undefined
  ): (() => void) => {
    let acceptedState: PopupState | null = null;
    let disposed = false;
    const accept = (incoming: PopupState): void => {
      if (disposed) {
        return;
      }
      try {
        const next = reconcilePopupState(acceptedState, incoming);
        if (next !== acceptedState) {
          acceptedState = next;
          callback(next);
        }
      } catch (error) {
        onError(toError(error));
      }
    };

    if (!isTauriRuntime()) {
      browserPopupListeners.add(accept);
      window.queueMicrotask(() => {
        if (browserShouldFail("protocol")) {
          accept({
            ...browserPopupState,
            protocolVersion: 999 as typeof POPUP_PROTOCOL_VERSION
          });
          return;
        }
        accept(browserPopupState);
      });
      return () => {
        disposed = true;
        browserPopupListeners.delete(accept);
      };
    }

    let unlisten: UnlistenFn | null = null;

    void (async () => {
      try {
        const nextUnlisten = await listen<PopupState>("popup-state", (event) => accept(event.payload));
        if (disposed) {
          nextUnlisten();
          return;
        }

        unlisten = nextUnlisten;
        const snapshot = await invoke<PopupState>("get_popup_snapshot");
        if (!disposed) {
          accept(snapshot);
        }
      } catch (error) {
        if (!disposed) {
          onError(toError(error));
        }
      }
    })();

    return () => {
      disposed = true;
      unlisten?.();
    };
  },
  onPopupKeyboardEntry: (
    callback: (entry: PopupKeyboardEntry) => void,
    onError: (error: Error) => void = () => undefined
  ): (() => void) => {
    if (!isTauriRuntime()) {
      const listener = (event: Event): void => {
        const entry = (event as CustomEvent<PopupKeyboardEntry>).detail;
        if (entry) {
          callback(entry);
        }
      };
      window.addEventListener("paper-float-popup-keyboard-entry", listener);
      return () => window.removeEventListener("paper-float-popup-keyboard-entry", listener);
    }

    let disposed = false;
    let unlisten: UnlistenFn | null = null;
    void listen<PopupKeyboardEntry>("popup-keyboard-entry", (event) => {
      if (!disposed) {
        callback(event.payload);
      }
    })
      .then((nextUnlisten) => {
        if (disposed) {
          nextUnlisten();
        } else {
          unlisten = nextUnlisten;
        }
      })
      .catch((error: unknown) => {
        if (!disposed) {
          onError(toError(error));
        }
      });

    return () => {
      disposed = true;
      unlisten?.();
    };
  }
};

function isTauriRuntime(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

function browserShouldFail(operation: string): boolean {
  const requested = new URLSearchParams(window.location.search).get("fail") ?? "";
  const failures = requested
    .split(",")
    .map((value) => value.trim())
    .filter(Boolean);
  if (failures.includes(operation)) {
    return true;
  }
  if (failures.includes(`${operation}-once`)) {
    const attempts = browserFailureAttempts.get(operation) ?? 0;
    browserFailureAttempts.set(operation, attempts + 1);
    return attempts === 0;
  }
  return false;
}

function browserResult<T>(operation: string, value: T): Promise<T> {
  return browserShouldFail(operation)
    ? Promise.reject(new Error(`固定测试：${operation} 操作失败。`))
    : Promise.resolve(value);
}

function browserDelay(operation: string): Promise<void> {
  const requested = new URLSearchParams(window.location.search).get("delay") ?? "";
  const delayed = requested
    .split(",")
    .map((value) => value.trim())
    .includes(operation);
  return delayed
    ? new Promise((resolve) => window.setTimeout(resolve, 350))
    : Promise.resolve();
}

function browserAcceptanceArmKey(scenario: AcceptanceScenario, ordinal: number): string {
  return `${scenario}:${ordinal}`;
}

function getBrowserSettingsPayload(): SettingsPayload {
  const runtimePlatform = browserRuntimePlatform;
  const payload: SettingsPayload = {
    settings: browserSettings,
    hasApiKey: browserHasApiKey,
    apiKeyStatus: browserHasApiKey ? "configured" : "missing",
    apiKeyStorage:
      runtimePlatform === "windows" ? "windows_credential_manager" : "system_keychain",
    runtimePlatform,
    doubleCopyStatus: {
      available: true,
      running: true,
      message:
        runtimePlatform === "windows"
          ? "Ctrl+C+C 双复制取词已就绪；只确认用户主动执行的复制。"
          : "Cmd+C+C 监听已启用。",
      code: runtimePlatform === "windows" ? "windows_double_copy_ready" : "ready"
    },
    selectionStatus: {
      available: true,
      running: true,
      message:
        runtimePlatform === "windows"
          ? browserSettings.enableAutomaticSelection
            ? "Windows 鼠标划词与快捷键取词已就绪；快捷键为 Ctrl+Alt+T。"
            : "Windows 快捷键取词已就绪：选中文字后按 Ctrl+Alt+T。"
          : "鼠标选区监听、AX selectedText 通知与直接读取均已启用。",
      code:
        runtimePlatform === "windows"
          ? browserSettings.enableAutomaticSelection
            ? "windows_auto_selection_ready"
            : "windows_uia_shortcut_ready"
          : "selection_sources_ready"
    },
    capabilitySnapshot: runtimePlatform === "windows" ? {
      accessibility: {
        grant: "granted",
        health: "ready",
        statusCode: "windows_uia_no_consent_required"
      },
      listenEvent: {
        grant: "granted",
        health: "ready",
        statusCode: "windows_raw_input_no_consent_required"
      },
      mouseTap: browserSettings.enableAutomaticSelection
        ? { health: "ready", statusCode: "windows_raw_mouse_ready" }
        : { health: "disabled", statusCode: "windows_auto_selection_disabled_by_setting" },
      keyTap: { health: "ready", statusCode: "windows_raw_input_ready" },
      axSelectedTextObserver: browserSettings.enableAutomaticSelection
        ? { health: "ready", statusCode: "windows_uia_observer_ready" }
        : { health: "disabled", statusCode: "windows_auto_selection_disabled_by_setting" },
      directSelectionRead: { health: "unknown", statusCode: "windows_uia_not_probed" },
      clipboardDoubleCopyFallback: { health: "ready", statusCode: "windows_double_copy_ready" }
    } : {
      accessibility: {
        grant: "granted",
        health: "ready",
        statusCode: "accessibility_preflight_granted"
      },
      listenEvent: {
        grant: "granted",
        health: "ready",
        statusCode: "listen_event_preflight_granted"
      },
      mouseTap: { health: "ready", statusCode: "selection_sources_ready" },
      keyTap: { health: "ready", statusCode: "ready" },
      axSelectedTextObserver: { health: "ready", statusCode: "selection_sources_ready" },
      directSelectionRead: { health: "ready", statusCode: "selection_read_found" },
      clipboardDoubleCopyFallback: { health: "disabled", statusCode: "inactive_key_tap_ready" }
    },
    permissionDiagnostics: runtimePlatform === "windows" ? undefined : {
      bundleIdentifier: "com.paperfloat.translator",
      appVersion: "0.1.0-test",
      bundlePath: "/Applications/Paper Float Translator.app",
      executablePath: "/Applications/Paper Float Translator.app/Contents/MacOS/paper-float-translator",
      signatureKind: "adhoc",
      accessibilityTrusted: true,
      selectionRead: {
        status: "selection_read_found",
        reason: "mouse_up",
        foundText: true,
        candidateCount: 2,
        durationMs: 3.4,
        sourceBundleId: "com.paperfloat.browser-preview",
        axError: "none"
      }
    }
  };

  if (!browserSettings.enableSelectionPopup) {
    payload.selectionStatus = {
      available: true,
      running: false,
      code: "selection_disabled_by_setting",
      message: "自动划词已按设置停用；不会安装鼠标或辅助功能选区监听。"
    };
    payload.capabilitySnapshot.mouseTap = {
      health: "disabled",
      statusCode: "selection_disabled_by_setting"
    };
    payload.capabilitySnapshot.axSelectedTextObserver = {
      health: "disabled",
      statusCode: "selection_disabled_by_setting"
    };
    payload.capabilitySnapshot.directSelectionRead = {
      health: "disabled",
      statusCode: "selection_disabled_by_setting"
    };
    if (runtimePlatform === "windows") {
      payload.doubleCopyStatus = {
        available: true,
        running: false,
        code: "selection_disabled",
        message: "Ctrl+C+C 双复制取词已在设置中关闭。"
      };
      payload.capabilitySnapshot.keyTap = {
        health: "disabled",
        statusCode: "selection_disabled"
      };
      payload.capabilitySnapshot.clipboardDoubleCopyFallback = {
        health: "disabled",
        statusCode: "selection_disabled"
      };
    }
    payload.permissionDiagnostics = payload.permissionDiagnostics
      ? { ...payload.permissionDiagnostics, selectionRead: undefined }
      : undefined;

    const runtimeFixture = new URLSearchParams(window.location.search).get("runtime");
    if (runtimeFixture === "disable-unconfirmed") {
      payload.selectionStatus = {
        available: true,
        running: false,
        code: "selection_disable_unconfirmed",
        message: "自动划词设置已关闭，但无法确认所有原生选区资源均已释放；请退出应用后重新打开。"
      };
      payload.capabilitySnapshot.mouseTap = {
        health: "unknown",
        statusCode: "selection_disable_unconfirmed"
      };
      payload.capabilitySnapshot.axSelectedTextObserver = {
        health: "unknown",
        statusCode: "selection_disable_unconfirmed"
      };
      payload.capabilitySnapshot.directSelectionRead = {
        health: "unknown",
        statusCode: "selection_disable_unconfirmed"
      };
    } else if (runtimeFixture === "disable-fail-closed") {
      payload.doubleCopyStatus = {
        available: true,
        running: false,
        code: "selection_disable_fail_closed",
        message: "为确保自动划词完全停用，Cmd+C+C 监听也已暂时停止；请刷新监听诊断以恢复。"
      };
      payload.capabilitySnapshot.keyTap = {
        health: "degraded",
        statusCode: "selection_disable_fail_closed"
      };
      payload.capabilitySnapshot.clipboardDoubleCopyFallback = {
        health: "degraded",
        statusCode: "selection_disable_fail_closed"
      };
    }
  }

  if (new URLSearchParams(window.location.search).get("runtime") === "observer-limited") {
    payload.selectionStatus = {
      available: true,
      running: true,
      code: "selection_mouse_ready_ax_observer_limited",
      message: "鼠标选区与 AX 直接读取可用；当前前台应用未注册 selectedText 通知。"
    };
    payload.capabilitySnapshot.mouseTap = {
      health: "ready",
      statusCode: "selection_mouse_ready_ax_observer_limited"
    };
    payload.capabilitySnapshot.axSelectedTextObserver = {
      health: "degraded",
      statusCode: "selection_mouse_ready_ax_observer_limited"
    };
    payload.capabilitySnapshot.directSelectionRead = {
      health: "ready",
      statusCode: "selection_mouse_ready_ax_observer_limited"
    };
  }

  const permissionFixture = new URLSearchParams(window.location.search).get("permission");
  if (permissionFixture === "accessibility-denied" || permissionFixture === "both-denied") {
    payload.capabilitySnapshot.accessibility = {
      grant: "denied",
      health: "unavailable",
      statusCode: "accessibility_permission_denied"
    };
    payload.capabilitySnapshot.axSelectedTextObserver = {
      health: "unavailable",
      statusCode: "accessibility_permission_denied"
    };
    payload.capabilitySnapshot.directSelectionRead = {
      health: "unavailable",
      statusCode: "accessibility_permission_denied"
    };
    payload.selectionStatus = {
      available: true,
      running: false,
      code: "accessibility_permission_denied",
      message: "需要辅助功能权限才能读取所选文本。"
    };
    if (payload.permissionDiagnostics) {
      payload.permissionDiagnostics.accessibilityTrusted = false;
    }
  }

  if (permissionFixture === "input-denied" || permissionFixture === "both-denied") {
    payload.capabilitySnapshot.listenEvent = {
      grant: "denied",
      health: "unavailable",
      statusCode: "input_monitoring_permission_denied"
    };
    payload.capabilitySnapshot.mouseTap = {
      health: "unavailable",
      statusCode: "input_monitoring_permission_denied"
    };
    payload.capabilitySnapshot.keyTap = {
      health: "unavailable",
      statusCode: "input_monitoring_permission_denied"
    };
    payload.capabilitySnapshot.clipboardDoubleCopyFallback = {
      health: "ready",
      statusCode: "pasteboard_fallback_ready"
    };
    payload.selectionStatus = {
      available: true,
      running: payload.capabilitySnapshot.axSelectedTextObserver.health === "ready",
      code: "selection_input_monitoring_limited",
      message: "输入监听未授权；AX selectedText 通知仍可作为受限路径。"
    };
  }

  if (permissionFixture === "runtime-degraded") {
    payload.capabilitySnapshot.mouseTap = {
      health: "degraded",
      statusCode: "selection_mouse_tap_disabled_timeout"
    };
    payload.capabilitySnapshot.axSelectedTextObserver = {
      health: "degraded",
      statusCode: "selection_ax_observer_restarting"
    };
    payload.selectionStatus = {
      available: true,
      running: false,
      code: "selection_runtime_degraded",
      message: "权限已授权，但运行时监听正在恢复。"
    };
  }

  if (new URLSearchParams(window.location.search).get("runtime") === "keychain-unavailable") {
    payload.hasApiKey = false;
    payload.apiKeyStatus = "unavailable";
    payload.runtimeWarning =
      "当前构建无法在后台安全读取系统钥匙串中的 API Key。若保存后仍显示“暂不可读”，需改用稳定签名构建。";
  }

  return payload;
}

function getBrowserAcceptanceReport(): AcceptanceReport {
  const selectionExpectations: Array<[AcceptanceScenario, number]> = [
    ["textEditDrag", 30],
    ["textEditDoubleClick", 30],
    ["textEditTripleClick", 30],
    ["textEditShiftExtend", 30],
    ["textEditKeyboardSelection", 30],
    ["sameTextSameLocation", 30],
    ["sameTextDifferentLocation", 30],
    ["safariFixture", 1],
    ["previewFixture", 1]
  ];
  const missingSelections = selectionExpectations.flatMap(([scenario, expected]) =>
    Array.from({ length: expected }, (_, index) => ({ scenario, ordinal: index + 1 })).filter(
      (entry) => entry.scenario !== "sameTextSameLocation" || entry.ordinal !== 1
    )
  );
  const sequence = (maximum: number): number[] =>
    Array.from({ length: maximum }, (_, index) => index + 1);
  const emptyWatcherResources = (): AcceptanceWatcherResourceCounts => ({
    snapshotVersion: 0,
    selectionObservers: 0,
    selectionObserverSources: 0,
    keyEventTaps: 0,
    keyEventTapSources: 0,
    mouseEventTaps: 0,
    mouseEventTapSources: 0,
    pasteboardTimers: 0,
    workspaceActivationObservers: 0,
    callbacks: 0,
    contexts: 0,
    effectiveSourceSets: 0,
    totalSources: 0
  });

  return {
    schemaVersion: 1,
    reportVersion: 4,
    clock: "monotonic",
    app: {
      version: "0.1.0-browser-fixture",
      bundleIdentifier: "com.paperfloat.translator.acceptance",
      buildKind: "adHocAcceptance"
    },
    system: {
      operatingSystem: "macOs",
      version: "browser-fixture",
      architecture: "arm64"
    },
    baselineApplications: [
      { application: "textEdit", version: "browser-fixture" },
      { application: "safari", version: "browser-fixture" },
      { application: "preview", version: "browser-fixture" }
    ],
    fixtures: [
      { kind: "textEditPlainText", fixtureId: "selection-baseline.txt", sha256: "0".repeat(64) },
      { kind: "safariHtml", fixtureId: "selection-baseline.html", sha256: "0".repeat(64) },
      { kind: "previewPdf", fixtureId: "selection-baseline.pdf", sha256: "0".repeat(64) }
    ],
    multiClickQuietWindowMs: 530,
    capacity: 100_000,
    recordedRecords: 2,
    overflowedRecords: 0,
    rejectedRecords: 0,
    status: {
      integrityValid: true,
      acceptancePassed: false,
      valid: false,
      invalidReasons: [
        "missingSelection",
        "wrongClassification",
        "latencyThresholdExceeded",
        "rapidTransitionFailure",
        "tapRecoveryFailure",
        "watcherResourceFailure",
        "selfIsolationFailure"
      ]
    },
    summary: {
      totalRecords: 2,
      selectionRecords: 1,
      uniqueSelectionRevisions: 1,
      uniquePopupRevisions: 1,
      duplicateSelectionRevisions: [],
      duplicatePopupRevisions: [],
      duplicateEvidence: {
        nativeReadReplays: 0,
        popupOutcomeReplays: 0,
        tapStatusReplays: 0,
        watcherPhaseReplays: 0,
        rapidProbeReplays: 0
      },
      selections: {
        expectedCount: 212,
        popupCommitCount: 1,
        missing: missingSelections,
        duplicates: [],
        unexpected: [],
        wrongClassification: [{ scenario: "sameTextSameLocation", ordinal: 1 }],
        staleWriteCommittedCount: 0
      },
      latency: { sampleCount: 1, p95Ms: 420, maxMs: 420 },
      scenarioLatency: selectionExpectations.map(([scenario]) => {
        const isObservedFixture = scenario === "sameTextSameLocation";
        const threshold = scenario === "textEditDoubleClick"
          ? { p95LimitMs: 880, maxLimitMs: 1_330 }
          : scenario === "safariFixture" || scenario === "previewFixture"
            ? { p95LimitMs: 1_200, maxLimitMs: 1_200 }
            : { p95LimitMs: 350, maxLimitMs: 800 };
        return {
          scenario,
          latency: {
            sampleCount: isObservedFixture ? 1 : 0,
            p95Ms: isObservedFixture ? 420 : null,
            maxMs: isObservedFixture ? 420 : null
          },
          threshold,
          thresholdPassed: false
        };
      }),
      rapidAToB: {
        expectedPairs: 20,
        maxTriggerGapMs: 300,
        finalBPairs: 0,
        missingPairs: sequence(20),
        missingAPairs: sequence(20),
        missingBPairs: sequence(20),
        duplicateAPairs: [],
        duplicateBPairs: [],
        outOfOrderPairs: [],
        gapExceededPairs: [],
        wrongFinalPairs: [],
        unexpectedPairs: [],
        unexpectedEvidencePairs: [],
        unexpectedEvidenceCount: 0,
        staleARejectedCount: 0,
        missingStaleRejectionPairs: sequence(20),
        duplicateStaleRejectionPairs: [],
        invalidStaleRejectionPairs: [],
        staleAWriteCount: 0,
        passed: false
      },
      tapRecovery: {
        expectedAttempts: 1,
        resolutionLimitMs: 2_000,
        readyWithinLimit: 0,
        degradedWithinLimit: 0,
        late: [],
        unresolved: [1],
        unexpectedAttempts: [],
        duplicateDisabledEvents: 0,
        duplicateResolutionEvents: 0,
        maxResolutionMs: null,
        passed: false
      },
      watcherResources: {
        expectedRestarts: 50,
        sampleCount: 0,
        missingRestarts: sequence(50),
        missingReleaseRestarts: sequence(50),
        missingStartedRestarts: sequence(50),
        unexpectedRestarts: [],
        duplicateReleaseRestarts: [],
        duplicateStartedRestarts: [],
        duplicateActiveRestarts: [],
        generationMismatchRestarts: [],
        outOfOrderRestarts: [],
        releasedResourceLeaks: [],
        peak: emptyWatcherResources(),
        finalActive: null,
        growthOrDuplicateDetected: false,
        finalSingleEffectiveSource: false,
        passed: false
      },
      selfIsolation: {
        configuredMinDurationMs: 300_000,
        observedDurationMs: 120_000,
        selectionTriggerCount: 0,
        startCount: 1,
        endCount: 1,
        lifecycleGenerationMatched: true,
        lifecycleOrderValid: true,
        interactionRecordCount: 1,
        interactionGenerationMatched: true,
        totalInteractionCount: 7,
        settingsInteractionCount: 5,
        popupInteractionCount: 2,
        pointerInteractionCount: 4,
        keyboardInteractionCount: 3,
        populatedBuckets: [0, 1],
        settingsBuckets: [0, 1],
        popupBuckets: [0, 1],
        everyBucketCovered: false,
        windowBucketCoverageValid: false,
        passed: false
      }
    },
    records: [
      {
        recordType: "selection",
        scenario: "sameTextSameLocation",
        ordinal: 1,
        generation: 1,
        selectionRevision: 1,
        popupRevision: 1,
        controllerSnapshotSelectionRevision: 1,
        controllerSnapshotPopupRevision: 1,
        attempt: 1,
        reason: "mouseDrag",
        sourceApp: { kind: "textEdit" },
        eventOffsetMicros: 100_000,
        controllerCommitOffsetMicros: 105_000,
        endToEndLatencyMs: 420,
        outcome: "popupCommitted",
        expectedMatch: "match",
        locationEvidence: {
          category: "referenceA",
          result: "mismatchedExpectedCluster"
        }
      },
      {
        recordType: "selfInteraction",
        scenario: "selfWindowIsolation",
        ordinal: 1,
        generation: 2,
        aggregates: [
          { window: "settings", kind: "pointer", count: 3, bucketCoverage: [0, 1] },
          { window: "settings", kind: "keyboard", count: 2, bucketCoverage: [0] },
          { window: "popup", kind: "pointer", count: 1, bucketCoverage: [0] },
          { window: "popup", kind: "keyboard", count: 1, bucketCoverage: [1] }
        ]
      }
    ]
  };
}

function getInitialBrowserPopupState(): PopupState {
  const params = new URLSearchParams(window.location.search);
  const requestedState = params.get("state");
  const fixtureName = params.get("fixture") ?? "default";
  const longFixture = fixtureName === "long";
  const fixtureText = getBrowserTextFixture(fixtureName);
  const fixtureTranslation = getBrowserTranslationFixture(fixtureName);

  if (requestedState === "hidden") {
    return {
      ...HIDDEN_POPUP_STATE,
      revision: 1
    };
  }

  if (requestedState === "pending") {
    return {
      protocolVersion: POPUP_PROTOCOL_VERSION,
      revision: 1,
      selectionRevision: 1,
      visible: true,
      status: "selection_pending",
      selectedText: fixtureText,
      pinned: false
    };
  }

  if (requestedState === "selection") {
    return {
      protocolVersion: POPUP_PROTOCOL_VERSION,
      revision: 1,
      selectionRevision: 1,
      visible: true,
      status: "selection_ready",
      selectedText: fixtureText,
      cleanedText: fixtureText,
      pinned: false
    };
  }

  if (requestedState === "loading") {
    return {
      protocolVersion: POPUP_PROTOCOL_VERSION,
      revision: 1,
      selectionRevision: 1,
      visible: true,
      status: "translating",
      cleanedText: fixtureText,
      translation: params.get("streaming") === "1" ? fixtureTranslation : undefined,
      targetLanguage: DEFAULT_SETTINGS.targetLanguage,
      pinned: false
    };
  }

  if (requestedState === "error") {
    const requestedError = params.get("error");
    const errorFixture = requestedError === "configuration"
      ? {
          error: "请先在设置页保存 DeepSeek API Key。",
          errorKind: "configuration" as const,
          retryable: false,
          recoveryAction: "open_settings" as const
        }
      : requestedError === "permission-input"
        ? {
            error: "输入监听权限尚未生效。",
            errorKind: "permission" as const,
            retryable: false,
            recoveryAction: "open_input_monitoring" as const
          }
        : requestedError === "permission-accessibility"
          ? {
              error: "辅助功能权限尚未生效。",
              errorKind: "permission" as const,
              retryable: false,
              recoveryAction: "open_accessibility" as const
            }
          : {
              error: longFixture
                ? "翻译服务暂时不可用。请检查网络连接与 API Key 后重试；当前选区已保留，不需要重新划词。这是一条用于验证长错误信息滚动和控件可达性的固定测试文案。"
                : "翻译服务暂时不可用。请检查网络连接与 API Key 后重试。",
              errorKind: "translation" as const,
              retryable: true,
              recoveryAction: "retry_translation" as const
            };
    return {
      protocolVersion: POPUP_PROTOCOL_VERSION,
      revision: 1,
      selectionRevision: 1,
      visible: true,
      status: "error",
      sourceText: fixtureText,
      cleanedText: fixtureText,
      ...errorFixture,
      targetLanguage: DEFAULT_SETTINGS.targetLanguage,
      pinned: false
    };
  }

  return {
    protocolVersion: POPUP_PROTOCOL_VERSION,
    revision: 1,
    selectionRevision: 1,
    visible: true,
    status: "translated",
    sourceText: fixtureText,
    cleanedText: fixtureText,
    translation:
      longFixture
        ? "Transformer 架构已经成为长上下文语言建模的默认骨干。它通过自注意力机制在序列内部建立依赖关系，使模型能够在保持表达能力的同时更稳定地处理较长文本。这个固定长译文会重复覆盖多行内容，用来验证主体区域能够滚动，而顶部关闭按钮和底部操作区始终保持可见、可点击，并且文本不会横向溢出窗口边界。"
        : fixtureTranslation,
    cached: true,
    targetLanguage: DEFAULT_SETTINGS.targetLanguage,
    pinned: false
  };
}

function getBrowserTextFixture(name: string): string {
  switch (name) {
    case "long":
      return "Transformer architectures have become the default backbone for long-context language modeling. This deliberately long browser fixture verifies that source previews, translated content, actions, and error messages remain bounded and scrollable without hiding the close or retry controls.";
    case "unbroken":
      return `https://example.invalid/${"selection".repeat(48)}`;
    case "cjk":
      return "中文选区不使用空格，也必须在窄窗口中正确换行并保持所有操作按钮可见。";
    case "emoji":
      return "👩‍🔬🧑🏽‍💻📚 研究者正在验证 emoji、组合字符与长文本预览。";
    case "multiline":
      return "first line\nsecond line\nthird line";
    case "rtl":
      return "هذا نص عربي لاختبار اتجاه النص داخل نافذة الترجمة العائمة";
    default:
      return "Transformer architectures have become the default backbone for long-context language modeling.";
  }
}

function getBrowserTranslationFixture(name: string): string {
  switch (name) {
    case "unbroken":
      return "译文".repeat(240);
    case "cjk":
      return "这是用于验证中文长文本换行和操作区可达性的固定译文。";
    case "emoji":
      return "👩‍🔬 研究者完成了 🧑🏽‍💻 界面验证。";
    case "multiline":
      return "第一行\n第二行\n第三行";
    case "rtl":
      return "هذا نص مترجم لاختبار العرض من اليمين إلى اليسار";
    default:
      return "Transformer 架构已经成为长上下文语言建模的默认骨干。它通过自注意力机制在序列内部建立依赖关系，使模型能够在保持表达能力的同时更稳定地处理较长文本。";
  }
}

function emitBrowserPopupState(nextState: PopupStateUpdate): void {
  browserPopupState = {
    ...nextState,
    protocolVersion: POPUP_PROTOCOL_VERSION,
    revision: browserPopupState.revision + 1,
    selectionRevision: nextState.selectionRevision ?? browserPopupState.selectionRevision,
    visible: nextState.status !== "hidden",
    pinned: nextState.pinned ?? browserPopupState.pinned
  };
  browserSelectionRevision = Math.max(browserSelectionRevision, browserPopupState.selectionRevision);
  browserPopupListeners.forEach((listener) => listener(browserPopupState));
}

function emitBrowserTranslation(targetLanguage?: string): void {
  if (!browserPopupState.visible || browserPopupState.selectionRevision === 0) {
    return;
  }
  const sourceText =
    browserPopupState.cleanedText ??
    browserPopupState.sourceText ??
    "Transformer architectures have become the default backbone for long-context language modeling.";

  const translationToken = ++browserTranslationToken;
  const selectionRevision =
    browserPopupState.selectionRevision > 0 ? browserPopupState.selectionRevision : ++browserSelectionRevision;

  emitBrowserPopupState({
    status: "translating",
    selectionRevision,
    sourceText,
    cleanedText: sourceText,
    targetLanguage,
    pinned: browserPopupState.pinned
  });

  window.setTimeout(() => {
    if (
      translationToken !== browserTranslationToken ||
      browserPopupState.selectionRevision !== selectionRevision ||
      !browserPopupState.visible
    ) {
      return;
    }
    emitBrowserPopupState({
      status: "translated",
      selectionRevision,
      sourceText,
      cleanedText: sourceText,
      translation:
        "Transformer 架构已经成为长上下文语言建模的默认骨干。它通过自注意力机制在序列内部建立依赖关系，使模型能够在保持表达能力的同时更稳定地处理较长文本。",
      cached: false,
      targetLanguage,
      pinned: browserPopupState.pinned
    });
  }, 420);
}

function toError(error: unknown): Error {
  return error instanceof Error ? error : new Error(String(error));
}

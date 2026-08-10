import { type ReactNode, useEffect, useRef, useState } from "react";
import {
  AlertCircle,
  Check,
  Copy,
  Database,
  Eraser,
  ExternalLink,
  KeyRound,
  Languages,
  Loader2,
  MousePointer2,
  RefreshCcw,
  RotateCcw,
  Save,
  Settings
} from "lucide-react";
import {
  DEFAULT_SETTINGS,
  UNKNOWN_CAPABILITY_SNAPSHOT,
  getCapabilityPermissionActions,
  getDoubleCopyCapabilityPresentation,
  getSelectionCapabilityPresentation,
  type AppSettings,
  type CapabilitySnapshot,
  type DeepSeekModel,
  type TranslateMode,
  type WatcherStatus
} from "@paper-float-translator/core/renderer";
import {
  desktopApi,
  type ApiKeyStatus,
  type ApiKeyStorage,
  type PermissionDiagnostics,
  type SettingsPayload
} from "./desktopApi";
import { AcceptancePanel } from "./AcceptancePanel";
import { hasUnsavedSettingsChanges } from "./settingsDraft";
import { getSettingsRuntimeWarning } from "./settingsRuntimeWarning";

const MODEL_OPTIONS: Array<{ value: DeepSeekModel; label: string }> = [
  { value: "deepseek-v4-flash", label: "DeepSeek V4 Flash" },
  { value: "deepseek-v4-pro", label: "DeepSeek V4 Pro" }
];

const MODE_OPTIONS: Array<{ value: TranslateMode; label: string }> = [
  { value: "academic_zh", label: "学术翻译" },
  { value: "bilingual", label: "双语对照" },
  { value: "terminology", label: "解释术语" }
];

const LANGUAGE_OPTIONS = ["中文", "英文", "日文", "韩文", "法文", "德文", "西班牙文"];
const CUSTOM_LANGUAGE_VALUE = "__custom__";

export function SettingsView(): JSX.Element {
  const [savedSettings, setSavedSettings] = useState<AppSettings>(DEFAULT_SETTINGS);
  const [draftSettings, setDraftSettings] = useState<AppSettings>(DEFAULT_SETTINGS);
  const [popupWidthInput, setPopupWidthInput] = useState(String(DEFAULT_SETTINGS.popupWidth));
  const [apiKey, setApiKey] = useState("");
  const [apiKeyStatus, setApiKeyStatus] = useState<ApiKeyStatus>("missing");
  const [apiKeyStorage, setApiKeyStorage] = useState<ApiKeyStorage>("system_keychain");
  const hasApiKey = apiKeyStatus === "configured";
  const [doubleCopyStatus, setDoubleCopyStatus] = useState<WatcherStatus>({
    available: true,
    running: false,
    message: ""
  });
  const [selectionStatus, setSelectionStatus] = useState<WatcherStatus>({
    available: true,
    running: false,
    message: ""
  });
  const [capabilitySnapshot, setCapabilitySnapshot] = useState<CapabilitySnapshot>(UNKNOWN_CAPABILITY_SNAPSHOT);
  const [permissionDiagnostics, setPermissionDiagnostics] = useState<PermissionDiagnostics | undefined>();
  const [loading, setLoading] = useState(true);
  const [loadAttempt, setLoadAttempt] = useState(0);
  const [initialLoadError, setInitialLoadError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [refreshingWatchers, setRefreshingWatchers] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const permissionRefreshArmed = useRef(false);
  const editorSnapshot = useRef({ savedSettings, draftSettings, popupWidthInput, apiKey });
  editorSnapshot.current = { savedSettings, draftSettings, popupWidthInput, apiKey };

  function applyRuntimeStatus(payload: SettingsPayload): void {
    setApiKeyStatus(payload.apiKeyStatus);
    setApiKeyStorage(payload.apiKeyStorage);
    setDoubleCopyStatus(payload.doubleCopyStatus);
    setSelectionStatus(payload.selectionStatus);
    setCapabilitySnapshot(payload.capabilitySnapshot);
    setPermissionDiagnostics(payload.permissionDiagnostics);
  }

  function applyAuthoritativePayload(
    payload: SettingsPayload,
    preserveDirtyDraft = true
  ): boolean {
    const previous = editorSnapshot.current;
    const draftWasDirty = hasUnsavedSettingsChanges(
      previous.savedSettings,
      previous.draftSettings,
      previous.popupWidthInput,
      previous.apiKey
    );
    const externalSettingsChanged = !appSettingsEqual(previous.savedSettings, payload.settings);
    const preserveDraft = preserveDirtyDraft && draftWasDirty;
    const nextDraft = preserveDraft ? previous.draftSettings : payload.settings;
    const nextWidth = preserveDraft ? previous.popupWidthInput : String(payload.settings.popupWidth);

    setSavedSettings(payload.settings);
    setDraftSettings(nextDraft);
    setPopupWidthInput(nextWidth);
    editorSnapshot.current = {
      savedSettings: payload.settings,
      draftSettings: nextDraft,
      popupWidthInput: nextWidth,
      apiKey: previous.apiKey
    };
    applyRuntimeStatus(payload);
    return preserveDraft && externalSettingsChanged;
  }

  function applyAcceptanceRuntimeStatus(payload: SettingsPayload): void {
    const conflict = applyAuthoritativePayload(payload);
    if (conflict) {
      setNotice(null);
      setError("验收刷新已应用外部设置；你的本地草稿已保留，请确认后再保存。 ");
    }
  }

  useEffect(() => {
    const reportTrustedSelfInteraction = (
      event: Event,
      eventKind: "pointer" | "keyboard"
    ): void => {
      if (event.isTrusted !== true) {
        return;
      }
      void desktopApi.recordAcceptanceSelfInteraction(eventKind).catch(() => undefined);
    };
    const handlePointerDown = (event: Event): void => {
      reportTrustedSelfInteraction(event, "pointer");
    };
    const handleKeyDown = (event: Event): void => {
      reportTrustedSelfInteraction(event, "keyboard");
    };

    window.addEventListener("pointerdown", handlePointerDown, true);
    window.addEventListener("keydown", handleKeyDown, true);
    return () => {
      window.removeEventListener("pointerdown", handlePointerDown, true);
      window.removeEventListener("keydown", handleKeyDown, true);
    };
  }, []);

  useEffect(() => {
    let alive = true;

    setLoading(true);
    setInitialLoadError(null);

    desktopApi
      .getSettings()
      .then((payload) => {
        if (!alive) {
          return;
        }

        applyAuthoritativePayload(payload, false);
        const runtimeWarning = getSettingsRuntimeWarning(payload);
        setError(runtimeWarning ? `设置已读取，但${runtimeWarning}` : null);
      })
      .catch((loadError: unknown) => {
        if (alive) {
          setInitialLoadError(loadError instanceof Error ? loadError.message : "读取设置失败。");
        }
      })
      .finally(() => {
        if (alive) {
          setLoading(false);
        }
      });

    return () => {
      alive = false;
    };
  }, [loadAttempt]);

  useEffect(() => {
    let refreshInFlight = false;

    const refreshAfterPermissionReturn = (): void => {
      if (
        !permissionRefreshArmed.current ||
        refreshInFlight ||
        document.visibilityState !== "visible"
      ) {
        return;
      }

      permissionRefreshArmed.current = false;
      refreshInFlight = true;
      setRefreshingWatchers(true);
      setNotice(null);
      setError(null);

      void desktopApi
        .refreshWatcherStatus()
        .then((refreshed) => {
          const conflict = applyAuthoritativePayload(refreshed);
          const runtimeWarning = getSettingsRuntimeWarning(refreshed);
          if (conflict) {
            setError(
              `权限与监听状态已刷新，外部设置已经生效；你的本地草稿已保留，请确认后再保存。${runtimeWarning ? ` ${runtimeWarning}` : ""}`
            );
          } else if (runtimeWarning) {
            setError(`权限与监听状态已刷新，但${runtimeWarning}`);
          } else {
            setNotice("已自动刷新权限与监听状态。");
          }
        })
        .catch((refreshError: unknown) => {
          setError(refreshError instanceof Error ? refreshError.message : "自动刷新权限状态失败。");
        })
        .finally(() => {
          refreshInFlight = false;
          setRefreshingWatchers(false);
        });
    };

    const handleVisibilityChange = (): void => {
      if (document.visibilityState === "visible") {
        refreshAfterPermissionReturn();
      }
    };

    window.addEventListener("focus", refreshAfterPermissionReturn);
    document.addEventListener("visibilitychange", handleVisibilityChange);

    return () => {
      window.removeEventListener("focus", refreshAfterPermissionReturn);
      document.removeEventListener("visibilitychange", handleVisibilityChange);
    };
  }, []);

  async function handleSave(): Promise<void> {
    const popupWidth = parsePopupWidthInput(popupWidthInput);

    if (popupWidth === null) {
      setNotice(null);
      setError("浮窗宽度请输入 320 到 640 之间的数字。");
      return;
    }

    setSaving(true);
    setNotice(null);
    setError(null);

    let committedPayload: SettingsPayload;
    try {
      committedPayload = await desktopApi.saveSettings({
        ...draftSettings,
        popupWidth
      });
      applyAuthoritativePayload(committedPayload, false);
    } catch (saveError) {
      setError(`设置未保存：${formatErrorMessage(saveError, "写入设置失败。")}`);
      setSaving(false);
      return;
    }

    let runtimeWarning = getSettingsRuntimeWarning(committedPayload);
    if (apiKey.trim()) {
      try {
        committedPayload = await desktopApi.saveApiKey(apiKey);
        applyAuthoritativePayload(committedPayload, false);
        runtimeWarning = getSettingsRuntimeWarning(committedPayload);
        setApiKey("");
      } catch (keyError) {
        setError(
          `设置已保存，但 API Key 保存失败：${formatErrorMessage(keyError, "写入 Key 失败。")} ${
            runtimeWarning ? `同时，${runtimeWarningDetail(runtimeWarning)}` : ""
          }`.trim()
        );
        setSaving(false);
        return;
      }
    }

    if (runtimeWarning) {
      setError(formatSavedRuntimeWarning(runtimeWarning));
    } else {
      setNotice("设置已保存并已应用。");
    }
    setSaving(false);
  }

  async function handleClearApiKey(): Promise<void> {
    setSaving(true);
    setNotice(null);
    setError(null);

    try {
      const result = await desktopApi.saveApiKey("");
      applyAuthoritativePayload(result, false);
      setApiKey("");
      const runtimeWarning = getSettingsRuntimeWarning(result);
      if (runtimeWarning) {
        setError(`API Key 已清除，但${runtimeWarning}`);
      } else {
        setNotice("API Key 已清除。");
      }
    } catch (clearError) {
      setError(clearError instanceof Error ? clearError.message : "清除 API Key 失败。");
    } finally {
      setSaving(false);
    }
  }

  async function handleClearCache(): Promise<void> {
    setSaving(true);
    setNotice(null);
    setError(null);

    try {
      await desktopApi.clearCache();
      setNotice("缓存已清空。");
    } catch (clearError) {
      setError(clearError instanceof Error ? clearError.message : "清空缓存失败。");
    } finally {
      setSaving(false);
    }
  }

  async function handleRefreshWatcherStatus(): Promise<void> {
    setRefreshingWatchers(true);
    setNotice(null);
    setError(null);

    try {
      const refreshed = await desktopApi.refreshWatcherStatus();
      const conflict = applyAuthoritativePayload(refreshed);
      const runtimeWarning = getSettingsRuntimeWarning(refreshed);
      if (conflict) {
        setError(
          `诊断已刷新，外部设置已经生效；你的本地草稿已保留，请确认后再保存。${runtimeWarning ? ` ${runtimeWarning}` : ""}`
        );
      } else if (runtimeWarning) {
        setError(`诊断已刷新，但${runtimeWarning}`);
      } else {
        setNotice("诊断已刷新。");
      }
    } catch (refreshError) {
      setError(refreshError instanceof Error ? refreshError.message : "刷新诊断失败。");
    } finally {
      setRefreshingWatchers(false);
    }
  }

  async function handleOpenPermissionSettings(kind: "accessibility" | "input_monitoring"): Promise<void> {
    setNotice(null);
    setError(null);
    permissionRefreshArmed.current = true;

    try {
      if (kind === "accessibility") {
        await desktopApi.openAccessibilitySettings();
      } else {
        await desktopApi.openInputMonitoringSettings();
      }
    } catch (openError) {
      permissionRefreshArmed.current = false;
      setError(openError instanceof Error ? openError.message : "打开系统权限设置失败。");
    }
  }

  async function handleRecoverAuthoritativeSettings(): Promise<void> {
    setLoading(true);
    setInitialLoadError(null);
    setNotice(null);
    setError(null);
    try {
      const refreshed = await desktopApi.refreshWatcherStatus();
      applyAuthoritativePayload(refreshed, false);
      const runtimeWarning = getSettingsRuntimeWarning(refreshed);
      if (runtimeWarning) {
        setError(`已从磁盘安全应用设置，但${runtimeWarning}`);
      } else {
        setNotice("已从磁盘安全应用设置并刷新监听器。 ");
      }
    } catch (refreshError) {
      setInitialLoadError(
        refreshError instanceof Error ? refreshError.message : "无法从磁盘安全应用设置。"
      );
    } finally {
      setLoading(false);
    }
  }

  async function handleOpenExternalDocumentation(): Promise<void> {
    setNotice(null);
    setError(null);

    try {
      await desktopApi.openExternal("https://api-docs.deepseek.com/");
    } catch (openError) {
      setError(openError instanceof Error ? openError.message : "打开 DeepSeek API 文档失败。");
    }
  }

  function handleCancelDraft(): void {
    setDraftSettings(savedSettings);
    setPopupWidthInput(String(savedSettings.popupWidth));
    setApiKey("");
    setError(null);
    setNotice("未保存的更改已取消。");
  }

  async function handleCopyPermissionDiagnostics(): Promise<void> {
    setNotice(null);
    setError(null);

    try {
      // Pull a read-only snapshot at click time. The native side ignores this
      // window's own mouse-up, so the Preview attempt remains the diagnostic
      // source instead of being replaced by the Copy button interaction.
      const latest = await desktopApi.getSettings();
      applyRuntimeStatus(latest);
      if (!latest.permissionDiagnostics) {
        throw new Error("当前平台没有可复制的权限诊断信息。");
      }
      await desktopApi.copyText(
        buildPermissionDiagnosticsText(
          latest.permissionDiagnostics,
          latest.capabilitySnapshot,
          latest.selectionStatus,
          latest.doubleCopyStatus
        )
      );
      setNotice("权限诊断信息已复制。");
    } catch (copyError) {
      setError(copyError instanceof Error ? copyError.message : "复制权限诊断信息失败。");
    }
  }

  const selectionDisabledConfirmed =
    !savedSettings.enableSelectionPopup && selectionStatus.code === "selection_disabled_by_setting";
  const selectionDisabledHealthy = selectionDisabledConfirmed && doubleCopyStatus.running;
  const selectionStatusMessage = savedSettings.enableSelectionPopup
    ? selectionStatus.message || "自动选区监听状态未知。"
    : selectionDisabledHealthy
      ? "拖选浮窗已确认关闭，仍可使用 Cmd+C+C。"
      : selectionDisabledConfirmed
        ? [selectionStatus.message, doubleCopyStatus.message].filter(Boolean).join(" ")
        : selectionStatus.message || "自动划词设置已关闭，但原生资源释放状态尚未确认。";
  const settingsLanguageSelectValue = LANGUAGE_OPTIONS.includes(draftSettings.targetLanguage)
    ? draftSettings.targetLanguage
    : CUSTOM_LANGUAGE_VALUE;
  const capabilityPresentation = getSelectionCapabilityPresentation(capabilitySnapshot);
  const selectionDisplayDetail = savedSettings.enableSelectionPopup
    ? capabilityPresentation.detail
    : selectionStatusMessage;
  const doubleCopyPresentation = getDoubleCopyCapabilityPresentation(capabilitySnapshot);
  const permissionActions = getCapabilityPermissionActions(capabilitySnapshot);
  const selectionTone: StatusTone = savedSettings.enableSelectionPopup
    ? capabilityPresentation.tone
    : selectionDisabledHealthy
      ? "neutral"
      : "warning";
  const selectionStatusValue = savedSettings.enableSelectionPopup
    ? capabilityPresentation.value
    : selectionDisabledConfirmed
      ? selectionDisabledHealthy
        ? "已关闭"
        : "已关闭 / 兜底停用"
      : "停用待确认";
  const selectionDiagnostic =
    savedSettings.enableSelectionPopup && capabilityPresentation.tone === "warning"
      ? { detail: capabilityPresentation.detail }
      : !savedSettings.enableSelectionPopup && !selectionDisabledHealthy
        ? { detail: selectionStatusMessage }
        : null;
  const shouldOfferAccessibilitySettings =
    savedSettings.enableSelectionPopup && permissionActions.includes("open_accessibility_settings");
  const shouldOfferInputMonitoringSettings =
    savedSettings.enableSelectionPopup && permissionActions.includes("open_input_monitoring_settings");
  const shouldShowPermissionDiagnostics =
    permissionDiagnostics !== undefined &&
    (selectionDiagnostic !== null ||
      permissionDiagnostics.signatureKind !== "developer_id" ||
      permissionDiagnostics.selectionRead !== undefined ||
      capabilitySnapshot.accessibility.grant !== "granted" ||
      capabilitySnapshot.listenEvent.grant !== "granted" ||
      capabilitySnapshot.mouseTap.health !== "ready" ||
      capabilitySnapshot.axSelectedTextObserver.health !== "ready" ||
      capabilitySnapshot.directSelectionRead.health !== "ready" ||
      (capabilitySnapshot.keyTap.health !== "ready" &&
        capabilitySnapshot.clipboardDoubleCopyFallback.health !== "ready"));
  const shouldShowPermissionActions =
    selectionDiagnostic !== null ||
    (savedSettings.enableSelectionPopup && shouldShowPermissionDiagnostics);
  const lifecycleMessage = savedSettings.enableSelectionPopup
    ? "关闭设置窗口只会隐藏界面，拖选与 Cmd+C+C 监听会继续在后台运行；点击 Dock 图标可重新打开，按 Cmd+Q 才会完全退出。"
    : selectionDisabledHealthy
      ? "关闭设置窗口只会隐藏界面；拖选监听已确认关闭，Cmd+C+C 仍会在后台运行。点击 Dock 图标可重新打开，按 Cmd+Q 才会完全退出。"
      : selectionDisabledConfirmed
        ? "拖选监听已关闭，但 Cmd+C+C 当前也未运行；请刷新诊断以恢复，按 Cmd+Q 可完全退出。"
        : "自动划词停用尚未得到原生资源释放确认；请刷新诊断，若仍未确认请按 Cmd+Q 退出后重新打开。";
  const hasUnsavedChanges = hasUnsavedSettingsChanges(
    savedSettings,
    draftSettings,
    popupWidthInput,
    apiKey
  );

  if (loading) {
    return (
      <main className="settings-page centered">
        <h1 className="visually-hidden">正在读取设置</h1>
        <Loader2 className="spin" size={22} />
        <span>正在读取设置</span>
      </main>
    );
  }

  if (initialLoadError) {
    return (
      <main className="settings-page centered load-failure">
        <section role="alert" aria-labelledby="settings-load-error-title">
          <AlertCircle size={24} />
          <h1 id="settings-load-error-title">无法读取当前设置</h1>
          <span>{initialLoadError}</span>
          <button type="button" className="primary" onClick={() => setLoadAttempt((attempt) => attempt + 1)}>
            <RefreshCcw size={16} />
            重新读取
          </button>
          <button type="button" className="secondary" onClick={handleRecoverAuthoritativeSettings}>
            <RefreshCcw size={16} />
            刷新并应用磁盘设置
          </button>
        </section>
      </main>
    );
  }

  return (
    <main className="settings-page">
      <header className="settings-header">
        <div className="brand-block">
          <div className="brand-mark" aria-hidden="true">
            <Languages size={28} />
          </div>
          <div>
            <p className="eyebrow">Paper Float Translator</p>
            <h1>AI 翻译助手</h1>
            <p>为论文阅读保留一个低打扰的翻译浮窗，拖选或 Cmd+C+C 即可触发。</p>
          </div>
        </div>
        <div className="header-actions">
          <span className="platform-pill">macOS · Tauri</span>
          <button
            className="icon-link"
            type="button"
            onClick={() => void handleOpenExternalDocumentation()}
            title="打开 DeepSeek API 文档"
            aria-label="打开 DeepSeek API 文档"
          >
            <ExternalLink size={18} />
          </button>
        </div>
      </header>

      <section className="status-strip" aria-label="运行状态">
        <StatusItem
          icon={<KeyRound size={19} />}
          label="API 状态"
          value={apiKeyStatus === "configured" ? "已连接" : apiKeyStatus === "unavailable" ? "暂不可读" : "未配置"}
          detail={
            apiKeyStatus === "configured"
              ? apiKeyStorage === "local_file"
                ? "本地构建：Key 仅保存在当前用户可读文件，不访问系统钥匙串"
                : "Key 已保存在系统钥匙串"
              : apiKeyStatus === "unavailable"
                ? apiKeyStorage === "local_file"
                  ? "本地 Key 文件暂不可读；请在下方重新保存"
                  : "后台不会弹出密码框；请在下方重新保存 API Key"
                : apiKeyStorage === "local_file"
                  ? "本地构建不读取钥匙串；请重新保存一次 API Key"
                  : "旧签名钥匙串条目已停用；请重新保存一次 API Key"
          }
          tone={apiKeyStatus === "configured" ? "ok" : "warning"}
        />
        <StatusItem
          icon={<MousePointer2 size={19} />}
          label="选区监听"
          value={selectionStatusValue}
          detail={selectionDisplayDetail}
          tone={selectionTone}
        />
        <StatusItem
          icon={<Database size={19} />}
          label="本地缓存"
          value={savedSettings.enableCache ? "已启用" : "已关闭"}
          detail={savedSettings.enableCache ? "相同文本会优先复用结果" : "每次翻译都会重新请求"}
          tone={savedSettings.enableCache ? "ok" : "neutral"}
        />
      </section>

      <div className="settings-edit-state" data-dirty={hasUnsavedChanges}>
        <span>上方显示当前已生效状态；下方表单是设置草稿。</span>
        <strong>{hasUnsavedChanges ? "草稿尚未保存" : "草稿与当前生效设置一致"}</strong>
      </div>

      <section className="settings-grid">
        <div className="panel">
          <div className="panel-heading">
            <div className="section-icon">
              <KeyRound size={18} />
            </div>
            <div>
              <h2>翻译服务</h2>
              <p>DeepSeek 连接与默认翻译参数。</p>
            </div>
          </div>

          <label className="field">
            <span>API Key</span>
            <input
              type="password"
              value={apiKey}
              disabled={saving}
              placeholder={
                hasApiKey
                  ? apiKeyStorage === "local_file"
                    ? "已保存在当前用户私有文件，留空则不修改"
                    : "已保存在新钥匙串条目，留空则不修改"
                  : apiKeyStorage === "local_file"
                    ? "重新输入 DeepSeek API Key（本地构建不读取钥匙串）"
                    : "重新输入 DeepSeek API Key（不会读取旧条目）"
              }
              onChange={(event) => setApiKey(event.target.value)}
            />
          </label>

          <label className="field">
            <span>默认模型</span>
            <select
              value={draftSettings.model}
              disabled={saving}
              onChange={(event) =>
                setDraftSettings({ ...draftSettings, model: event.target.value as DeepSeekModel })
              }
            >
              {MODEL_OPTIONS.map((option) => (
                <option key={option.value} value={option.value}>
                  {option.label}
                </option>
              ))}
            </select>
          </label>

          <label className="field">
            <span>翻译模式</span>
            <select
              value={draftSettings.mode}
              disabled={saving}
              onChange={(event) =>
                setDraftSettings({ ...draftSettings, mode: event.target.value as TranslateMode })
              }
            >
              {MODE_OPTIONS.map((option) => (
                <option key={option.value} value={option.value}>
                  {option.label}
                </option>
              ))}
            </select>
          </label>

          <label className="field">
            <span>默认目标语言</span>
            <select
              value={settingsLanguageSelectValue}
              disabled={saving}
              onChange={(event) => {
                const nextLanguage = event.target.value;
                setDraftSettings({
                  ...draftSettings,
                  targetLanguage: nextLanguage === CUSTOM_LANGUAGE_VALUE ? "" : nextLanguage
                });
              }}
            >
              {LANGUAGE_OPTIONS.map((language) => (
                <option key={language} value={language}>
                  {language}
                </option>
              ))}
              <option value={CUSTOM_LANGUAGE_VALUE}>自定义...</option>
            </select>
            {settingsLanguageSelectValue === CUSTOM_LANGUAGE_VALUE ? (
              <input
                value={draftSettings.targetLanguage}
                disabled={saving}
                placeholder="输入目标语言，例如 俄文"
                onChange={(event) =>
                  setDraftSettings({ ...draftSettings, targetLanguage: event.target.value })
                }
              />
            ) : null}
            <small>拖选翻译和 Cmd+C+C 会直接使用这个语言。</small>
          </label>
        </div>

        <div className="panel">
          <div className="panel-heading">
            <div className="section-icon">
              <Settings size={18} />
            </div>
            <div>
              <h2>桌面行为</h2>
              <p>控制触发方式、浮窗尺寸和本地处理。</p>
            </div>
          </div>

          <p className="lifecycle-note">
            {lifecycleMessage}
          </p>

          <label className="switch-row">
            <input
              type="checkbox"
              checked={draftSettings.enableSelectionPopup}
              disabled={saving}
              onChange={(event) =>
                setDraftSettings({ ...draftSettings, enableSelectionPopup: event.target.checked })
              }
            />
            <span className="switch-track" aria-hidden="true" />
            <span className="switch-copy">
              <strong>拖选后显示操作浮窗</strong>
              <small>{selectionDisplayDetail}</small>
            </span>
          </label>

          {shouldShowPermissionActions ? (
            <div className="permission-actions" role="group" aria-label="选区监听权限操作">
              {shouldOfferAccessibilitySettings ? (
                <button
                  type="button"
                  className="inline-action"
                  disabled={saving}
                  onClick={() => void handleOpenPermissionSettings("accessibility")}
                >
                  <ExternalLink size={14} />
                  打开辅助功能权限
                </button>
              ) : null}
              {shouldOfferInputMonitoringSettings ? (
                <button
                  type="button"
                  className="inline-action"
                  disabled={saving}
                  onClick={() => void handleOpenPermissionSettings("input_monitoring")}
                >
                  <ExternalLink size={14} />
                  打开输入监听权限
                </button>
              ) : null}
              <button
                type="button"
                className="inline-action"
                onClick={handleRefreshWatcherStatus}
                disabled={saving || refreshingWatchers}
              >
                {refreshingWatchers ? <Loader2 size={14} className="spin" /> : <RefreshCcw size={14} />}
                刷新诊断
              </button>
              {selectionDiagnostic ? <small>{selectionDiagnostic.detail}</small> : null}
              {permissionDiagnostics ? (
                <div className="permission-diagnostics">
                  <div>
                    <span>辅助功能授权</span>
                    <strong>{formatPermissionGrant(capabilitySnapshot.accessibility.grant)}</strong>
                  </div>
                  <div>
                    <span>Listen Event 授权</span>
                    <strong>{formatPermissionGrant(capabilitySnapshot.listenEvent.grant)}</strong>
                  </div>
                  <div>
                    <span>鼠标 tap</span>
                    <strong>{formatCapabilityHealth(capabilitySnapshot.mouseTap.health)}</strong>
                  </div>
                  <div>
                    <span>键盘 tap</span>
                    <strong>{formatCapabilityHealth(capabilitySnapshot.keyTap.health)}</strong>
                  </div>
                  <div>
                    <span>AX selectedText</span>
                    <strong>{formatCapabilityHealth(capabilitySnapshot.axSelectedTextObserver.health)}</strong>
                  </div>
                  <div>
                    <span>直接读取选区</span>
                    <strong>{formatCapabilityHealth(capabilitySnapshot.directSelectionRead.health)}</strong>
                  </div>
                  <div>
                    <span>剪贴板双复制回退</span>
                    <strong>{formatCapabilityHealth(capabilitySnapshot.clipboardDoubleCopyFallback.health)}</strong>
                  </div>
                  <div>
                    <span>签名</span>
                    <strong>{formatSignatureKind(permissionDiagnostics.signatureKind)}</strong>
                  </div>
                  <div>
                    <span>应用版本</span>
                    <strong>{permissionDiagnostics.appVersion}</strong>
                  </div>
                  <div>
                    <span>Team ID</span>
                    <code>{permissionDiagnostics.teamIdentifier || "not set"}</code>
                  </div>
                  <div>
                    <span>CDHash</span>
                    <code>{permissionDiagnostics.cdHash || "unknown"}</code>
                  </div>
                  <div className="wide">
                    <span>运行路径</span>
                    <code>{permissionDiagnostics.bundlePath}</code>
                  </div>
                  {permissionDiagnostics.selectionRead ? (
                    <>
                      <div>
                        <span>最近读取</span>
                        <strong>{formatSelectionReadStatus(permissionDiagnostics.selectionRead.status)}</strong>
                      </div>
                      <div>
                        <span>触发来源</span>
                        <code>{permissionDiagnostics.selectionRead.reason}</code>
                      </div>
                      <div>
                        <span>来源 bundle ID</span>
                        <code>{permissionDiagnostics.selectionRead.sourceBundleId}</code>
                      </div>
                      <div>
                        <span>读取耗时</span>
                        <strong>{permissionDiagnostics.selectionRead.durationMs.toFixed(1)} ms</strong>
                      </div>
                      <div>
                        <span>候选元素</span>
                        <strong>{permissionDiagnostics.selectionRead.candidateCount}</strong>
                      </div>
                      <div>
                        <span>AX 错误</span>
                        <code>{permissionDiagnostics.selectionRead.axError}</code>
                      </div>
                    </>
                  ) : null}
                  {permissionDiagnostics.signatureKind !== "developer_id" ? (
                    <p>
                      当前签名不是 Developer ID Application，不能作为最终发布与 TCC 持久性验收包。
                      ad-hoc 覆盖安装后 macOS 还可能显示旧授权，但这不代表当前构建已获信任。
                    </p>
                  ) : null}
                  <button type="button" className="inline-action" onClick={handleCopyPermissionDiagnostics}>
                    <Copy size={14} />
                    复制诊断信息
                  </button>
                </div>
              ) : null}
            </div>
          ) : null}

          <label className="field">
            <span>兜底触发</span>
            <input value="快速按两次 Cmd+C" readOnly />
            <small>{doubleCopyStatus.message || "双复制监听状态未知。"}</small>
            {doubleCopyPresentation.tone === "warning" ? <small>{doubleCopyPresentation.detail}</small> : null}
          </label>

          <label className="field">
            <span>浮窗宽度</span>
            <input
              type="number"
              min={320}
              max={640}
              step={20}
              value={popupWidthInput}
              disabled={saving}
              onChange={(event) => setPopupWidthInput(event.target.value)}
            />
            <small>范围 320-640，保存时生效。</small>
          </label>

          <label className="switch-row">
            <input
              type="checkbox"
              checked={draftSettings.cleanPdfText}
              disabled={saving}
              onChange={(event) =>
                setDraftSettings({ ...draftSettings, cleanPdfText: event.target.checked })
              }
            />
            <span className="switch-track" aria-hidden="true" />
            <span className="switch-copy">
              <strong>自动清洗 PDF 换行和断词</strong>
              <small>减少论文复制时常见的换行、断词干扰。</small>
            </span>
          </label>

          <label className="switch-row">
            <input
              type="checkbox"
              checked={draftSettings.enableCache}
              disabled={saving}
              onChange={(event) =>
                setDraftSettings({ ...draftSettings, enableCache: event.target.checked })
              }
            />
            <span className="switch-track" aria-hidden="true" />
            <span className="switch-copy">
              <strong>启用本地翻译缓存</strong>
              <small>相同参数的文本会优先使用本地结果。</small>
            </span>
          </label>
        </div>
      </section>

      <AcceptancePanel onRuntimeStatusChange={applyAcceptanceRuntimeStatus} />

      <footer className="settings-actions">
        <div
          className={error ? "status-line error" : hasUnsavedChanges ? "status-line dirty" : "status-line"}
          aria-live="polite"
          role={error ? "alert" : "status"}
        >
          {!notice && !error && hasUnsavedChanges ? <span>有尚未保存的更改。</span> : null}
          {notice ? (
            <>
              <Check size={16} />
              <span>{notice}</span>
            </>
          ) : null}
          {error ? (
            <>
              <AlertCircle size={16} />
              <span>{error}</span>
            </>
          ) : null}
        </div>

        <div className="button-row">
          <button type="button" className="secondary" onClick={handleClearCache} disabled={saving}>
            <Eraser size={16} />
            清空缓存
          </button>
          <button
            type="button"
            className="secondary"
            onClick={handleClearApiKey}
            disabled={saving || apiKeyStatus === "missing"}
          >
            <KeyRound size={16} />
            清除 Key
          </button>
          <button type="button" className="secondary" onClick={handleCancelDraft} disabled={saving || !hasUnsavedChanges}>
            <RotateCcw size={16} />
            取消更改
          </button>
          <button type="button" className="primary" onClick={handleSave} disabled={saving || !hasUnsavedChanges}>
            {saving ? <Loader2 size={16} className="spin" /> : <Save size={16} />}
            保存设置
          </button>
        </div>
      </footer>
    </main>
  );
}

type StatusTone = "ok" | "warning" | "neutral";

interface StatusItemProps {
  icon: ReactNode;
  label: string;
  value: string;
  detail: string;
  tone: StatusTone;
}

function StatusItem({ icon, label, value, detail, tone }: StatusItemProps): JSX.Element {
  return (
    <div className="status-card" data-tone={tone}>
      <div className="status-icon" aria-hidden="true">
        {icon}
      </div>
      <div>
        <span>{label}</span>
        <strong>{value}</strong>
        <p>{detail}</p>
      </div>
    </div>
  );
}

function formatSelectionReadStatus(status: string): string {
  switch (status) {
    case "selection_read_found":
      return "已找到文本";
    case "selection_read_duplicate":
      return "重复选区";
    case "selection_read_empty":
      return "没有文本";
    default:
      return status;
  }
}

function formatSignatureKind(kind: PermissionDiagnostics["signatureKind"]): string {
  switch (kind) {
    case "adhoc":
      return "ad-hoc";
    case "apple_development":
      return "Apple Development";
    case "developer_id":
      return "Developer ID";
    default:
      return "unknown";
  }
}

function formatPermissionGrant(grant: CapabilitySnapshot["accessibility"]["grant"]): string {
  switch (grant) {
    case "granted":
      return "已授权";
    case "denied":
      return "未授权";
    case "unsupported":
      return "不支持";
    default:
      return "未知";
  }
}

function formatCapabilityHealth(health: CapabilitySnapshot["mouseTap"]["health"]): string {
  switch (health) {
    case "ready":
      return "就绪";
    case "degraded":
      return "受限";
    case "disabled":
      return "已禁用";
    case "unavailable":
      return "不可用";
    default:
      return "未知";
  }
}

function buildPermissionDiagnosticsText(
  diagnostics: PermissionDiagnostics,
  capabilitySnapshot: CapabilitySnapshot,
  selectionStatus: WatcherStatus,
  doubleCopyStatus: WatcherStatus
): string {
  return [
    "Paper Float Translator permission diagnostics",
    `Bundle ID: ${diagnostics.bundleIdentifier}`,
    `App version: ${diagnostics.appVersion}`,
    `Bundle path: ${diagnostics.bundlePath}`,
    `Executable path: ${diagnostics.executablePath}`,
    `Signature: ${formatSignatureKind(diagnostics.signatureKind)}`,
    `Team ID: ${diagnostics.teamIdentifier || "not set"}`,
    `CDHash: ${diagnostics.cdHash || "unknown"}`,
    `Accessibility trusted: ${diagnostics.accessibilityTrusted ? "true" : "false"}`,
    `Accessibility grant: ${capabilitySnapshot.accessibility.grant} | health=${capabilitySnapshot.accessibility.health} | status=${capabilitySnapshot.accessibility.statusCode || "none"}`,
    `Listen Event grant: ${capabilitySnapshot.listenEvent.grant} | health=${capabilitySnapshot.listenEvent.health} | status=${capabilitySnapshot.listenEvent.statusCode || "none"}`,
    `Mouse tap: ${capabilitySnapshot.mouseTap.health} | status=${capabilitySnapshot.mouseTap.statusCode || "none"}`,
    `Key tap: ${capabilitySnapshot.keyTap.health} | status=${capabilitySnapshot.keyTap.statusCode || "none"}`,
    `AX selectedText observer: ${capabilitySnapshot.axSelectedTextObserver.health} | status=${capabilitySnapshot.axSelectedTextObserver.statusCode || "none"}`,
    `Direct selection read: ${capabilitySnapshot.directSelectionRead.health} | status=${capabilitySnapshot.directSelectionRead.statusCode || "none"}`,
    `Clipboard double-copy fallback: ${capabilitySnapshot.clipboardDoubleCopyFallback.health} | status=${capabilitySnapshot.clipboardDoubleCopyFallback.statusCode || "none"}`,
    diagnostics.selectionRead
      ? [
          `Selection read status: ${diagnostics.selectionRead.status}`,
          `Selection read reason: ${diagnostics.selectionRead.reason}`,
          `Selection read found: ${diagnostics.selectionRead.foundText}`,
          `Selection read candidates: ${diagnostics.selectionRead.candidateCount}`,
          `Selection read duration ms: ${diagnostics.selectionRead.durationMs}`,
          `Selection read source bundle ID: ${diagnostics.selectionRead.sourceBundleId}`,
          `Selection read AX error: ${diagnostics.selectionRead.axError}`,
          `Selection read generation: ${diagnostics.selectionRead.generation ?? "none"}`,
          `Selection read attempt: ${diagnostics.selectionRead.attempt ?? "none"}`,
          `Selection read trigger-to-read ms: ${diagnostics.selectionRead.triggerToReadMs ?? "none"}`,
          `Selection read terminal: ${diagnostics.selectionRead.terminal ?? "none"}`
        ].join("\n")
      : "Selection read: none",
    `Selection status: ${selectionStatus.code || "none"} | available=${selectionStatus.available} | running=${selectionStatus.running}`,
    `Selection message: ${selectionStatus.message}`,
    `Double copy status: ${doubleCopyStatus.code || "none"} | available=${doubleCopyStatus.available} | running=${doubleCopyStatus.running}`,
    `Double copy message: ${doubleCopyStatus.message}`
  ].join("\n");
}

function parsePopupWidthInput(value: string): number | null {
  if (!value.trim()) {
    return null;
  }

  const parsed = Number(value);

  if (!Number.isFinite(parsed)) {
    return null;
  }

  const rounded = Math.round(parsed);
  return rounded >= 320 && rounded <= 640 ? rounded : null;
}

function formatErrorMessage(error: unknown, fallback: string): string {
  return error instanceof Error && error.message.trim() ? error.message : fallback;
}

function runtimeWarningDetail(warning: string): string {
  return warning.replace(/^设置已保存(?:，|,)?但?/, "").trim();
}

function formatSavedRuntimeWarning(warning: string): string {
  return `设置已保存，但${runtimeWarningDetail(warning)}`;
}

function appSettingsEqual(left: AppSettings, right: AppSettings): boolean {
  return (
    left.model === right.model &&
    left.mode === right.mode &&
    left.cleanPdfText === right.cleanPdfText &&
    left.enableCache === right.enableCache &&
    left.enableSelectionPopup === right.enableSelectionPopup &&
    left.targetLanguage === right.targetLanguage &&
    left.popupWidth === right.popupWidth
  );
}

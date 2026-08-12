import { useEffect, useLayoutEffect, useRef, useState, type PointerEvent } from "react";
import {
  AlertCircle,
  BookOpenText,
  Check,
  Clipboard,
  Languages,
  Loader2,
  Pin,
  PinOff,
  RefreshCcw,
  Settings,
  X
} from "lucide-react";
import {
  DEFAULT_TARGET_LANGUAGE,
  HIDDEN_POPUP_STATE,
  normalizeTargetLanguage,
  type PopupState
} from "@paper-float-translator/core/renderer";
import { desktopApi, type PopupKeyboardEntry } from "./desktopApi";
import { compactPopupPreview, popupErrorTitle } from "./popupPresentation";

const INITIAL_STATE: PopupState = {
  ...HIDDEN_POPUP_STATE
};

const LANGUAGE_OPTIONS = ["中文", "英文", "日文", "韩文", "法文", "德文", "西班牙文"];
const CUSTOM_LANGUAGE_VALUE = "__custom__";

export function PopupView(): JSX.Element {
  const [state, setState] = useState<PopupState>(INITIAL_STATE);
  const [connectionError, setConnectionError] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const [connectionAttempt, setConnectionAttempt] = useState(0);
  const [targetLanguageInput, setTargetLanguageInput] = useState<string>(DEFAULT_TARGET_LANGUAGE);
  const [copied, setCopied] = useState(false);
  const rootRef = useRef<HTMLDivElement | null>(null);
  const dragInProgressRef = useRef(false);
  const stateRef = useRef<PopupState>(INITIAL_STATE);
  const pendingKeyboardEntryRef = useRef<PopupKeyboardEntry | null>(null);
  const keyboardFocusFrameRef = useRef<number | null>(null);
  stateRef.current = state;
  const isSelectionState = state.status === "selection_pending" || state.status === "selection_ready";
  const isSelectionReady = state.status === "selection_ready";
  const targetLanguageSelectValue = LANGUAGE_OPTIONS.includes(targetLanguageInput)
    ? targetLanguageInput
    : CUSTOM_LANGUAGE_VALUE;
  const sourcePreview = compactPopupPreview(state.cleanedText ?? state.sourceText ?? state.selectedText);
  const shellClassName = [
    "popup-shell",
    isSelectionState ? "selection-shell" : "",
    isBrowserPreview() ? "browser-preview" : ""
  ]
    .filter(Boolean)
    .join(" ");

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
    return desktopApi.onPopupState(
      (nextState) => {
        setConnectionError(null);
        setActionError(null);
        setCopied(false);
        if (nextState.status !== "selection_pending" && nextState.status !== "selection_ready") {
          setTargetLanguageInput(normalizeTargetLanguage(nextState.targetLanguage, DEFAULT_TARGET_LANGUAGE));
        }
        setState(nextState);
      },
      (error) => {
        setConnectionError(`无法同步浮窗状态：${error.message}`);
        void desktopApi.revealPopupProtocolError().catch(() => undefined);
      }
    );
  }, [connectionAttempt]);

  useEffect(() => {
    const flushKeyboardEntry = (): void => {
      keyboardFocusFrameRef.current = null;
      const entry = pendingKeyboardEntryRef.current;
      if (!entry) {
        return;
      }
      const result = focusSelectionControl(entry);
      if (result !== "wait") {
        pendingKeyboardEntryRef.current = null;
      }
    };

    const unsubscribe = desktopApi.onPopupKeyboardEntry(
      (entry) => {
        pendingKeyboardEntryRef.current = entry;
        if (keyboardFocusFrameRef.current !== null) {
          window.cancelAnimationFrame(keyboardFocusFrameRef.current);
        }
        keyboardFocusFrameRef.current = window.requestAnimationFrame(flushKeyboardEntry);
      },
      (error) => setConnectionError(`无法建立键盘入口：${error.message}`)
    );

    return () => {
      unsubscribe();
      if (keyboardFocusFrameRef.current !== null) {
        window.cancelAnimationFrame(keyboardFocusFrameRef.current);
      }
    };
  }, []);

  useEffect(() => {
    const listener = (event: KeyboardEvent): void => {
      if (event.key === "Escape") {
        closePopup();
      }
    };

    window.addEventListener("keydown", listener);
    return () => window.removeEventListener("keydown", listener);
  }, []);

  useLayoutEffect(() => {
    if (!rootRef.current) {
      return;
    }

    const height = Math.ceil(rootRef.current.scrollHeight + 2);
    void desktopApi.resizePopup(height).catch((error: unknown) => {
      const message = error instanceof Error && error.message.trim() ? error.message : "无法调整浮窗尺寸。";
      setConnectionError(`无法同步浮窗状态：${message}`);
    });
  }, [connectionError, state, targetLanguageInput]);

  useLayoutEffect(() => {
    const entry = pendingKeyboardEntryRef.current;
    if (entry && focusSelectionControl(entry) !== "wait") {
      pendingKeyboardEntryRef.current = null;
    }
  }, [state.revision, state.selectionRevision, state.status, state.visible]);

  function focusSelectionControl(
    entry: PopupKeyboardEntry
  ): "focused" | "wait" | "discarded" {
    const current = stateRef.current;
    if (entry.revision > current.revision) {
      return "wait";
    }
    if (
      entry.revision !== current.revision ||
      entry.selectionRevision !== current.selectionRevision ||
      !current.visible ||
      (current.status !== "selection_pending" && current.status !== "selection_ready")
    ) {
      return "discarded";
    }

    const controls = Array.from(
      rootRef.current?.querySelectorAll<HTMLElement>(
        ".selection-actions button:not([disabled]), .selection-actions select:not([disabled]), .selection-actions input:not([disabled])"
      ) ?? []
    ).filter((control) => control.getClientRects().length > 0);
    const target = entry.direction === "backward" ? controls.at(-1) : controls[0];
    if (!target) {
      return "wait";
    }
    target.focus();
    return "focused";
  }

  async function copyTranslation(): Promise<void> {
    if (await runPopupAction(desktopApi.copyTranslation, "复制译文失败。")) {
      setCopied(true);
    }
  }

  async function copySource(): Promise<void> {
    if (await runPopupAction(desktopApi.copySource, "复制原文失败。")) {
      setCopied(true);
    }
  }

  async function translateSelection(): Promise<void> {
    await runPopupAction(desktopApi.translateSelection, "启动翻译失败。");
  }

  async function retryTranslation(): Promise<void> {
    await runPopupAction(
      () => desktopApi.retryTranslation(normalizeTargetLanguage(targetLanguageInput, state.targetLanguage)),
      "重新翻译失败。"
    );
  }

  async function explainTerms(): Promise<void> {
    await runPopupAction(
      () => desktopApi.explainTerms(normalizeTargetLanguage(targetLanguageInput, state.targetLanguage)),
      "术语解释失败。"
    );
  }

  async function runPopupAction(action: () => Promise<void>, fallback: string): Promise<boolean> {
    setActionError(null);
    try {
      await action();
      return true;
    } catch (error) {
      setActionError(error instanceof Error && error.message.trim() ? error.message : fallback);
      return false;
    }
  }

  function closePopup(): void {
    void runPopupAction(desktopApi.closePopup, "关闭浮窗失败。");
  }

  function togglePin(): void {
    void runPopupAction(desktopApi.togglePin, "更新固定状态失败。");
  }

  function openRecovery(action: () => Promise<void>, fallback: string): void {
    void runPopupAction(action, fallback);
  }

  function stopDragPropagation(event: PointerEvent<HTMLElement>): void {
    event.stopPropagation();
  }

  function startWindowDrag(event: PointerEvent<HTMLElement>): void {
    if (event.button !== 0 || dragInProgressRef.current) {
      return;
    }

    event.preventDefault();
    event.stopPropagation();
    dragInProgressRef.current = true;
    void desktopApi.setPopupDragging(true).catch(() => {
      if (!isBrowserPreview()) {
        setActionError("无法进入拖动状态。");
      }
    });
    void desktopApi
      .startWindowDrag()
      .catch(() => {
        if (!isBrowserPreview()) {
          setActionError("无法拖动浮窗。");
        }
      })
      .finally(() => {
        dragInProgressRef.current = false;
        void desktopApi.setPopupDragging(false).catch(() => {
          if (!isBrowserPreview()) {
            setActionError("无法结束拖动状态。");
          }
        });
      });
  }

  return (
    <main
      ref={rootRef}
      className={shellClassName}
      data-status={state.status}
      data-visible={state.visible || Boolean(connectionError)}
      aria-label="划词翻译浮窗"
      aria-busy={!connectionError && state.status === "translating"}
    >
      <h1 className="visually-hidden">划词翻译浮窗</h1>
      {connectionError ? (
        <section className="popup-body" aria-live="assertive">
          <div className="popup-error" role="alert">
            <AlertCircle size={18} />
            <span dir="auto">
              <strong>浮窗连接异常</strong>
              {connectionError}
            </span>
          </div>
        </section>
      ) : null}

      {connectionError ? (
        <footer className="popup-actions" aria-label="连接异常操作">
          <button type="button" onClick={() => setConnectionAttempt((attempt) => attempt + 1)}>
            <RefreshCcw size={15} />
            重新连接
          </button>
          <button type="button" onClick={closePopup}>
            <X size={15} />
            关闭
          </button>
        </footer>
      ) : null}

      {!connectionError && state.visible && !isSelectionState ? (
        <>
          <header
            className="popup-header"
            onPointerDown={startWindowDrag}
          >
            <div className="popup-title">
              <span className="popup-brand-mark" aria-hidden="true">
                <Languages size={15} />
              </span>
              <span>Paper Float</span>
              <span className="popup-state-label">
                {state.status === "translating"
                  ? "翻译中"
                  : state.status === "error"
                    ? "需要处理"
                    : "译文"}
              </span>
              {state.cached ? <span className="badge">缓存</span> : null}
            </div>
            <div className="popup-tools" onPointerDownCapture={stopDragPropagation}>
              <button
                type="button"
                title={state.pinned ? "取消固定" : "固定窗口"}
                aria-pressed={state.pinned}
                onClick={togglePin}
              >
                {state.pinned ? <PinOff size={16} /> : <Pin size={16} />}
              </button>
              <button
                type="button"
                title="关闭并静默当前选区"
                aria-label="关闭"
                onClick={closePopup}
              >
                <X size={16} />
              </button>
            </div>
          </header>

          <section
            className="popup-body"
            aria-live="polite"
            aria-busy={state.status === "translating"}
          >
            {actionError ? (
              <div className="popup-error compact" role="alert">
                <AlertCircle size={16} />
                <span dir="auto">
                  <strong>操作失败</strong>
                  {actionError}
                </span>
              </div>
            ) : null}
            {state.status === "translating" ? (
              state.translation ? (
                <>
                  <div className="popup-message compact">
                    <Loader2 className="spin" size={16} />
                    <span>正在接收译文</span>
                  </div>
                  <article className="translation-text streaming" dir="auto">{state.translation}</article>
                </>
              ) : (
                <div className="popup-message">
                  <Loader2 className="spin" size={20} />
                  <span>正在翻译</span>
                </div>
              )
            ) : null}

            {state.status === "error" ? (
              <div className="popup-error" role="alert">
                <AlertCircle size={18} />
                <span dir="auto">
                  <strong>{popupErrorTitle(state.errorKind)}</strong>
                  {state.error ?? "翻译失败。"}
                </span>
              </div>
            ) : null}

            {state.status === "translated" ? (
              <>
                <article className="translation-text" dir="auto">{state.translation}</article>
                {sourcePreview ? (
                  <div className="source-preview">
                    <span>原文</span>
                    <p dir="auto">{sourcePreview}</p>
                  </div>
                ) : null}
              </>
            ) : null}

          </section>
        </>
      ) : null}

      {!connectionError && state.visible ? (
        <footer
          className={
            state.status === "selection_pending"
              ? "popup-actions selection-actions pending-actions"
              : isSelectionState
                ? "popup-actions selection-actions"
                : "popup-actions"
          }
          aria-label={isSelectionState ? "选区操作" : "翻译操作"}
        >
        {isSelectionState && !isSelectionReady ? (
          <>
            <div
              className="selection-summary selection-drag-surface pending-summary"
              role="status"
              aria-live="polite"
              title="拖动浮窗"
              onPointerDown={startWindowDrag}
            >
              <span className="pending-title">
                <Loader2 className="spin" size={16} />
                <strong>{actionError ? "操作失败" : "正在读取所选内容"}</strong>
              </span>
              {actionError || sourcePreview ? <span dir="auto">{actionError ?? sourcePreview}</span> : null}
            </div>
            <button type="button" title="关闭并静默当前选区" onClick={closePopup}>
              <X size={15} />
              关闭
            </button>
          </>
        ) : isSelectionReady ? (
          <>
            <div
              className="selection-summary selection-drag-surface"
              role="status"
              aria-live="polite"
              title="拖动浮窗"
              onPointerDown={startWindowDrag}
            >
              <strong>{actionError ? "操作失败" : "所选内容"}</strong>
              {actionError || sourcePreview ? <span dir="auto">{actionError ?? sourcePreview}</span> : null}
            </div>
            <button type="button" onClick={copySource} disabled={!state.cleanedText && !state.sourceText}>
              {copied ? <Check size={15} /> : <Clipboard size={15} />}
              {copied ? "已复制" : "复制"}
            </button>
            <button type="button" className="primary-action" onClick={translateSelection} disabled={!state.cleanedText}>
              <Languages size={15} />
              翻译
            </button>
            <button type="button" title="关闭并静默当前选区" onClick={closePopup}>
              <X size={15} />
              关闭
            </button>
          </>
        ) : (
          <>
            <label className="target-language-control">
              <span>目标语言</span>
              <select
                value={targetLanguageSelectValue}
                disabled={state.status === "translating"}
                onChange={(event) => {
                  const nextLanguage = event.target.value;
                  setTargetLanguageInput(nextLanguage === CUSTOM_LANGUAGE_VALUE ? "" : nextLanguage);
                }}
              >
                {LANGUAGE_OPTIONS.map((language) => (
                  <option key={language} value={language}>
                    {language}
                  </option>
                ))}
                <option value={CUSTOM_LANGUAGE_VALUE}>自定义...</option>
              </select>
              {targetLanguageSelectValue === CUSTOM_LANGUAGE_VALUE ? (
                <input
                  value={targetLanguageInput}
                  placeholder="语言"
                  disabled={state.status === "translating"}
                  onChange={(event) => {
                    setTargetLanguageInput(event.target.value);
                  }}
                />
              ) : null}
            </label>
            <button type="button" onClick={copyTranslation} disabled={!state.translation}>
              {copied ? <Check size={15} /> : <Clipboard size={15} />}
              {copied ? "已复制" : "复制"}
            </button>
            {state.status === "error" && state.recoveryAction === "open_settings" ? (
              <button
                type="button"
                onClick={() => openRecovery(desktopApi.openSettings, "打开设置失败。")}
              >
                <Settings size={15} />
                打开设置
              </button>
            ) : null}
            {state.status === "error" && state.recoveryAction === "open_accessibility" ? (
              <button
                type="button"
                onClick={() => openRecovery(desktopApi.openAccessibilitySettings, "打开辅助功能权限失败。")}
              >
                <Settings size={15} />
                打开辅助功能权限
              </button>
            ) : null}
            {state.status === "error" && state.recoveryAction === "open_input_monitoring" ? (
              <button
                type="button"
                onClick={() => openRecovery(desktopApi.openInputMonitoringSettings, "打开输入监听权限失败。")}
              >
                <Settings size={15} />
                打开输入监听权限
              </button>
            ) : null}
            {state.status !== "error" || state.retryable !== false ? (
              <button
                type="button"
                onClick={retryTranslation}
                disabled={!state.cleanedText || state.status === "translating"}
              >
                <RefreshCcw size={15} />
                重译
              </button>
            ) : null}
            {state.status !== "error" || state.retryable !== false ? (
              <button
                type="button"
                onClick={explainTerms}
                disabled={!state.cleanedText || state.status === "translating"}
              >
                <BookOpenText size={15} />
                术语
              </button>
            ) : null}
          </>
        )}
        </footer>
      ) : null}
    </main>
  );
}

function isBrowserPreview(): boolean {
  return typeof window !== "undefined" && !("__TAURI_INTERNALS__" in window);
}

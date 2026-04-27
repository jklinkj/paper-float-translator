import { useEffect, useLayoutEffect, useRef, useState } from "react";
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
  X
} from "lucide-react";
import { DEFAULT_TARGET_LANGUAGE, normalizeTargetLanguage, type PopupState } from "@paper-float-translator/core";

const INITIAL_STATE: PopupState = {
  status: "idle",
  pinned: false
};

const LANGUAGE_OPTIONS = ["中文", "英文", "日文", "韩文", "法文", "德文", "西班牙文"];
const CUSTOM_LANGUAGE_VALUE = "__custom__";

export function PopupView(): JSX.Element {
  const [state, setState] = useState<PopupState>(INITIAL_STATE);
  const [targetLanguageInput, setTargetLanguageInput] = useState<string>(DEFAULT_TARGET_LANGUAGE);
  const [copied, setCopied] = useState(false);
  const rootRef = useRef<HTMLDivElement | null>(null);
  const isSelectionState = state.status === "selection";
  const targetLanguageSelectValue = LANGUAGE_OPTIONS.includes(targetLanguageInput)
    ? targetLanguageInput
    : CUSTOM_LANGUAGE_VALUE;

  useEffect(() => {
    return window.paperFloatTranslator.onPopupState((nextState) => {
      setCopied(false);
      if (nextState.status !== "selection") {
        setTargetLanguageInput(normalizeTargetLanguage(nextState.targetLanguage, DEFAULT_TARGET_LANGUAGE));
      }
      setState(nextState);
    });
  }, []);

  useEffect(() => {
    const listener = (event: KeyboardEvent): void => {
      if (event.key === "Escape") {
        void window.paperFloatTranslator.closePopup();
      }
    };

    window.addEventListener("keydown", listener);
    return () => window.removeEventListener("keydown", listener);
  }, []);

  useLayoutEffect(() => {
    if (!rootRef.current) {
      return;
    }

    const height = rootRef.current.scrollHeight + 2;
    void window.paperFloatTranslator.resizePopup(height);
  }, [state, targetLanguageInput]);

  async function copyTranslation(): Promise<void> {
    await window.paperFloatTranslator.copyTranslation();
    setCopied(true);
  }

  async function copySource(): Promise<void> {
    await window.paperFloatTranslator.copySource();
    setCopied(true);
  }

  async function translateSelection(): Promise<void> {
    await window.paperFloatTranslator.translateSelection();
  }

  async function retryTranslation(): Promise<void> {
    await window.paperFloatTranslator.retryTranslation(normalizeTargetLanguage(targetLanguageInput, state.targetLanguage));
  }

  async function explainTerms(): Promise<void> {
    await window.paperFloatTranslator.explainTerms(normalizeTargetLanguage(targetLanguageInput, state.targetLanguage));
  }

  return (
    <main ref={rootRef} className={isSelectionState ? "popup-shell selection-shell" : "popup-shell"}>
      {!isSelectionState ? (
        <>
          <header className="popup-header">
            <div className="popup-title">
              <BookOpenText size={17} />
              <span>论文翻译</span>
              {state.cached ? <span className="badge">缓存</span> : null}
            </div>
            <div className="popup-tools">
              <button
                type="button"
                title={state.pinned ? "取消固定" : "固定窗口"}
                onClick={() => window.paperFloatTranslator.togglePin()}
              >
                {state.pinned ? <PinOff size={16} /> : <Pin size={16} />}
              </button>
              <button type="button" title="关闭" onClick={() => window.paperFloatTranslator.closePopup()}>
                <X size={16} />
              </button>
            </div>
          </header>

          <section className="popup-body">
            {state.status === "loading" ? (
              <div className="popup-message">
                <Loader2 className="spin" size={20} />
                <span>正在翻译</span>
              </div>
            ) : null}

            {state.status === "error" ? (
              <div className="popup-error">
                <AlertCircle size={18} />
                <span>{state.error ?? "翻译失败。"}</span>
              </div>
            ) : null}

            {state.status === "success" ? (
              <article className="translation-text">{state.translation}</article>
            ) : null}

            {state.status === "idle" ? <div className="popup-message">等待翻译</div> : null}
          </section>
        </>
      ) : null}

      <footer className={isSelectionState ? "popup-actions selection-actions" : "popup-actions"}>
        {isSelectionState ? (
          <>
            <button type="button" onClick={copySource} disabled={!state.cleanedText && !state.sourceText}>
              {copied ? <Check size={15} /> : <Clipboard size={15} />}
              {copied ? "已复制" : "复制"}
            </button>
            <button type="button" className="primary-action" onClick={translateSelection} disabled={!state.cleanedText}>
              <Languages size={15} />
              翻译
            </button>
            <button type="button" onClick={() => window.paperFloatTranslator.closePopup()}>
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
                disabled={state.status === "loading"}
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
                  disabled={state.status === "loading"}
                  onChange={(event) => setTargetLanguageInput(event.target.value)}
                />
              ) : null}
            </label>
            <button type="button" onClick={copyTranslation} disabled={!state.translation}>
              {copied ? <Check size={15} /> : <Clipboard size={15} />}
              {copied ? "已复制" : "复制"}
            </button>
            <button type="button" onClick={retryTranslation} disabled={!state.cleanedText || state.status === "loading"}>
              <RefreshCcw size={15} />
              重译
            </button>
            <button type="button" onClick={explainTerms} disabled={!state.cleanedText || state.status === "loading"}>
              <BookOpenText size={15} />
              术语
            </button>
          </>
        )}
      </footer>
    </main>
  );
}

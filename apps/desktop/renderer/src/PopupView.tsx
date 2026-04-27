import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { AlertCircle, BookOpenText, Check, Clipboard, Loader2, Pin, PinOff, RefreshCcw, X } from "lucide-react";
import type { PopupState } from "@paper-float-translator/core";

const INITIAL_STATE: PopupState = {
  status: "idle",
  pinned: false
};

export function PopupView(): JSX.Element {
  const [state, setState] = useState<PopupState>(INITIAL_STATE);
  const [copied, setCopied] = useState(false);
  const rootRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    return window.paperFloatTranslator.onPopupState((nextState) => {
      setCopied(false);
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
  }, [state]);

  async function copyTranslation(): Promise<void> {
    await window.paperFloatTranslator.copyTranslation();
    setCopied(true);
  }

  return (
    <main ref={rootRef} className="popup-shell">
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

      <footer className="popup-actions">
        <button type="button" onClick={copyTranslation} disabled={!state.translation}>
          {copied ? <Check size={15} /> : <Clipboard size={15} />}
          {copied ? "已复制" : "复制"}
        </button>
        <button type="button" onClick={() => window.paperFloatTranslator.retryTranslation()} disabled={!state.cleanedText}>
          <RefreshCcw size={15} />
          重译
        </button>
        <button type="button" onClick={() => window.paperFloatTranslator.explainTerms()} disabled={!state.cleanedText}>
          <BookOpenText size={15} />
          术语
        </button>
      </footer>
    </main>
  );
}

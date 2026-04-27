import { useEffect, useMemo, useState } from "react";
import {
  AlertCircle,
  Check,
  Eraser,
  ExternalLink,
  KeyRound,
  Loader2,
  Save,
  Settings
} from "lucide-react";
import {
  DEFAULT_SETTINGS,
  type AppSettings,
  type DeepSeekModel,
  type TranslateMode,
  type TriggerMode
} from "@paper-float-translator/core";

const MODEL_OPTIONS: Array<{ value: DeepSeekModel; label: string }> = [
  { value: "deepseek-v4-flash", label: "DeepSeek V4 Flash" },
  { value: "deepseek-v4-pro", label: "DeepSeek V4 Pro" }
];

const MODE_OPTIONS: Array<{ value: TranslateMode; label: string }> = [
  { value: "academic_zh", label: "学术中文" },
  { value: "bilingual", label: "中英对照" },
  { value: "literal", label: "直译" },
  { value: "natural", label: "意译" },
  { value: "terminology", label: "解释术语" }
];

const TRIGGER_MODE_OPTIONS: Array<{ value: TriggerMode; label: string }> = [
  { value: "clipboard_shortcut", label: "先复制，再翻译" },
  { value: "auto_copy_shortcut", label: "自动复制选区" },
  { value: "mac_double_copy", label: "Cmd+C+C 实验" }
];

export function SettingsView(): JSX.Element {
  const [settings, setSettings] = useState<AppSettings>(DEFAULT_SETTINGS);
  const [popupWidthInput, setPopupWidthInput] = useState(String(DEFAULT_SETTINGS.popupWidth));
  const [doubleCopyWindowInput, setDoubleCopyWindowInput] = useState(String(DEFAULT_SETTINGS.doubleCopyWindowMs));
  const [apiKey, setApiKey] = useState("");
  const [hasApiKey, setHasApiKey] = useState(false);
  const [doubleCopyStatus, setDoubleCopyStatus] = useState({ available: true, running: false, message: "" });
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;

    window.paperFloatTranslator
      .getSettings()
      .then((payload) => {
        if (!alive) {
          return;
        }

        setSettings(payload.settings);
        setPopupWidthInput(String(payload.settings.popupWidth));
        setDoubleCopyWindowInput(String(payload.settings.doubleCopyWindowMs));
        setHasApiKey(payload.hasApiKey);
        setDoubleCopyStatus(payload.doubleCopyStatus);
      })
      .catch((loadError: unknown) => {
        setError(loadError instanceof Error ? loadError.message : "读取设置失败。");
      })
      .finally(() => {
        if (alive) {
          setLoading(false);
        }
      });

    return () => {
      alive = false;
    };
  }, []);

  const shortcutHint = useMemo(() => {
    return settings.shortcut.replace("CommandOrControl", "Cmd/Ctrl");
  }, [settings.shortcut]);

  const workflowHint = useMemo(() => {
    if (settings.triggerMode === "auto_copy_shortcut") {
      return `选中文本后按 ${shortcutHint}，程序会尝试自动复制并翻译。`;
    }

    if (settings.triggerMode === "mac_double_copy") {
      return "选中文本后快速按两次 Cmd+C，自动在鼠标附近显示翻译浮窗。";
    }

    return `先按 Cmd+C 复制文本，再按 ${shortcutHint} 翻译当前剪贴板文本。`;
  }, [settings.triggerMode, shortcutHint]);

  async function handleSave(): Promise<void> {
    const popupWidth = parsePopupWidthInput(popupWidthInput);
    const doubleCopyWindowMs = parseDoubleCopyWindowInput(doubleCopyWindowInput);

    if (popupWidth === null) {
      setNotice(null);
      setError("浮窗宽度请输入 320 到 640 之间的数字。");
      return;
    }

    if (doubleCopyWindowMs === null) {
      setNotice(null);
      setError("双复制窗口请输入 400 到 3000 之间的毫秒数。");
      return;
    }

    setSaving(true);
    setNotice(null);
    setError(null);

    try {
      const savedSettings = await window.paperFloatTranslator.saveSettings({
        ...settings,
        popupWidth,
        doubleCopyWindowMs
      });
      setSettings(savedSettings);
      setPopupWidthInput(String(savedSettings.popupWidth));
      setDoubleCopyWindowInput(String(savedSettings.doubleCopyWindowMs));

      if (apiKey.trim()) {
        const result = await window.paperFloatTranslator.saveApiKey(apiKey);
        setHasApiKey(result.hasApiKey);
        setApiKey("");
      }

      const refreshed = await window.paperFloatTranslator.getSettings();
      setHasApiKey(refreshed.hasApiKey);
      setDoubleCopyStatus(refreshed.doubleCopyStatus);
      setNotice("设置已保存。");
    } catch (saveError) {
      setError(saveError instanceof Error ? saveError.message : "保存设置失败。");
    } finally {
      setSaving(false);
    }
  }

  async function handleClearApiKey(): Promise<void> {
    setSaving(true);
    setNotice(null);
    setError(null);

    try {
      const result = await window.paperFloatTranslator.saveApiKey("");
      setHasApiKey(result.hasApiKey);
      setApiKey("");
      setNotice("API Key 已清除。");
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
      await window.paperFloatTranslator.clearCache();
      setNotice("缓存已清空。");
    } catch (clearError) {
      setError(clearError instanceof Error ? clearError.message : "清空缓存失败。");
    } finally {
      setSaving(false);
    }
  }

  if (loading) {
    return (
      <main className="settings-page centered">
        <Loader2 className="spin" size={22} />
        <span>正在读取设置</span>
      </main>
    );
  }

  return (
    <main className="settings-page">
      <header className="settings-header">
        <div>
          <h1>Paper Float Translator</h1>
          <p>{workflowHint}</p>
        </div>
        <button
          className="icon-link"
          type="button"
          onClick={() => window.paperFloatTranslator.openExternal("https://api-docs.deepseek.com/")}
          title="打开 DeepSeek API 文档"
        >
          <ExternalLink size={18} />
        </button>
      </header>

      <section className="settings-grid">
        <div className="panel">
          <div className="panel-heading">
            <KeyRound size={18} />
            <h2>DeepSeek</h2>
          </div>

          <label className="field">
            <span>API Key</span>
            <input
              type="password"
              value={apiKey}
              placeholder={hasApiKey ? "已保存在系统钥匙串，留空则不修改" : "输入 DeepSeek API Key"}
              onChange={(event) => setApiKey(event.target.value)}
            />
          </label>

          <label className="field">
            <span>默认模型</span>
            <select
              value={settings.model}
              onChange={(event) => setSettings({ ...settings, model: event.target.value as DeepSeekModel })}
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
              value={settings.mode}
              onChange={(event) => setSettings({ ...settings, mode: event.target.value as TranslateMode })}
            >
              {MODE_OPTIONS.map((option) => (
                <option key={option.value} value={option.value}>
                  {option.label}
                </option>
              ))}
            </select>
          </label>
        </div>

        <div className="panel">
          <div className="panel-heading">
            <Settings size={18} />
            <h2>桌面行为</h2>
          </div>

          <label className="field">
            <span>取词方式</span>
            <select
              value={settings.triggerMode}
              onChange={(event) => setSettings({ ...settings, triggerMode: event.target.value as TriggerMode })}
            >
              {TRIGGER_MODE_OPTIONS.map((option) => (
                <option key={option.value} value={option.value}>
                  {option.label}
                </option>
              ))}
            </select>
            {settings.triggerMode === "clipboard_shortcut" ? (
              <small>推荐：PDF 里先手动复制，避免读到旧剪贴板。</small>
            ) : null}
            {settings.triggerMode === "auto_copy_shortcut" ? (
              <small>备用：需要 macOS 辅助功能权限，部分 PDF 阅读器可能不稳定。</small>
            ) : null}
            {settings.triggerMode === "mac_double_copy" ? (
              <small>{doubleCopyStatus.message || "macOS 实验功能：快速复制同一段文字两次后触发。"}</small>
            ) : null}
          </label>

          {settings.triggerMode === "mac_double_copy" ? (
            <label className="field">
              <span>双复制触发窗口</span>
              <input
                type="number"
                min={400}
                max={3000}
                step={100}
                value={doubleCopyWindowInput}
                onChange={(event) => setDoubleCopyWindowInput(event.target.value)}
              />
              <small>范围 400-3000ms，默认 1200ms。</small>
            </label>
          ) : null}

          <label className="field">
            <span>全局快捷键</span>
            <input
              value={settings.shortcut}
              onChange={(event) => setSettings({ ...settings, shortcut: event.target.value })}
            />
          </label>

          <label className="field">
            <span>浮窗宽度</span>
            <input
              type="number"
              min={320}
              max={640}
              step={20}
              value={popupWidthInput}
              onChange={(event) => setPopupWidthInput(event.target.value)}
            />
            <small>范围 320-640，保存时生效。</small>
          </label>

          <label className="switch-row">
            <input
              type="checkbox"
              checked={settings.cleanPdfText}
              onChange={(event) => setSettings({ ...settings, cleanPdfText: event.target.checked })}
            />
            <span>自动清洗 PDF 换行和断词</span>
          </label>

          <label className="switch-row">
            <input
              type="checkbox"
              checked={settings.enableCache}
              onChange={(event) => setSettings({ ...settings, enableCache: event.target.checked })}
            />
            <span>启用本地翻译缓存</span>
          </label>
        </div>
      </section>

      <footer className="settings-actions">
        <div className="status-line">
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
          <button type="button" className="secondary" onClick={handleClearApiKey} disabled={saving || !hasApiKey}>
            <KeyRound size={16} />
            清除 Key
          </button>
          <button type="button" className="primary" onClick={handleSave} disabled={saving}>
            {saving ? <Loader2 size={16} className="spin" /> : <Save size={16} />}
            保存设置
          </button>
        </div>
      </footer>
    </main>
  );
}

function parsePopupWidthInput(value: string): number | null {
  if (!value.trim()) {
    return null;
  }

  const parsed = Number(value);

  if (!Number.isFinite(parsed)) {
    return null;
  }

  return Math.round(parsed);
}

function parseDoubleCopyWindowInput(value: string): number | null {
  if (!value.trim()) {
    return null;
  }

  const parsed = Number(value);

  if (!Number.isFinite(parsed) || parsed < 400 || parsed > 3000) {
    return null;
  }

  return Math.round(parsed);
}

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
import { DEFAULT_SETTINGS, type AppSettings, type DeepSeekModel, type TranslateMode } from "@paper-float-translator/core";

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

export function SettingsView(): JSX.Element {
  const [settings, setSettings] = useState<AppSettings>(DEFAULT_SETTINGS);
  const [popupWidthInput, setPopupWidthInput] = useState(String(DEFAULT_SETTINGS.popupWidth));
  const [apiKey, setApiKey] = useState("");
  const [hasApiKey, setHasApiKey] = useState(false);
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
        setHasApiKey(payload.hasApiKey);
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

    try {
      const savedSettings = await window.paperFloatTranslator.saveSettings({
        ...settings,
        popupWidth
      });
      setSettings(savedSettings);
      setPopupWidthInput(String(savedSettings.popupWidth));

      if (apiKey.trim()) {
        const result = await window.paperFloatTranslator.saveApiKey(apiKey);
        setHasApiKey(result.hasApiKey);
        setApiKey("");
      }

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
          <p>选中文本后按 {shortcutHint}，在鼠标附近显示论文翻译浮窗。</p>
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

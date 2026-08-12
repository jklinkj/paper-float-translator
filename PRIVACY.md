# Privacy policy

Last updated: 2026-08-13

Paper Float Translator is an open-source desktop application. This policy
describes the behavior of the official source code and release builds published
from <https://github.com/jklinkj/paper-float-translator>.

## Short version

- Selection detection, popup display, settings, and glossary handling happen on
  your computer.
- The project operates no telemetry, analytics, advertising, or translation
  proxy server.
- Selected text may be sent directly to DeepSeek only after you deliberately
  choose a translation or terminology action, or perform the configured
  `Cmd+C+C` / `Ctrl+C+C` double-copy translation gesture. Merely selecting text,
  copying once, or opening the action popup does not send it to DeepSeek.
- A translation request includes the selected text, target language, translation
  mode, model name, and any configured glossary entries. Your API key is sent to
  DeepSeek in the request's authorization header.
- Translation cache is local, optional, limited to 500 entries, and expires after
  seven days. You can disable or clear it in Settings.

## Data processed on your device

The app may process the following data locally to provide its features:

- text exposed as the current selection by macOS Accessibility or Windows UI
  Automation;
- text placed on the clipboard by you when using the deliberate `Cmd+C+C` or
  `Ctrl+C+C` fallback;
- app settings, including trigger and cache preferences;
- glossary entries;
- selected source text and translated text in the optional translation cache;
- runtime capability status and content-free acceptance diagnostics; and
- a DeepSeek API key stored in the operating system credential store.

Automatic capture never simulates a copy action and never rewrites the clipboard.
The double-copy fallback observes clipboard changes only after the user performs
real copy gestures. The app keeps the current selection in memory long enough to
show the action or result popup.

## Network transfers

The desktop app has no project-operated backend. It may transfer translation
data directly to `https://api.deepseek.com` only after an explicit translation
action: clicking a translation or terminology control, or deliberately performing
the configured `Cmd+C+C` / `Ctrl+C+C` double-copy gesture. A valid local cache hit
is returned without a DeepSeek request. When a request is made, it contains:

- the selected text;
- a system instruction derived from the selected mode and target language;
- configured glossary entries, when present;
- the configured DeepSeek model; and
- the API key in an authorization header.

DeepSeek returns the generated translation directly to the app. DeepSeek is an
independent third-party service and processes requests under its own terms and
privacy practices. Review the [DeepSeek privacy policy](https://cdn.deepseek.com/policies/en-US/deepseek-privacy-policy.html),
[DeepSeek Chinese privacy policy](https://cdn.deepseek.com/policies/zh-CN/deepseek-privacy-policy.html),
and [DeepSeek Open Platform terms](https://cdn.deepseek.com/policies/en-US/deepseek-open-platform-terms-of-service.html)
before sending confidential, personal, regulated, or third-party text.

The app opens documentation or policy links only when you ask it to. On Windows,
the installer may download the Microsoft Edge WebView2 runtime from Microsoft if
the required system runtime is absent. The project does not control operating
system network activity such as certificate validation.

## Local storage and retention

Production builds store `settings.json`, `cache.json`, and `glossary.json` below
the platform application-data directory:

- Windows: `%APPDATA%\com.paperfloat.translator`
- macOS: `~/Library/Application Support/com.paperfloat.translator`

When cache is enabled, entries can include source text, translation, model, mode,
target language, glossary-version hash, and creation time. The app keeps at most
500 entries and removes entries older than seven days. Settings and glossary data
remain until you change or delete them.

Windows production builds store the API key in Windows Credential Manager.
macOS production builds store it in Keychain. A separately identified local
macOS diagnostic build may store the key in a mode-0600 file within its app-data
directory; that feature is not enabled in formal releases.

## Your controls

You can:

- leave the API key unconfigured to prevent translation requests;
- turn selection popup and experimental automatic selection features off;
- turn local translation cache off;
- use **Clear cache** to erase cached source text and translations; and
- use **Clear Key** to remove the API key from the operating system credential
  store.

The normal uninstaller removes the application but intentionally preserves local
settings for upgrades and reinstalls. Before uninstalling, use **Clear cache** and
**Clear Key**. For complete removal, also delete the application-data directory
listed above. This behavior is documented in the [Windows release guide](docs/WINDOWS_RELEASE.md).

## Permissions

Selection capture requires operating-system accessibility or UI Automation
capabilities. On macOS, optional selection detection can require Accessibility
and Input Monitoring permission. On Windows, deliberate selection capture uses
UI Automation, and experimental automatic selection is a separate opt-in feature.
You can revoke permissions in operating-system settings; translation fallback and
feature availability may then be reduced.

## Project communications

The app itself sends no telemetry to project maintainers. If you file a GitHub
issue, discussion, or security report, GitHub processes the information you choose
to submit under GitHub's own policies. Do not include API keys, confidential text,
or personal data in public reports.

## Changes and questions

Material changes to app data flows must update this file in the same pull request.
Privacy questions may be opened as a GitHub issue without including sensitive
content. Security-sensitive reports should use the private process in
[SECURITY.md](SECURITY.md).

---

# 隐私政策（中文）

更新日期：2026-08-13

Paper Float Translator 是开源桌面软件。本政策适用于从项目官方仓库
<https://github.com/jklinkj/paper-float-translator> 构建和发布的软件。

## 一句话说明

- 选区检测、浮窗、设置和术语表都在本机处理。
- 项目不运营遥测、统计、广告或翻译中转服务器。
- 仅当你主动点击“翻译”或“解释术语”，或者主动执行已配置的
  `Cmd+C+C` / `Ctrl+C+C` 双复制翻译手势时，所选文本才可能直接发送给
  DeepSeek；仅仅划词、单次复制或显示操作浮窗不会发起 DeepSeek 请求。
- 请求会包含所选文本、目标语言、翻译模式、模型名称及已配置的术语表；
  API Key 会作为授权信息发送给 DeepSeek。
- 翻译缓存保存在本机，可以关闭或清空，最多 500 条，7 天后过期。

## 本机处理的数据

软件会在本机处理当前选区、你主动复制后由 `Cmd+C+C` 或 `Ctrl+C+C`
读取的剪贴板文本、设置、术语表、本地翻译缓存、内容无关的运行诊断状态，
以及保存在系统凭据库中的 DeepSeek API Key。软件不会模拟复制，也不会改写
剪贴板。

## 网络传输

软件没有项目方运营的后端。只有在你主动点击翻译或术语解释操作，或者主动
执行已配置的 `Cmd+C+C` / `Ctrl+C+C` 双复制翻译手势后，软件才可能直接向
`https://api.deepseek.com` 发送请求；命中有效本地缓存时不会请求 DeepSeek。
实际发出的请求包含所选文本、由目标语言和模式生成的系统提示、已配置的术语表、
模型名称和授权用 API Key。DeepSeek 是独立第三方服务；发送机密、个人、受监管
或他人文本前，请阅读
[DeepSeek 中文隐私政策](https://cdn.deepseek.com/policies/zh-CN/deepseek-privacy-policy.html)
和其开放平台条款。

只有你主动点击时，软件才会打开外部文档或政策链接。如果 Windows 缺少
Microsoft Edge WebView2，安装器可能从 Microsoft 下载该系统运行时。

## 本地保存与删除

正式版将 `settings.json`、`cache.json` 和 `glossary.json` 保存到：

- Windows：`%APPDATA%\com.paperfloat.translator`
- macOS：`~/Library/Application Support/com.paperfloat.translator`

Windows 正式版把 API Key 保存到 Windows 凭据管理器；macOS 正式版保存到
钥匙串。你可以在设置中关闭或清空缓存，并使用“清除 Key”删除系统凭据。

普通卸载默认保留设置，便于升级或重装。彻底移除前，请先在软件内执行
“清空缓存”和“清除 Key”，卸载后再删除上述应用数据目录。详细步骤见
[Windows 发布与卸载说明](docs/WINDOWS_RELEASE.md)。

## 权限与联系

选区读取需要操作系统提供的辅助功能或 UI Automation 能力；你可以在系统设置
中撤回权限。软件不会向项目维护者发送遥测。如果你在 GitHub 提交问题，请勿
附上 API Key、机密原文或个人信息；安全问题请按 [SECURITY.md](SECURITY.md)
私下报告。

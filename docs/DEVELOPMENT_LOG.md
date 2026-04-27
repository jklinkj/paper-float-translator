# Development Log

## 2026-04-27

### Initial Architecture And Desktop MVP

完成 Paper Float Translator 第一版桌面 MVP 的项目落地。

主要内容：

- 创建 Electron + Vite + React + TypeScript monorepo。
- 固定项目计划到 `docs/PROJECT_PLAN.md`。
- 建立 `apps/desktop`、`packages/core`、`packages/deepseek`、`packages/storage` 模块边界。
- 实现 Electron main/preload/renderer 进程分层。
- 实现全局快捷键触发翻译链路：
  - 读取鼠标位置。
  - 显示 loading 浮窗。
  - 模拟复制当前选区。
  - 读取并恢复剪贴板。
  - 清洗 PDF 断词、换行和多余空格。
  - 查询本地缓存。
  - 调用 DeepSeek API。
  - 在鼠标附近展示译文浮窗。
- 实现 React 设置页：
  - DeepSeek API Key。
  - 模型选择。
  - 翻译模式。
  - 快捷键。
  - PDF 清洗开关。
  - 缓存开关。
- 实现 React 浮窗：
  - loading、译文、错误状态。
  - 复制译文。
  - 固定窗口。
  - 重新翻译。
  - 解释术语。
  - Esc 关闭。
- 实现 DeepSeek adapter：
  - 默认 `https://api.deepseek.com`。
  - 默认模型 `deepseek-v4-flash`。
  - 支持 `deepseek-v4-pro`。
  - 请求中显式关闭 thinking。
- 实现本地存储：
  - 设置使用 JSON 文件。
  - 缓存使用 JSON KV。
  - 术语表使用 JSON。
  - API Key 优先使用系统钥匙串 `keytar`，不可用时 fallback 到 Electron `safeStorage` 加密文件。
- 补充核心单元测试：
  - 文本清洗。
  - cache key。
  - DeepSeek 请求体。
  - 设置和缓存存储。

验证结果：

- `npm run test` 通过，12 tests。
- `npm run typecheck` 通过。
- `npm run build` 通过。

当前限制：

- 当前依赖安装使用过 `npm install --ignore-scripts`，因此本机 Electron 二进制 postinstall 可能尚未完成。
- 如需启动桌面开发版，需要在网络可用时运行 `npm rebuild electron` 或普通 `npm install`。
- macOS 上模拟复制需要给运行进程授予辅助功能权限。
- Linux 分支依赖 `xdotool`，Windows 分支使用 PowerShell SendKeys，后续需要真实平台验证。

### Mac Runtime Bring-Up

完成 macOS 本机运行环境打通和第一轮真实桌面验证。

主要内容：

- 使用 Electron 镜像完成 `npm rebuild electron`，解决 GitHub 下载 Electron 二进制连接失败的问题。
- 确认 `node -e "console.log(require('electron'))"` 能返回 macOS Electron 可执行文件路径。
- 启动 `npm run dev`，确认设置页能在 Electron 窗口中真实渲染。
- 使用 Computer Use 打开 TextEdit，创建并选中英文测试句子。
- 发现当前系统未授予自动按键所需的 macOS 辅助功能权限，`osascript` 不能发送按键。
- 在 Electron main process 中增加 macOS 辅助功能权限预检，未授权时直接在浮窗中显示明确处理路径。
- 在应用菜单中增加 `Translate Selection` 入口，方便开发期触发同一条翻译链路。

本轮验证结果：

- Electron 二进制安装成功。
- `npm run dev` 可以启动桌面 App。
- 设置页渲染正常。
- TextEdit 外部选区可由 Computer Use 创建和选中。
- 当前机器需要手动授予辅助功能权限后，才能完成模拟 `Cmd+C` 和全局快捷键端到端验证。
- 浮窗窗口已由人工确认可见。
- 菜单入口 `Translate Selection` 已验证会触发翻译链路并创建可见浮窗。

### Popup Width And PDF Selection Reliability Fixes

修复手动测试发现的两个问题。

主要内容：

- 修复浮窗宽度输入在编辑过程中可能把空值解析成 `0`，保存后被归一化为最小宽度 `320` 的问题。
- 设置页浮窗宽度改为字符串编辑，保存时再解析和校验。
- 设置存储层支持从 IPC 或旧配置中接收数字字符串形式的 `popupWidth`。
- 保存设置后，如果浮窗已经存在，会立即按新宽度调整并重新定位。
- 选区读取从固定等待 `140ms` 改为剪贴板 sentinel + 最多 `1600ms` 轮询，提升 PDF 阅读器慢复制时的成功率。
- 将 `API KEY.md` 加入 `.gitignore`，避免本地密钥文件被误提交。

验证结果：

- `npm run test` 通过，13 tests。
- `npm run typecheck` 通过。
- `npm run build` 通过。
- 真实 UI 验证：将浮窗宽度从 `320` 改为 `520` 并保存后，设置页保持 `520`，不再跳回 `320`。

### Clipboard-First Trigger Refactor

重构取词触发逻辑，避免 PDF 自动复制失败时读到旧剪贴板并命中旧缓存。

主要内容：

- 默认取词方式改为 `clipboard_shortcut`：用户先手动 `Cmd+C` 复制，再按翻译快捷键读取当前剪贴板翻译。
- 保留旧自动复制逻辑为 `auto_copy_shortcut` 备用模式。
- 增加 `mac_double_copy` 实验模式，通过 macOS `NSPasteboard.changeCount` 监听快速双复制。
- 设置页增加“取词方式”和“双复制触发窗口”配置。
- 触发模式加入配置归一化，旧配置会自动补默认值。
- 翻译完成后不恢复旧剪贴板，保留用户刚复制的原文。
- 针对剪贴板为空、非文本、自动复制失败、双复制失败分别给出更清晰的错误提示。

验证目标：

- PDF 中先复制再翻译时，不再读取上一段剪贴板文本。
- 同一段重复翻译仍可正常命中缓存。
- 自动复制模式仍保留给需要一键触发的场景。
- macOS 双复制模式作为实验功能，不影响默认快捷键模式。

### Cmd+C+C Only Trigger And Low-Latency Watcher

收敛取词入口，降低 macOS 双复制触发延迟。

主要内容：

- 移除全局翻译快捷键、取词方式选择和自动模拟复制路径。
- App 启动后常驻 macOS `NSPasteboard.changeCount` watcher。
- 双复制 watcher 轮询间隔从 `250ms` 降到 `50ms`。
- 双复制触发窗口固定为 `900ms`，不再暴露为设置项。
- 对同一文本加入短暂触发冷却，避免多连复制重复请求。
- 启动时预热隐藏浮窗，降低首次触发的窗口加载延迟。
- 设置页只保留 API Key、模型、翻译模式、浮窗宽度、PDF 清洗、缓存和双复制监听状态。
- 设置类型移除 `shortcut`、`triggerMode`、`doubleCopyWindowMs`，旧配置中的这些字段会被忽略。

验证目标：

- 选中文本后快速按两次 `Cmd+C`，第二次复制后应很快显示 loading 浮窗。
- 不再需要 `Cmd+Shift+Y` 或辅助功能权限。
- 翻译完成后剪贴板仍保留用户复制的原文。

### Terminology Output Normalization

规范化浮窗“术语”按钮输出，避免模型返回 Markdown。

主要内容：

- 收紧 `terminology` 模式 prompt，明确禁止 Markdown、标题、加粗、编号、项目符号和表格。
- 固定术语输出格式为 `英文术语：中文译名。说明：一句话解释。`。
- 在 DeepSeek adapter 中仅对 `terminology` 模式做本地兜底清洗。
- 移除常见包装句，如“关键术语解释”“推荐译法”等。
- 将 `english (中文)：解释` 归一化为纯文本目标格式。

验证目标：

- 点击“术语”按钮后，浮窗不再出现 `**`、Markdown 编号、标题或总结句。
- 普通翻译模式不做 Markdown 清洗，避免破坏正常译文。

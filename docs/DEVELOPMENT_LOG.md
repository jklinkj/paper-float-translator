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

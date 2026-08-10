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

### Mac Selection Action Popup

新增 macOS 自动选区操作浮窗，降低用户触发翻译前的操作成本。

主要内容：

- 新增 macOS Accessibility selection watcher，通过全局鼠标拖选/双击结束事件尝试读取当前前台应用选中文本。
- 后续移除轻量轮询触发，避免拖选过程中提前弹出浮窗。
- 选区读取从 focused element 扩展到鼠标下元素、父链、focused window 和有限子树搜索。
- 自动选区读取成功后，只显示操作浮窗，不立即请求 DeepSeek。
- 浮窗新增 `selection` 状态，提供“复制 / 翻译 / 关闭”。
- 点击“翻译”后复用现有缓存、DeepSeek 请求、错误展示和结果浮窗链路。
- 设置页增加自动选区监听状态和辅助功能权限入口，并保留 `Cmd+C+C` 双复制兜底监听状态。
- 无 macOS 辅助功能权限时只在设置页提示，不打断阅读，`Cmd+C+C` 仍可继续使用。

验证目标：

- TextEdit 和浏览器中拖选文本后，应自动出现操作浮窗。
- 点击“复制”只写入选区原文到剪贴板，不请求 DeepSeek。
- 点击“翻译”后进入现有 loading / success / error 流程。
- 不支持 Accessibility 选区读取的应用中，`Cmd+C+C` 兜底触发仍可用。

### Selection Popup Timing And Dismiss Behavior

修复自动选区浮窗在拖选过程中提前出现，以及浮窗关闭过于依赖关闭按钮的问题。

主要内容：

- 移除自动选区 watcher 的常驻轮询触发，只在鼠标拖选松开后读取选区。
- 全局鼠标监听从 `NSEvent.addGlobalMonitor` 改为 `CGEventTap`，提升拖选和外部点击捕获可靠性。
- selection 事件携带鼠标松开坐标，浮窗使用固定锚点定位，不再跟随后续鼠标位置移动。
- selection 浮窗改为紧凑按钮条，只显示“复制 / 翻译 / 关闭”，不再展示选中文本内容。
- 未固定浮窗会主动获得焦点，点击外部可通过失焦或全局鼠标按下事件自动隐藏；固定后的翻译结果浮窗保持可见。

验证目标：

- 拖选过程中不出现浮窗，鼠标松开后才显示按钮条。
- 松开后移动鼠标，浮窗位置不跳动。
- 点击浮窗外部，未固定浮窗自动关闭。

### Selection Lifecycle Suppression

修复已处理选区在结果浮窗关闭后重新弹出的问题。

主要内容：

- main process 增加自动选区抑制状态，记录已经处理过的 `cleanedText`。
- 点击 selection 浮窗的“复制 / 翻译 / 关闭”后，会 suppress 当前选区。
- 未固定浮窗因外部点击或失焦自动隐藏时，也会 suppress 与当前 selection 生命周期关联的文本。
- watcher 后续读到同一段已处理选区时直接忽略，不再弹出 selection 小浮窗。
- 读到空选区或不同文本时解除旧 suppress，允许新选区正常弹窗。

## 2026-07-19

### Selection Reliability Remediation

因鼠标划词监听长期无法稳定工作，启动专项修复；唯一执行基线为 `docs/SELECTION_RELIABILITY_REMEDIATION_PLAN_2026-07-19.md`，独立只读审查任务为 `019f7684-5af2-7cc1-995d-e89728372f10`。

本轮完成：

- 将 Accessibility、Listen Event 授权、mouse/key tap、AX selectedText observer、直接读取与剪贴板回退拆为独立强类型能力；未知状态 fail closed，并按具体缺失能力显示系统设置入口。
- 原生监听改为鼠标事件与 AXObserver 并行；支持拖选、双击、三击、Shift/键盘选区；冻结 mouse-up PID/AX element/window/CG anchor；generation 取消旧重试；同文本的新手势不再被永久去重。
- 处理 event tap 被系统禁用后的恢复/降级并过滤自身进程。key tap `Ready` 时 selection 弹窗保持原应用焦点；其余 health fail-safe 聚焦，运行中降级也会补入口。冻结 source PID 随 selection 事件进入 Rust，首个 Tab/Shift+Tab 按 revision 落到首/末控件，关闭时仅在 popup 确实聚焦且 revision/前台条件匹配时恢复来源 App。
- 建立 `protocolVersion + revision + selectionRevision` 权威快照协议和六态 popup 状态机；前端先订阅后快照并按 revision 合并；关闭、新选区、禁用和退出均使旧翻译失效。
- 增加精确 `errorKind + retryable + recoveryAction`，区分设置、Accessibility、Input Monitoring、重新选择和翻译重试；所有弹窗异步操作失败均提供可见反馈。
- 将窗口几何与文本清洗拆成独立 Rust 模块；统一负坐标、目标显示器、物理/逻辑像素和 hit-test；修复 pending 网格、66px 裁切、长词、RTL、emoji 与高内容状态无法缩回。
- 设置页区分草稿、已保存和当前生效；初次加载失败阻止编辑并可重试；设置/Key/状态刷新部分成功分别反馈；从任何真实失焦返回时自动刷新权限。
- watcher context 通过显式 release 事件精确回收；缓存采用进程内锁、原子临时文件 rename 和 500 项上限；API Key 使用 Security.framework Keychain API，不再通过 CLI 参数传递或先删后写。
- 明确关闭设置只隐藏、后台监听继续、Dock 重开、Cmd+Q 完全退出的生命周期。

固定验证结果：

- `npm run quality`：53/53 Vitest、23/23 Playwright UI、renderer production build、89/89 Rust、Cargo fmt/check 与严格 clippy 全部通过。
- UI 自动化覆盖 hidden/pending/ready/translating/translated/error、精确恢复入口、协议/按钮失败、Tab/Shift+Tab/Space/Escape、320/379/380/420/640px、长文/无断点/CJK/emoji/多行/RTL，以及设置加载、校验、取消、完整失败、两类部分成功和权限组合。
- 原生 bridge 在 arm64/x86_64 下均以 `-Wall -Wextra -Werror` 编译；50 次 restart smoke：`starts=51`、`releases=51`、`callbackAfterRelease=0`、lifecycle 2–52 单调。
- 固定 Safari HTML 与 Preview PDF fixture 可选择且 SHA-256 记录于 `apps/desktop/tests/fixtures/README.md`；PDF 连续两次生成哈希一致并完成视觉检查。
- 安全升级到 Vite 6.4.3、Vitest 3.2.7、Babel 7.29.7；`npm audit` 的生产与完整依赖均为 0 漏洞。
- ad-hoc arm64 `.app` 按本轮源码重建并通过 `codesign --verify` 与 Designated Requirement；CDHash 为 `a93a10facb634e91d45cbe0bec5e4aabcf5913f4`。

独立审查第 1 轮修复：

- 审查任务独立复跑质量门禁、原生 harness、依赖审计、固定 fixture/PDF 与签名检查；结论为 `REJECTED`，未发现新增确定性 P0，提出 `REV-14` 和 `REV-15` 两项 P1。
- `REV-14`：UI 边界 helper 补齐 shell bottom 与 control bottom 约束，并加入将 shell/control 分别下移 600px 的负向哨兵；哨兵在缺陷存在时为 false，修复后完整 23 项 Playwright 通过。
- `REV-15`：原生 callback ABI 透传冻结选区 `targetPID`；能力策略覆盖六种 popup 状态 × 五种 key-tap health；AX ready + key tap unavailable、disabled fallback → recovered、旧 revision、外部点击、重复关闭和 focus return 均有确定性断言。
- 本应用窗口激活时 workspace observer 不再卸载外部应用的 AXObserver；来源 App 已退出或第三方 App 已取得前台时，恢复焦点安全地 no-op，不抢用户当前焦点。

独立审查第 2 轮结果：

- 结论仍为 `REJECTED`，原因是规划 §8.1/§9 的真实 Tauri GUI/TCC 量化验收尚未执行；审查者明确确认本轮未发现新的确定性代码 P0/P1。
- 审查前后快照一致：tracked `926141f7…cade5`、untracked `101b1915…5c86`、40 个未跟踪文件；所有会生成产物的复验均在 `/tmp` 隔离副本完成。
- 独立复跑 53/53 Vitest、23/23 Playwright、89/89 Rust、renderer build、fmt/check/clippy、双架构 native 与 50 次 restart 全部通过；依赖审计仍为 0 漏洞。
- 审查者分别移除 shell bottom 与 control bottom 条件构造两个 mutant；两个 mutant 都使对应负向哨兵按预期失败，因此 `REV-14` 正式通过。
- `REV-15` 的冻结 PID ABI、capability 驱动焦点、首个 Tab/Shift+Tab、revision 防旧事件和一次性 focus return 在代码与自动化层面通过；真实 key tap Ready、unavailable+AX ready、disabled→recovered 及关闭后第二次 AX 选区仍必须实机验证。

仍未伪造为通过的发布/环境门槛：

- 当前钥匙串没有 Developer ID（`0 valid identities`），所以 ad-hoc 包不能证明稳定 TCC 身份、Gatekeeper、notarization 或覆盖安装后的权限持久性。
- 当前机器没有第二显示器，真实负 X/上下排列/混合缩放保留为发布前人工门槛；自动化几何测试为必需但不冒充实机证据。
- TextEdit、Safari 固定 HTML 和 Preview 固定 PDF 的 30 次量化拖选/双击/三击/Shift/键盘、延迟和自身隔离，需要在用户确认运行本地诊断包及系统权限操作后执行。
- Swift watcher 不再在每次 mouse down 时清空 `lastEmittedSelection`，避免旧选区被重新制造成新 selection。

验证目标：

- 点击“翻译”并关闭结果浮窗后，同一段仍保持选中时 selection 小浮窗不回弹。
- 点击 selection 浮窗的“复制”或“关闭”后，同一段不会反复弹出。
- 重新拖选另一段文本时，selection 小浮窗正常出现。

### Selection Popup Toggle

增加“拖选后显示操作浮窗”设置项，默认开启。

主要内容：

- `AppSettings` 增加 `enableSelectionPopup`，旧配置文件缺失该字段时归一化为 `true`。
- 设置页在“桌面行为”区域增加开关；关闭后状态文案提示仍可使用 `Cmd+C+C`。
- main process 保留 macOS selection watcher，但在开关关闭时忽略 watcher 发来的 selection 事件。
- `Cmd+C+C` 双复制兜底、翻译结果浮窗和未固定浮窗外部点击关闭逻辑保持不变。

验证目标：

- 默认开启时拖选文本松开后显示 `复制 / 翻译 / 关闭` 小浮窗。
- 关闭并保存后，拖选文本不再显示小浮窗。
- 关闭后快速按两次 `Cmd+C` 仍可触发翻译。

### Result Popup Target Language

增加结果浮窗目标语言切换，不改变拖选操作浮窗的轻量形态。

主要内容：

- `AppSettings` 增加 `targetLanguage`，默认 `中文`，旧配置缺失时自动补默认值。
- 设置页增加“默认目标语言”，拖选翻译和 `Cmd+C+C` 直接使用该默认语言。
- DeepSeek prompt、请求类型、缓存 key 和缓存条目都增加目标语言，避免不同语言互相命中缓存。
- selection 小浮窗仍只显示“复制 / 翻译 / 关闭”；结果浮窗提供目标语言输入，点击“重译”或“术语”时使用当前输入。

验证目标：

- 默认语言为中文时，拖选后点击“翻译”和 `Cmd+C+C` 都直接输出中文。
- 结果浮窗中改成日文后点击“重译”，应请求并显示日文结果。
- 同一文本的中文和日文翻译不会互相命中缓存。

### Any Source Language Prompt

修复 prompt 仍假设源语言为英文的问题，让 DeepSeek 自动识别原文语言。

主要内容：

- 系统身份从“英文学术论文翻译助手”改为“学术内容翻译助手”。
- 普通翻译、双语对照和术语解释 prompt 移除“英文论文内容”“英文原文”“英文术语”等源语言假设。
- 明确要求自动识别用户提供的原文语言，并翻译为目标语言。
- 明确禁止因为原文不是英文而要求用户重新提供英文文本。

验证目标：

- 中文原文翻译成日文时直接输出日文，不再提示需要英文文本。
- 英文论文原文翻译成中文时保持原有学术翻译效果。

### Tauri v2 Desktop Shell Migration

将桌面壳从 Electron 迁移到 Tauri v2，保留当前已验证的 macOS MVP 行为。

主要内容：

- `apps/desktop` 改为 Tauri v2 + React/Vite + TypeScript + Rust backend。
- 删除 Electron 入口、preload、`electron-vite` 配置和桌面端 Electron 依赖。
- 新增 `src-tauri`，由 Rust backend 管理设置、Keychain、缓存、DeepSeek 请求、浮窗窗口和 watcher 生命周期。
- React renderer 改为通过 Tauri `invoke` / `listen` 调用桌面能力，不再暴露 Node 或 API Key。
- 保留 macOS selection watcher 和 pasteboard watcher 的 Swift 脚本逻辑，由 Rust 子进程启动和解析 stdout。
- 保留自动拖选操作浮窗、`Cmd+C+C` 兜底、目标语言切换、术语解释、缓存、固定窗口和外部点击关闭。
- API Key 仅写入 macOS Keychain，不再做明文或前端 fallback。
- 保留 `packages/deepseek` 和 `packages/storage` 作为历史兼容层；Tauri 运行时不再引用，待确认无复用后删除。
- 移除未使用的 Tauri clipboard/store 插件依赖和 capability 权限，当前剪贴板、设置、缓存分别由 Rust `arboard`、JSON 文件和 Keychain 实现。
- 更新 `docs/PROJECT_PLAN.md`，将当前运行架构固定为 Tauri backend + React renderer。

验证目标：

- `npm run dev` 启动 Tauri 设置窗口和隐藏浮窗窗口。
- 拖选文本后自动显示 `复制 / 翻译 / 关闭` 操作浮窗。
- `Cmd+C+C` 仍可触发翻译。
- 设置页可保存 API Key、模型、默认目标语言、浮窗宽度和自动选区开关。
- 翻译结果浮窗的目标语言切换、重译和术语解释继续可用。

### Tauri Watcher Debug Fixes

修复 Tauri 迁移后 `Cmd+C+C` 和 watcher 调试不可靠的问题。

主要内容：

- 定位到快速双复制可能在 `50ms` pasteboard 轮询间隔内只产生一次 watcher stdout。
- pasteboard watcher 输出增加 `NSPasteboard.changeCount` 跳变量；当一次轮询发现 changeCount 跳变 `>= 2` 时，也按双复制处理。
- 忽略 `pbcopy` 或系统复制过程中可能出现的空文本中间态，避免它覆盖上一条有效复制记录。
- Swift watcher 增加父进程检测，Tauri dev 重启或 App 退出后自动退出，避免遗留孤儿 watcher。
- watcher stderr 会写入状态，后续若 Swift 脚本编译或运行失败，不再静默显示“已启用”。

验证结果：

- 无间隔连续写入两次剪贴板可以触发浮窗。
- `200ms` 间隔双复制仍正常触发。
- 停止 Tauri dev app 后没有遗留 `swift-frontend` watcher 进程。

### Tauri Popup Click And Selection Lifecycle Fixes

修复浮窗内按钮点击被全局鼠标 watcher 抢先关闭，以及 selection 状态被错误 suppress 的问题。

主要内容：

- 浮窗内点击判断统一到 Tauri logical 坐标，并增加边框容错，避免点在浮窗按钮上被误判为外部点击。
- `翻译 / 重译 / 术语 / 复制 / 固定` 按钮点击不再被 `mouse_down` 外部关闭逻辑抢先隐藏。
- 外部点击关闭 selection 小浮窗时只清空当前 active selection，不再 suppress 文本。
- 点击 selection 小浮窗的 `翻译 / 复制 / 关闭` 才视为用户已处理当前选区，并 suppress 当前文本，避免旧选区回弹。
- Swift selection watcher 不再永久记住上一条 selection 文本；真实重新拖选同一段文本可以再次发出 selection 事件。

验证结果：

- Rust 单测覆盖 popup hit-test 和 selection suppression 分支。
- `cargo test`、`cargo check`、`cargo clippy -- -D warnings`、`npm run test`、`npm run typecheck`、`npm run build` 通过。

### Cmd+C+C Key Event Trigger And Draggable Popup

修复 `Cmd+C+C` 无效、翻译模式过多和浮窗无法拖动的问题。

主要内容：

- macOS 双复制监听改为监听真实 `Cmd+C` key down 节奏，第二次按键后延迟读取剪贴板并触发翻译。
- pasteboard changeCount 仅保留为诊断状态，不再作为双复制唯一触发依据。
- 设置页和类型层移除“直译 / 意译”，旧配置中的 `literal` / `natural` 自动归一为“学术翻译”。
- 结果浮窗 header 使用 Tauri 拖拽区域；selection 小浮窗增加紧凑拖拽把手。
- popup 内容 resize 只执行一次初始定位，避免用户拖动后又被拉回触发位置。

验证目标：

- 快速按两次 `Cmd+C` 后，浮窗应按当前剪贴板文本翻译。
- 设置页翻译模式只显示“学术翻译 / 双语对照 / 解释术语”。
- 结果浮窗和 selection 小浮窗都可拖动，拖动后重译或内容变化不回跳。

### Selection Popup Focus And Native Window Drag

修复 selection 小浮窗抢焦点影响 `Cmd+C+C`，以及 Tauri CSS 拖拽区域不够可靠的问题。

主要内容：

- selection 小浮窗显示时不再调用 `set_focus()`，避免抢走原应用选区焦点。
- `Cmd+C+C` 读取到空剪贴板时，如果当前 selection 小浮窗仍有关联文本，则回退使用 active selection 文本翻译。
- 结果浮窗、loading 和 error 浮窗仍会获得焦点，保留 Esc 和按钮操作。
- renderer bridge 新增 `startWindowDrag()`，调用 Tauri `getCurrentWindow().startDragging()`。
- 结果浮窗 header 和 selection 小浮窗拖拽把手改用原生拖窗 API，按钮区域阻止拖动事件冒泡。
- Tauri capability 增加 `core:window:allow-start-dragging`。

验证目标：

- 拖选浮窗开启时，`Cmd+C+C` 不再因为剪贴板为空而失败。
- selection 小浮窗和结果浮窗拖动应实时跟随鼠标。
- 拖动后重译、术语或目标语言变化不应导致浮窗回跳。

### Manual Popup Drag State

修复 Tauri 原生拖窗在当前 popup 场景下拖动跳跃和拖动时误关闭的问题。

主要内容：

- renderer 改为使用 pointer capture + `outerPosition()` / `setPosition()` 手动实时移动 popup。
- selection 小浮窗拖拽把手和结果浮窗 header 复用同一套手动拖动逻辑。
- Rust backend 新增 `popup_dragging` 内部状态，拖动期间全局 mouse watcher 不执行外部点击关闭。
- Tauri capability 增加窗口读写位置权限。
- `startDragging()` 不再作为主拖动路径。

验证目标：

- selection 小浮窗拖动应实时跟随鼠标，不再松手后跳跃。
- 拖动由拖选或 `Cmd+C+C` 触发的结果浮窗时不应消失。
- 拖动结束后，未固定浮窗点击外部仍应关闭。

### Popup Language Selection Stability

修复 `Cmd+C+C` 触发结果浮窗后，切换目标语言或点击重译时浮窗偶发消失的问题。

主要内容：

- Rust backend 新增短暂的 popup 交互保护窗口，语言选择、输入和底部按钮交互期间，全局 mouse watcher 不会把原生下拉菜单选项点击误判为外部点击。
- React popup bridge 新增 `protectPopupInteraction` command，并在浮窗底部控件的 pointer/focus/change 交互时刷新保护窗口。
- 外部点击关闭逻辑仍保留；保护窗口过期后，未固定浮窗点击外部仍会关闭。

验证结果：

- `cargo test` 新增覆盖“保护期内外部 mouse_down 不关闭”和“保护期过期后仍关闭”。
- `npm run typecheck`、`npm test`、`cargo check` 通过。

## 2026-07-19 鼠标划词可靠性专项：RC-29 至 RC-46 确定性快照

本节是当前架构与验收状态；上文的 Electron、Swift watcher、按文本
suppression、固定交互保护窗口和“关闭后 selection watcher 继续运行”等
内容保留为历史记录，不再描述现行实现。唯一执行基线为
`docs/SELECTION_RELIABILITY_REMEDIATION_PLAN_2026-07-19.md`。

### 当前生产架构

- macOS 监听已是编译进 Tauri 主进程的 Objective-C bridge，不再启动或解析
  Swift helper/stdout；event taps、AX observer、timers、run-loop sources 和
  callback context 由 watcher lifecycle generation 显式拥有和释放。
- native 触发点冻结 PID、窗口、优先 AX 元素/selected range、锚点、时间与
  generation；Rust controller 只接受严格递增 generation，并以 revisioned
  snapshot/event、翻译 token 和真实 commit 顺序防止旧读取/旧翻译覆盖。
- `enableSelectionPopup=false` 现在会停止 mouse selection tap、AX selection
  observer、选区读取/重试和文本回调，只保留 `Cmd+C+C` 的 key/pasteboard
  链路；关闭与重新开启均经新的 lifecycle generation 和 terminal readiness
  确认，不能再表述为“仅隐藏 UI”。
- 工作区来自目标 `NSScreen.frame/visibleFrame` 的四边 inset 并转换到统一的
  CG 顶左坐标；失败时弹层 fail-closed，不回退到当前/主显示器。
- 设置页区分草稿、已保存和当前生效；设置文件、Keychain 和 watcher 重启
  独立反馈，保存期间锁定草稿，外部磁盘变更按 clean/dirty 草稿分别同步或
  报冲突，运行时 warning 不再被额外刷新覆盖。

### 本轮确定性缺口修复

- RC-29/30：设置/Keychain 错误不再伪装 missing；真实 Tauri `show`、
  `startDragging` 等窗口权限纳入 capability 契约。
- RC-31/39：目标显示器 visible work area 的四边、负坐标、Dock/菜单栏和
  混合 scale 适配改由原生平台层提供并有契约测试。
- RC-32/36/37：冻结窗口、strict generation 与同窗多控件 selected-range
  bounds 唯一候选消歧已接入生产路径；歧义时放弃而不是读取任意旧选区。
- RC-33/35：watcher readiness 不再依赖固定 sleep；新增显式、内存态、
  有容量上限且不含内容的验收会话，记录次数、revision、端到端时延、
  A→B、tap 恢复和资源计数。
- RC-34/38：签名分类区分 ad-hoc、Apple Development、Developer ID；发布
  校验使用独立构造的严格 designated requirement，不信任制品自带的宽松
  requirement。
- RC-40：自动划词开关已经控制真实 selection sources 与 disabled terminal；
  关闭失败 fail-closed，不用保存值冒充运行态。
- RC-41：double 作为可取消 provisional 手势，第三击到达后只允许最终
  triple generation 提交；stop/restart 会清除 provisional 状态。
- RC-42/43：tap-disabled 连续验收入口可自动注入并到达终态；rapid A→B
  以真实 PopupController commit 顺序而非 trigger offset 判定旧写入。
- RC-44：RC-41 局部复验确认固定 350ms 双击端到端阈值与系统多击窗口
  确定性冲突。现已完整保留第二次 mouse-up 起算的端到端时延，显式记录
  生产 watcher 实际 Q，并以 Q+350ms P95/Q+800ms max 约束。无效 Q
  走真实 watcher startup 后 Q=0、selection sources=0、专用 terminal 且不完成
  readiness；Q 漂移使报告 fail-closed。
- RC-45：验收 start/end 与 refresh/save/restart 共享 transition gate，遵守
  transition→native Q→acceptance 锁序；off-main start 后的同步 Q getter 作为
  主队列 FIFO 屏障，关闭了 end 读旧 Q、restart 捕获新 Q 的 TOCTOU，且没有
  acceptance-lock→main-thread 锁反转。
- RC-46：退出采用不可逆 quitting 标记与同步短生命周期门；native 使用
  latest-submission token 处理后台排队与主线程即时调用的越队。stale start
  只以 sentinel 释放未安装 owner，不推进当前 lifecycle，也不访问
  AppState/acceptance；restart-first/exit-first、锁中毒及三种 native 越队最终
  均收束到正确 sources。

### 局部复验与当前结论

- RC-35/43 三文件冻结范围的只读六维复验为 `APPROVED`；其历史计数已经被
  本节后述最终确定性门禁取代，不再作为当前终态数字。
- RC-40/42/43 UI 范围的独立只读复验为 `APPROVED`：Settings Playwright
  20/20，包含 disabled unconfirmed/fail-closed、外部设置冲突、tap 卡死、
  状态失败重试和 50 次 watcher restart 浏览器路径。
- RC-41 原生范围的局部复验未发现 P0/P1；RC-44/45 冻结由同一只读审查者
  `APPROVED`。RC-46 首冻因 sentinel 误伤 acceptance integrity 被
  `REJECTED`（P2=1），修复后二冻为 `APPROVED`（P0/P1/P2=0），冻结
  `lib.rs` SHA-256 为
  `7adbf11905bbe8b4d6077582fc86ac2cf1d6adc0022620fb2cc14e28fa99dc6f`。
- 根级全量质量链通过：Vitest/契约 73/73、浏览器 UI 47/47（含 10 项自动
  可访问性检查）、renderer 生产构建、默认 Rust 193/193、acceptance feature
  195/195、默认/feature check 与严格 clippy 均通过，`git diff --check`
  通过。原生严格双架构脚本通过：watcher restart 50 次且
  `lifecycleRaces=true`/`raceFinalSources=0`、freeze 37/37、selection/work-area
  65/65、multi-click 95/95。
- 默认和 acceptance `.app` 使用独立 target 目录构建且 `codesign --verify
  --deep --strict` 均通过。默认二进制 SHA-256 为
  `7b6edab111e4dd05c195436d2708a744fc4185f2ce890274fded6e9354c79b22`，只含生产
  `paper_float_multi_click_quiet_window_seconds`，不含注入命令或
  `paper_float_test_*`；acceptance 二进制 SHA-256 为
  `ff050a6b3ff68ba1c9d8e85df3e1c89b4d29305017585751b72cf86cf8a6a67e`，明确含
  注入命令及 `paper_float_test_inject_tap_disabled`。严格发布脚本对默认包因
  ad-hoc 而拒绝、对 acceptance 包因测试符号而拒绝；Gatekeeper 对默认包
  返回 `rejected`，均符合制品分类预期。
- 同一只读审查任务已完成 ROUND 3：审查者在隔离副本复现根级质量链、
  双架构 native harness、默认/acceptance 双制品与静态生产路径，未发现新的
  代码/自动化 P0/P1/P2；正式结论仍为 `REJECTED`，因为真实 Tauri GUI/TCC
  量化、Developer ID/稳定 TCC 身份与真实多屏尚未完成。该轮唯一新增 P2
  `REV-35` 是规划第 13 节的旧计数/状态不一致，本次已同步为
  73/47/193/195 与 ROUND 3 证据，正在交回同一任务做只读快照复核。

### 尚未伪造为通过的门槛

- 真实 Tauri GUI/TCC、TextEdit/Safari/Preview 量化、五分钟自身隔离和真实
  焦点/权限撤销恢复尚未运行；启动本地未发布应用或修改系统权限前必须取得
  用户当下明确确认。
- 当前 `security find-identity -v -p codesigning` 为
  `0 valid identities found`。ad-hoc 包不能证明 Developer ID、notarization、
  Gatekeeper、稳定 TCC 身份或覆盖安装后的权限持久性。
- 当前没有第二显示器；负 X、上下排列和混合缩放已有自动化几何证据，但
  真实多屏仍是发布前人工门槛。

## 2026-07-19 RC-47：验收制品与正式用户状态隔离

ROUND 3 后的只读运行预检发现，原 acceptance feature 仍复用正式应用的
product/bundle、Application Support、Keychain 与 TCC client identity。为避免
真实 GUI 验收先污染用户状态，新增 RC-47/REV-36，并在修复与复验期间禁止
启动旧验收包或操作 TCC。

本轮实现：

- acceptance 使用 `Paper Float Translator Acceptance`、
  `com.paperfloat.translator.acceptance`、独立 Cargo target、app-data/报告根及
  Keychain service/account；默认包继续只含正式名称空间。
- 默认与 acceptance 两种编译制品都在 Tauri Builder/WebView 创建前校验
  product/bundle，setup 再校验一次；两个方向的 feature/config 错配均
  fail-closed。
- 验收根、报告目录、settings/cache/glossary/report 拒绝 symlink 与多硬链接；
  Unix 读取使用 `O_NOFOLLOW` 并在已打开文件上复验普通文件和 `nlink=1`。
- 普通 JSON 与验收报告的临时文件均使用 `create_new` 独占创建，写入、同步、
  关闭、目标复验、rename 和父目录同步顺序固定；预置临时 file/symlink 不会被
  跟随或误删，根外 sentinel 保持不变。
- 制品验证器失败闭合检查 plist 四字段、codesign Identifier、CDHash、指定要求、
  raw symbols/strings 与二进制 SHA；窗口契约比较除 title 外的完整对象，并含
  字段及反向身份守卫突变负例。

最新确定性证据：

- 根级质量链：Vitest/契约 78/78、浏览器 UI 47/47、renderer 生产构建、默认
  Rust 201/201、格式/check/严格 clippy 全部通过。
- acceptance feature：Rust 216/216、check 与 all-features 严格 clippy 通过。
- 原生双架构：restart 50（`callbackAfterRelease=0`、`raceFinalSources=0`）、
  freeze 37、work-area 65、multi-click 95 全部通过。
- 最新 default/acceptance `.app` 均在当前源码之后重建并通过隔离 verifier：
  binary SHA-256 分别为
  `0170593afc6d16017b3dfa555cca7bd0a415e6cc64796df601b71474278b466e` 与
  `35cb69330699df810a4a3cdbd6604922c2522b3299f7ccb4ee5d2e9d699746df`。
- 正式 settings/cache SHA-256 仍为
  `9f513101db0c38f2c30b93f56f5db5a849c5ea231c70d115ded8a05aa0648bee` 与
  `c1b6f5d3d80a3c418015ec8367533692a6937ab8e3c7112a7ac3cfe2ef5eafdf`；
  正式 glossary、正式验收报告和专用 acceptance app-data 根均不存在。

内部第三轮只读预审已对 RC-47 源码与确定性自动化返回 `APPROVED`
（P0/P1/P2=0），并接受“恶意同 UID 主体主动替换祖先目录”不属于本次防止错配
制品误触正式状态的威胁模型；若未来纳入该威胁，应另立 dirfd/openat/renameat
需求。

同一正式独立审查任务 `019f7684-5af2-7cc1-995d-e89728372f10` 随后完成
REV-36 第三冻结复验并返回 `APPROVED`（P0=0、P1=0、P2=0）。审查者在隔离
副本独立通过 78/78 Vitest、201/201 默认 Rust、216/216 acceptance feature
Rust、格式及默认/all-features strict clippy，并只读复核双制品 verifier、
same-path exit 1、签名/身份/符号隔离和复验前后冻结摘要完全一致。按授权限制，
审查者没有读取正式 settings/cache 内容，没有启动 `.app`、访问 Keychain 或
探测 TCC；复用 workspace `node_modules` 时只刷新了 Git 忽略的 Vitest 缓存，
tracked/untracked 摘要、源码、配置、测试、制品和用户数据均未变化。

REV-36 只允许主线程请求用户对启动专用 acceptance 应用并仅为其 identity
检查/调整 Accessibility 与 Input Monitoring 的当下明确授权；它不等于已经
执行真实 GUI/TCC。Developer ID/稳定权限身份、真实多屏及 TextEdit/Safari/
Preview 量化仍是全项目 `REJECTED` 的剩余门槛。

## 2026-07-19 RC-48：授权前跨文档证据同步

REV-36 通过后的完成性审计发现，`docs/RELEASE.md` 的“Current Release
Blockers”仍引用 RC-47 前的 73/193/195 计数与旧双制品 SHA，
`docs/PROJECT_PLAN.md` 也仍把专项唯一基线截在 RC-46。该漂移不会改变已通过
代码，但可能让真实验收启动旧 acceptance 制品，因此按变更控制登记
RC-48/REV-37。

本轮只修改文档：发布手册同步为 78/47/201/216、当前双制品 SHA 与独立
`com.paperfloat.translator.acceptance` 身份，并明确 REV-36 只允许请求用户
授权，不能替代真实 GUI/TCC 或 Developer ID 发布证据；项目总计划同步指向
RC-01 至 RC-48。产品代码、测试、配置和 `.app` 均未修改或启动，Keychain/TCC
与正式用户数据未被访问。冻结后须交同一独立任务做 REV-37 只读复验。

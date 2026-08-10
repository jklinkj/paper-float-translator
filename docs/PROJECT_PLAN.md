# Paper Float Translator Project Plan

## Summary

目标是做一个 macOS 优先的桌面端论文划词翻译工具：

选中文本句子或段落后，程序尝试通过 macOS Accessibility 读取当前选区，并在鼠标附近弹出轻量操作浮窗；点击“翻译”后调用 DeepSeek 展示译文。若当前应用不支持自动选区读取，仍可快速按两次 `Cmd+C` 触发兜底翻译。

第一版采用：

- Tauri v2 + Vite + React + TypeScript + Rust backend
- 桌面 App 优先
- macOS 自动选区操作浮窗优先，`Cmd+C+C` 双复制作为兜底
- DeepSeek 默认模型：`deepseek-v4-flash`
- API Key 使用系统钥匙串保存
- 项目按可扩展架构设计，后续支持浏览器插件、术语表、流式输出和正式分发

## Architecture

项目采用 monorepo 结构：

```text
paper-float-translator/
  apps/
    desktop/
      renderer/
      src-tauri/
  packages/
    core/
    deepseek/
    storage/
  docs/
    PROJECT_PLAN.md
```

模块职责：

- `apps/desktop`：Tauri 桌面端入口，负责窗口、macOS 选区监听、macOS pasteboard 监听、剪贴板、command/event bridge、浮窗。
- `apps/desktop/src-tauri`：Rust backend，负责系统能力、设置、Keychain、缓存、DeepSeek 请求和 watcher 生命周期。
- `apps/desktop/renderer`：React/Vite 前端，负责设置页和浮窗 UI。
- `packages/core`：平台无关核心逻辑，包括文本清洗、prompt 构造、缓存 key、通用类型。
- `packages/deepseek`：历史 TypeScript DeepSeek adapter；Tauri 版运行时由 Rust backend 直接调用 DeepSeek，待确认无复用后删除。
- `packages/storage`：历史 TypeScript 存储封装；Tauri 版运行时使用 Rust JSON store 和 macOS Keychain，待确认无复用后删除。
- `docs/PROJECT_PLAN.md`：固定产品目标、架构、实现路径和验收标准。

运行边界：

- Tauri Rust backend：
  - 仅在已生效的 `enableSelectionPopup=true` 时启动 macOS 自动选区输入源；关闭后释放 mouse tap、AX selection observer、选区重试和直接读取。
  - 常驻启动 macOS 双复制兜底监听。
  - 进程内 Objective-C native bridge 冻结选区触发时的 PID、窗口、元素/范围、锚点、generation 和单调时钟，再由 Rust controller 校验、取消和分配 revision。
  - 分别建模 Accessibility 授权、Input Monitoring、mouse/key tap、AX observer、AX direct read 与只读 pasteboard 能力，并以 watcher lifecycle generation 的 ready/degraded/disabled 终态握手。
  - 检测快速两次 `Cmd+C` 作为兜底。
  - 读取剪贴板文本。
  - 调用 DeepSeek API。
  - 通过目标显示器原生 `visibleFrame`、统一坐标模型与 revisioned 状态机管理浮窗位置和生命周期。
  - 访问系统钥匙串。
- Tauri command/event bridge：
  - renderer 通过 `invoke` 调用受控 command。
  - backend 提供权威 `PopupState`/settings 快照，并通过带协议版本和 revision 的 window event 推送增量；事件丢失可由快照恢复。
  - API Key 不进入 renderer。
- React renderer：
  - 设置页 UI。
  - 浮窗 UI。
  - loading、错误、译文展示和用户操作。

## MVP Scope

第一阶段只做桌面核心闭环：

- 选中文本后自动显示操作浮窗
- 操作浮窗支持复制、翻译、关闭
- 不支持自动选区读取时，快速按两次 `Cmd+C` 触发兜底翻译
- 已生效设置开启时运行 macOS selection watcher，关闭时停止所有 selection sources
- App 启动后常驻 macOS pasteboard watcher
- 读取当前剪贴板文本
- 清洗 PDF 文本
- 调 DeepSeek API
- 鼠标旁展示浮窗
- 浮窗支持 loading、译文、错误、复制、固定、关闭
- 设置页支持：
  - DeepSeek API Key
  - 默认模型
  - 翻译模式
  - 默认目标语言
  - 拖选后显示操作浮窗开关
  - 自动选区监听状态
  - 双复制兜底监听状态
  - PDF 清洗开关
  - 本地缓存开关

第一阶段不做：

- 全局快捷键触发
- 自动模拟复制选区
- 浏览器插件
- 多 provider
- 云同步
- 自动更新（签名、公证和 TCC 身份已是发布硬门槛，不能以“第一版”排除）

## Implementation Path

### Phase 1: Desktop Core

搭建 Tauri v2 + Vite + React + TypeScript + Rust backend 项目。

实现基础架构：

- monorepo workspace
- shared TypeScript 类型
- Tauri command/event bridge
- 无边框 always-on-top 浮窗
- 设置页

实现翻译主链路：

```text
mouse selection
  -> detect macOS mouse selection end
  -> read selected text via Accessibility
  -> show action popup near cursor
  -> user clicks translate
  -> clean text
  -> check local cache
  -> call DeepSeek if cache miss
  -> show popup near cursor
```

实现基础安全策略：

- API Key 不写入前端代码。
- renderer 不直接访问 API Key。
- API Key 存系统钥匙串。
- 普通设置存本地配置文件。
- 日志中不打印 API Key 和完整请求头。

### Phase 2: Paper Reading Enhancements

增强论文阅读体验：

- 术语表 glossary
- 中英对照模式
- 解释术语
- 重新翻译
- 长段落模式
- DeepSeek streaming 输出
- 缓存管理 UI

术语表和目标语言参与缓存 key：

```text
sha256(model + mode + targetLanguage + glossaryVersion + cleanedText)
```

### Phase 3: Browser Extension

增加浏览器插件：

- 网页中使用 `window.getSelection()` 获取选区。
- 复用 `packages/core` 的文本清洗、prompt、类型和缓存 key。
- 支持网页 / arXiv / PDF.js 场景。
- 桌面 App 继续负责跨应用场景。

### Phase 4: Productization

完善产品化能力：

- 打包安装包。
- macOS / Windows / Linux 平台适配。
- 平台权限引导。
- 错误日志。
- 版本发布流程。
- Developer ID 签名、公证、Gatekeeper 与稳定 TCC 身份作为发布前硬门槛；自动更新后置。

## Core Interfaces

翻译模型：

```ts
type DeepSeekModel = "deepseek-v4-flash" | "deepseek-v4-pro";
```

翻译模式：

```ts
type TranslateMode =
  | "academic_zh"
  | "bilingual"
  | "terminology";
```

翻译请求：

```ts
interface TranslateRequest {
  text: string;
  model: DeepSeekModel;
  mode: TranslateMode;
  targetLanguage: string;
  glossary?: Record<string, string>;
}
```

翻译结果：

```ts
interface TranslateResult {
  cleanedText: string;
  translation: string;
  cached: boolean;
}
```

选区读取策略：

- 主路径使用 macOS Accessibility API 读取当前 focused UI element 的选中文本。
- 使用编译进主进程的 Objective-C bridge 协同 macOS `CGEventTap` 与 AX selected-text notifications，覆盖拖选、双击、三击、Shift 扩选和键盘选区；两类输入源互补而非互斥。
- mouse-up/AX notification 建立严格递增的 native generation，并冻结来源 PID、窗口 ID、优先元素/selected range、鼠标锚点和触发时间。延迟重试只能验证并使用冻结上下文，不得重新查询当时的前台应用或当前鼠标位置。
- 新手势、关闭、禁用和退出统一取消旧 generation；Rust 只接受严格递增的 generation，旧/相等/零值事件不得改变 snapshot、revision 或翻译 token。
- 双击先作为可取消的 provisional 手势；生产 watcher 读取 `NSEvent.doubleClickInterval` 并加入调度余量形成 Q，第三击到达后只允许最终 triple 提交。独立双击等待 Q 后再读取，端到端验收按“Q + 固定应用处理预算”判定。
- 读取候选优先使用冻结锚点元素；整窗回退只在 selected-range bounds 与锚点能唯一归属时选择候选，同距、无 bounds 或跨控件歧义均 fail-closed。
- 不做选区轮询触发，避免拖选过程中提前弹出浮窗；event tap/observer 失效必须进入可见诊断或在两秒内恢复，不能静默假装 ready。
- 读取成功后只显示操作浮窗，不立即调用 DeepSeek。
- 自动选区操作浮窗默认开启；用户关闭并保存后，运行时释放 mouse tap、AX observer、选区重试/读取与回调，只保留 `Cmd+C+C` 的 key tap 和只读 pasteboard 诊断/回退链路。重新开启必须创建新的 watcher generation 并等待 terminal readiness。
- 去重只抑制同一 generation/revision 的重复通知，不按文本内容永久抑制；相同文字在新的真实手势中必须产生新的 revision。
- 自动选区 watcher 失败或权限不足时发布精确 capability/status 与恢复动作，继续保留可用的双复制兜底，但不得用兜底健康冒充自动选区健康。
- `Cmd+C+C` 主路径监听真实 `Cmd+C` key down 节奏；macOS `NSPasteboard.general.changeCount` 的 `50ms` 轮询只作只读 fallback/诊断。
- 两次复制窗口固定为 `900ms`。
- 两次复制的清洗后文本必须相同且非空。
- `changeCount` 跳变只说明发生了多个剪贴板修订；不得据此伪造未观测到的中间复制内容或把单个最终值冒充两次相同复制。
- 复制过程中出现的空文本中间态不会覆盖上一条有效复制记录。
- 命中双复制后立即在鼠标附近显示 loading 浮窗。
- 双复制冷却只约束同一 key sequence/revision，不能变成跨手势文本永久去重。
- 翻译完成后剪贴板保留用户复制的原文。
- native bridge 与应用同进程，由显式 watcher handle/lifecycle generation 管理；退出使用不可逆 quitting 标记、同步短生命周期门和 native latest-submission token，保证后台排队与主线程即时 start/stop 越队时仍由最新提交胜出，stale context 只释放 owner 且不污染当前验收状态。退出最终释放 event taps、run-loop sources、observers、timers 和 context，不存在独立 Swift helper 父进程模型。

## DeepSeek Policy

默认请求策略：

- base URL：`https://api.deepseek.com`
- 默认模型：`deepseek-v4-flash`
- 高质量模型：`deepseek-v4-pro`
- 关闭 thinking
- 第一版使用非流式请求
- 后续支持 streaming

默认 system prompt 目标：

- 面向多语言学术内容翻译。
- 自动识别用户提供的原文语言，不要求源语言必须是英文。
- 翻译成准确、自然、适合目标语言学术阅读的内容。
- 保留公式、变量名、引用编号、专有名词。
- 必要时在目标语言译名后保留原文术语。
- 只输出译文，不输出解释。
- 术语模式必须输出纯文本列表，格式为 `原文术语：目标语言译名。说明：用目标语言一句话解释。`，不使用 Markdown。

## Text Cleaning

默认开启 PDF 文本清洗。

规则：

- 修复 PDF 断词：
  - `exam-\nple` -> `example`
- 合并普通换行：
  - `line one\nline two` -> `line one line two`
- 合并重复空格。
- 保留公式、变量、引用编号。
- 避免破坏 `state-of-the-art` 这类合法连字符词。

## Storage

本地存储策略：

- API Key：系统钥匙串。
- 设置：本地配置文件。
- 缓存：带进程内锁、原子临时文件替换和 500 项上限的本地 `cache.json`。
- 术语表：本地 JSON。

缓存 key：

```text
sha256(model + mode + targetLanguage + glossaryVersion + cleanedText)
```

缓存命中时不调用 DeepSeek。

## UI Behavior

浮窗行为：

- 默认显示在触发位置右下方。
- 靠近屏幕边缘时自动反向避让。
- 选区状态是紧凑按钮条，只显示“复制 / 翻译 / 关闭”，不展示选中文本。
- 选区浮窗固定在鼠标松开位置附近，不跟随之后的鼠标移动。
- key tap 为 `Ready` 时选区浮窗保留来源应用焦点；key tap 为 Degraded/Disabled/Unavailable/Unknown 时 fail-safe 聚焦弹窗，避免键盘用户同时失去全局和本地入口。首个全局 Tab/Shift+Tab 以 revision 定向进入首/末可用控件，Escape 可关闭；只有仍聚焦弹窗且来源/前台/revision 条件成立时才恢复来源应用焦点。
- 结果浮窗和选区浮窗都通过手动 pointer 拖动支持实时移动，用户拖动后不因内容高度变化自动回到触发位置。
- 拖动期间全局鼠标监听暂停外部点击关闭，避免拖动过程中浮窗误消失。
- 未固定浮窗获得焦点后，点击外部会因失焦关闭；全局鼠标按下事件也会关闭外部点击。
- loading 状态显示“正在翻译”。
- 成功后显示译文。
- 失败后显示可读错误。
- 结果浮窗提供目标语言输入框；修改后点击“重译”才重新请求。
- Esc 关闭浮窗。
- 固定后不因失焦自动关闭。
- 未固定时可点击外部关闭。

浮窗按钮：

- 复制
- 翻译选区
- 复制译文
- 固定 / 取消固定
- 重新翻译
- 解释术语

设置页：

- API Key 输入和保存
- 默认模型选择
- 翻译模式选择
- 默认目标语言输入
- 拖选后显示操作浮窗开关
- 自动选区监听状态
- 辅助功能权限入口
- 双复制监听状态
- PDF 清洗开关
- 缓存开关
- 明确区分草稿、已保存和当前生效；保存进行中锁定草稿控件，磁盘外部变更在草稿干净时同步、草稿脏时显示冲突而不覆盖。
- 设置文件、Keychain、watcher 重启分别反馈；部分成功不得伪装为全部成功，运行时 warning 与磁盘/内存不一致必须提供刷新恢复入口。
- 权限 CTA 由具体 capability 决定；从系统设置返回后刷新权威快照，不以固定睡眠或旧前端副本推测 readiness。

## Testing

单元测试：

- 文本清洗
- prompt 构造
- cache key
- DeepSeek request body
- storage 读写
- glossary 合并

集成测试：

- macOS 自动选区监听触发操作浮窗
- macOS 双复制触发
- 非 macOS 显示不支持状态
- 复制选区原文不请求 API
- 空剪贴板或非文本复制显示错误
- 空选区不请求 API
- 缓存命中不请求 API
- API 失败显示错误
- 浮窗不超出屏幕
- Esc 关闭浮窗

验收标准：

- 鼠标划词专项以 `docs/SELECTION_RELIABILITY_REMEDIATION_PLAN_2026-07-19.md` 的 RC-01 至 RC-55、场景矩阵和 §8.1 为唯一详细基线；本文件不得维护一套更弱的替代标准。
- 固定 TextEdit 拖选/双击/三击/Shift/键盘各 30/30 正确且唯一；同文同/异位置各 30 次均产生新 revision；20 组 A→B 无旧写入。
- 非多击 TextEdit 路径 P95 不高于 350ms、最大 800ms；独立双击从第二次 mouse-up 端到端计时，报告记录 watcher 实际 Q，并按 Q+350/Q+800 判定。Safari/Preview 依固定 fixture 与对应 1,200ms 预算。
- tap disabled 在两秒内恢复或明确降级；50 次 restart 后 observer/tap/context 不增长；设置页和弹层连续操作五分钟 selection 自触发为 0。
- 自动划词关闭后 selection sources/read/retry 为 0 且 `Cmd+C+C` 保留；重新开启使用新 generation，无旧回调。
- 翻译完成后剪贴板仍保留用户复制的原文；无 API Key、网络失败、权限/监听失败均有精确提示和恢复动作。
- API Key、选中文本、译文、剪贴板内容、窗口标题或文本衍生信息不得进入验收报告、renderer 持久态、普通日志或仓库。
- 自动化、浏览器 UI、原生 harness、真实 Tauri GUI/TCC、签名/公证和硬件相关多屏证据必须明确分层；任何一层不得冒充另一层。

## Assumptions

- 第一版采用 macOS 优先架构，Windows/Linux 暂时显示自动选区和双复制不支持。
- 自动选区操作浮窗是主触发方式，`Cmd+C+C` 是稳定兜底触发方式。
- 浏览器插件后置，不阻塞桌面 MVP。
- 本地诊断包可以继续开发验证，但 Developer ID、notarization、Gatekeeper、稳定 TCC 身份和覆盖安装权限持久性是公开发布硬门槛；自动更新后置。
- DeepSeek 是第一版唯一 provider，但 adapter 保持可扩展。
- 关闭设置窗口只隐藏，Dock 可重开，`Cmd+Q` 完全退出；但自动选区输入源是否运行由已生效设置控制，不能把“应用后台常驻”误写成“selection watcher 永久常驻”。

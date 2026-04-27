# Paper Float Translator Project Plan

## Summary

目标是做一个 macOS 优先的桌面端论文划词翻译工具：

选中英文句子或段落，快速按两次 `Cmd+C`，程序读取当前剪贴板，调用 DeepSeek 翻译，并在鼠标附近弹出轻量浮窗展示译文。

第一版采用：

- Electron + Vite + React + TypeScript
- 桌面 App 优先
- macOS `Cmd+C+C` 双复制触发，不做全局快捷键触发
- DeepSeek 默认模型：`deepseek-v4-flash`
- API Key 使用系统钥匙串保存
- 项目按可扩展架构设计，后续支持浏览器插件、术语表、流式输出和正式分发

## Architecture

项目采用 monorepo 结构：

```text
paper-float-translator/
  apps/
    desktop/
      electron/
      renderer/
      preload/
  packages/
    core/
    deepseek/
    storage/
  docs/
    PROJECT_PLAN.md
```

模块职责：

- `apps/desktop`：Electron 桌面端入口，负责窗口、macOS pasteboard 监听、剪贴板、IPC、浮窗。
- `packages/core`：平台无关核心逻辑，包括文本清洗、prompt 构造、缓存 key、通用类型。
- `packages/deepseek`：DeepSeek API adapter。
- `packages/storage`：设置、缓存、术语表、系统钥匙串封装。
- `docs/PROJECT_PLAN.md`：固定产品目标、架构、实现路径和验收标准。

进程边界：

- Electron main process：
  - 常驻启动 macOS 双复制监听。
  - 检测快速两次 `Cmd+C`。
  - 读取剪贴板文本。
  - 调用 DeepSeek API。
  - 管理浮窗位置和生命周期。
  - 访问系统钥匙串。
- preload：
  - 暴露受控 IPC API。
  - 不直接暴露 Node 能力给 renderer。
- renderer：
  - 设置页 UI。
  - 浮窗 UI。
  - loading、错误、译文展示和用户操作。

## MVP Scope

第一阶段只做桌面核心闭环：

- 选中文本后快速按两次 `Cmd+C` 触发翻译
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
  - 双复制监听状态
  - PDF 清洗开关
  - 本地缓存开关

第一阶段不做：

- 全局快捷键触发
- 自动模拟复制选区
- 浏览器插件
- 多 provider
- 云同步
- 签名、公证、自动更新

## Implementation Path

### Phase 1: Desktop Core

搭建 Electron + Vite + React + TypeScript 项目。

实现基础架构：

- monorepo workspace
- shared TypeScript 类型
- main/preload/renderer IPC
- 无边框 always-on-top 浮窗
- 设置页

实现翻译主链路：

```text
Cmd+C+C
  -> detect repeated macOS pasteboard text
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
- 直译 / 意译模式
- 解释术语
- 重新翻译
- 长段落模式
- DeepSeek streaming 输出
- 缓存管理 UI

术语表参与缓存 key：

```text
sha256(model + mode + glossaryVersion + cleanedText)
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
- 后续再考虑签名、公证和自动更新。

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
  | "literal"
  | "natural"
  | "terminology";
```

翻译请求：

```ts
interface TranslateRequest {
  text: string;
  model: DeepSeekModel;
  mode: TranslateMode;
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

- 使用 macOS `NSPasteboard.general.changeCount` 监听剪贴板变化。
- 轮询间隔固定为 `50ms`。
- 两次复制窗口固定为 `900ms`。
- 两次复制的清洗后文本必须相同且非空。
- 命中双复制后立即在鼠标附近显示 loading 浮窗。
- 触发后对同一文本做短暂去重冷却，避免多连复制重复请求。
- 翻译完成后剪贴板保留用户复制的原文。

## DeepSeek Policy

默认请求策略：

- base URL：`https://api.deepseek.com`
- 默认模型：`deepseek-v4-flash`
- 高质量模型：`deepseek-v4-pro`
- 关闭 thinking
- 第一版使用非流式请求
- 后续支持 streaming

默认 system prompt 目标：

- 面向英文学术论文翻译。
- 翻译成准确、自然、适合中文学术阅读的中文。
- 保留公式、变量名、引用编号、专有名词。
- 必要时在中文译名后保留英文术语。
- 只输出译文，不输出解释。
- 术语模式必须输出纯文本列表，格式为 `英文术语：中文译名。说明：一句话解释。`，不使用 Markdown。

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
- 缓存：本地 SQLite 或轻量 KV。
- 术语表：本地 JSON。

缓存 key：

```text
sha256(model + mode + glossaryVersion + cleanedText)
```

缓存命中时不调用 DeepSeek。

## UI Behavior

浮窗行为：

- 默认显示在鼠标右下方。
- 靠近屏幕边缘时自动反向避让。
- loading 状态显示“正在翻译”。
- 成功后显示译文。
- 失败后显示可读错误。
- Esc 关闭浮窗。
- 固定后不因失焦自动关闭。
- 未固定时可点击外部关闭。

浮窗按钮：

- 复制译文
- 固定 / 取消固定
- 重新翻译
- 解释术语

设置页：

- API Key 输入和保存
- 默认模型选择
- 翻译模式选择
- 双复制监听状态
- PDF 清洗开关
- 缓存开关

## Testing

单元测试：

- 文本清洗
- prompt 构造
- cache key
- DeepSeek request body
- storage 读写
- glossary 合并

集成测试：

- macOS 双复制触发
- 非 macOS 显示不支持状态
- 空剪贴板或非文本复制显示错误
- 空选区不请求 API
- 缓存命中不请求 API
- API 失败显示错误
- 浮窗不超出屏幕
- Esc 关闭浮窗

验收标准：

- 浏览器、PDF 阅读器、Word、Zotero 中选中文本后，快速按两次 `Cmd+C` 可以弹出译文。
- 翻译完成后剪贴板仍保留用户复制的原文。
- 无 API Key、网络失败、双复制监听失败时都有明确提示。
- API Key 不出现在 renderer、日志或仓库。
- 同一句文本重复翻译能命中缓存。
- 第一版可在本机开发运行和构建。

## Assumptions

- 第一版采用 macOS 优先架构，Windows/Linux 暂时显示双复制不支持。
- `Cmd+C+C` 是桌面端第一版唯一触发方式。
- 浏览器插件后置，不阻塞桌面 MVP。
- 第一版是本机可用版，不做签名、公证、自动更新。
- DeepSeek 是第一版唯一 provider，但 adapter 保持可扩展。

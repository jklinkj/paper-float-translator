# 正式开源与 SignPath 准备核查

核查日期：2026-08-12

## 结论

Paper Float Translator 的代码、文档、资源和依赖可以采用 Apache License 2.0
公开发布。现有 npm 和 Cargo 锁定依赖的许可元数据均为开放许可；项目自己的包
此前缺少许可证声明，现已统一补齐。仓库中唯一不再使用的项目预编译 helper
已删除，macOS native bridge 保留可复现的 Objective-C 源码构建。

## 已落地

- 根目录 Apache-2.0 `LICENSE`、`NOTICE`、第三方说明。
- npm、Cargo、Tauri bundle 统一声明 Apache-2.0。
- 中英双语隐私政策，准确描述 DeepSeek 直连、本地缓存、系统凭据和卸载保留。
- SECURITY、CONTRIBUTING、CODEOWNERS、PR 模板和代码签名政策。
- npm 锁文件许可检查、cargo-deny 许可/来源检查、GitHub dependency review。
- Windows 未签名发布候选工作流，输出安装器、SHA-256、SPDX SBOM 和来源证明。
- NSIS 安装前隐私提示；自动卸载和彻底清理说明。

## 仍需项目所有者在 GitHub/SignPath 完成

- 合并并公开当前本地改动。
- 开启 GitHub MFA、依赖图、私密漏洞报告和 main 分支保护。
- 发布一次明确标记为 unsigned beta 的同形态 NSIS 安装包。
- 提交 SignPath 申请；获批后使用 SignPath 实际分配的标识接入可信构建和人工批准。

## 公开依据

- SignPath Foundation 条件：https://signpath.org/terms.html
- SignPath 申请：https://signpath.org/apply.html
- cargo-deny：https://github.com/EmbarkStudios/cargo-deny
- cargo-deny GitHub Action：https://github.com/EmbarkStudios/cargo-deny-action
- GitHub dependency review：https://docs.github.com/en/code-security/concepts/supply-chain-security/dependency-review
- GitHub artifact attestations：https://docs.github.com/en/actions/concepts/security/artifact-attestations
- DeepSeek 中文隐私政策：https://cdn.deepseek.com/policies/zh-CN/deepseek-privacy-policy.html

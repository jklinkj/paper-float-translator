# Windows 代码签名调研计划

日期：2026-08-12

## 主问题

Paper Float Translator 的 Windows 数字签名是什么，公开发布时应怎样取得并接入 Tauri/NSIS 构建流程？

## 子问题

1. Windows Authenticode 能证明什么，以及它与 SmartScreen 提示之间是什么关系。
2. 公开可信的代码签名证书或云签名服务如何申请，个人/公司与本地硬件/云端密钥分别有什么取舍。
3. Paper Float Translator 当前缺少什么，以及获得签名资格后如何接入本地与 GitHub Actions 发布流程。

## 综合结构

先用非技术语言解释签名，再给出适合本项目的推荐路线、申请步骤、接入步骤，以及签名前后的现实预期和安全注意事项。

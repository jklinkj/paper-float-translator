# Windows 代码签名核查结果

核查日期：2026-08-12

## 数字签名的作用

- Windows Authenticode 签名让系统验证可执行文件的发布者身份，以及签名后文件是否被篡改。
- 时间戳使签名在签名证书到期后仍可验证；微软建议使用 SHA-256 和 RFC 3161 时间戳。
- 签名不等于杀毒证明，也不保证新软件首次下载时完全没有 SmartScreen 提示。
- 微软当前说明：有效 OV/EV 证书会显示已验证发布者，但新应用仍可能先显示“无法识别”；EV 已不再自动获得 SmartScreen 初始信誉。Microsoft Store 分发则由 Microsoft 签名，不出现下载 SmartScreen 警告。

## 可选获得方式

### 1. SignPath Foundation 免费开源签名

- 面向符合条件的纯开源项目免费。
- 证书发布者显示为 SignPath Foundation，不是项目作者个人名称。
- 要求包括：OSI 批准的开源许可证、无专有组件、已经发布并有文档、维护活跃、仓库开启 MFA、有代码签名政策、明确维护/审核/批准角色，以及可验证的自动化构建。
- Paper Float Translator 已于 2026-08-12 在本地工作区补齐 Apache-2.0 `LICENSE`、隐私政策、代码签名政策、依赖许可检查和可验证的 Windows 候选构建；这些内容仍需合并并公开到 GitHub 默认分支后，才构成可供 SignPath 审核的公开证据。

### 2. 购买公开可信代码签名证书或云签名服务

- 可向公开 CA/签名服务申请，如 SSL.com、DigiCert、Sectigo、GlobalSign 等。
- 需要验证个人或公司的真实身份。证书显示的是经过验证的法定名称，不是任意填写的品牌名。
- 2023 年后的公开代码签名私钥通常必须放在合规 USB 硬件令牌或云 HSM 中，不应当作普通 PFX 文件提交进 Git 仓库。
- 价格差异很大，需按所在地、个人/公司资格、硬件令牌或云签名、签名次数和税费向提供商确认。

### 3. Microsoft Azure Artifact Signing

- 微软推荐用于非 Store 公开分发，支持 CI/CD，约 10 美元/月（以微软当前页面为准）。
- 但截至核查日期，其 Public Trust 资格只覆盖美国、加拿大、欧盟、英国的组织，以及美国、加拿大的个人开发者；中国大陆个人/组织不在公开信任资格列表中。
- Private Trust 不会被普通 Windows 用户默认信任，不适合公开下载应用。

### 4. 自签名证书

- 免费，可以验证内部构建是否被改动。
- 普通用户电脑不信任它；微软说明其 SmartScreen 效果与未签名相同。
- 只适合自己或可统一安装根证书的内部测试机。

## 对 Paper Float Translator 的推荐

1. 若项目确定采用真正的 OSI 开源许可证并保持全部代码开源，优先完善仓库治理后申请 SignPath Foundation。
2. 若未来需要闭源、商业双许可、显示自己的个人/公司法定发布者名称，选择支持申请人所在地的商业 CA 或云签名服务。
3. 不建议为了 SmartScreen 购买 EV；微软已说明 EV 不再自动绕过 SmartScreen。
4. 获得签名服务后，将签名接入 Tauri 的 `bundle.windows.signCommand` 或证书指纹配置，并签署主程序及 NSIS 安装器；每次签名使用 SHA-256 和 RFC 3161 时间戳。
5. 发布前用 `Get-AuthenticodeSignature` 与 `signtool verify /pa /v` 校验签名，再做干净 Windows 虚拟机安装/升级/卸载测试。

## 2026-08-12 仓库正式开源化核查补充

- 项目采用 Apache License 2.0，不采用商业双许可。
- 当前 npm/Cargo 依赖元数据未发现专有许可证；允许清单与锁文件审计已加入仓库。
- 已删除不再被构建引用的旧 `paper-float-watcher-aarch64-apple-darwin` 预编译文件；当前 macOS native bridge 由公开的 Objective-C 源码在构建期编译。
- `PRIVACY.md` 明确：选区检测在本机进行；仅在用户点击翻译/术语解释后，文本、目标语言、模式和术语表提示直连 DeepSeek；项目没有自营中转或遥测服务器。
- `CODE_SIGNING_POLICY.md` 明确角色、MFA、可验证构建、手工批准、签名范围与事故处置，并明确当前 beta 尚未签名。
- `.github/workflows/windows-release-candidate.yml` 只生成未签名候选包、SHA-256、SPDX SBOM 和构建来源证明，不自动发布，也不伪造尚未取得的 SignPath 标识。
- 外部未完成项包括：把改动公开到 GitHub、启用仓库保护与 MFA、先发布有文档的 unsigned beta、提交 SignPath 申请、获批后接入 SignPath 给出的 organization/project/policy identifiers。

## 主要来源

- Microsoft SmartScreen reputation: https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/smartscreen-reputation
- Microsoft Authenticode timestamping: https://learn.microsoft.com/en-us/windows/win32/seccrypto/time-stamping-authenticode-signatures
- Microsoft Artifact Signing FAQ: https://learn.microsoft.com/en-us/azure/artifact-signing/faq
- Tauri Windows code signing: https://v2.tauri.app/distribute/sign/windows/
- SignPath Foundation conditions: https://signpath.org/terms.html
- SignPath application page: https://signpath.org/apply.html

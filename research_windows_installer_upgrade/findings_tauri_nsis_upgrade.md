# Paper Float Translator Windows 安装与卸载核查

核查日期：2026-08-12

## 结论

- 当前发布物是 Tauri 生成的 NSIS `-setup.exe` 安装程序。
- NSIS 安装时会在应用安装目录生成 `uninstall.exe`；卸载器不是另行发布的独立安装包。
- 当前 D 盘旧版目录中已确认存在 `uninstall.exe`，旧版主程序版本为 `0.1.0`。
- 新安装包版本为 `0.1.1`，并与旧版使用相同产品名和应用标识。
- 当前用户的标准 Windows 卸载注册表位置没有找到这份旧安装的登记。因此不建议依赖安装器自动识别并覆盖旧版；更稳妥的方式是先运行旧目录中的 `uninstall.exe`，保留应用数据，再安装新版。
- 卸载确认页包含“删除应用数据”选项。需要保留设置时必须保持该选项未勾选。
- 新安装器提供安装目录选择页；由于默认的当前用户安装位置位于 `%LOCALAPPDATA%`，安装时应手动选择 D 盘目标目录。
- 新安装包当前未做 Windows 数字签名，Windows 可能显示 SmartScreen 提醒。

## 本机核查对象

- 旧版：`D:\Program Files\paper float\Paper Float Translator\paper-float-translator.exe`（0.1.0）
- 旧版卸载器：`D:\Program Files\paper float\Paper Float Translator\uninstall.exe`
- 新版安装包：`E:\Project\translate\target\cargo-windows\release\bundle\nsis\Paper Float Translator_0.1.1_x64-setup.exe`

## 官方资料

- Tauri 官方说明 Windows 应用可以通过 NSIS `-setup.exe` 分发：https://v2.tauri.app/distribute/windows-installer/
- Tauri Windows 安装器配置参考：https://v2.tauri.app/reference/config/#windowsconfig

# Windows 划词意图门控：公开资料结论

## 结论

Windows UI Automation 的 `TextSelectionChanged` 只能说明选区被修改，不能证明用户刚刚进行了鼠标划词。自动浮窗必须由独立的物理鼠标手势授权；UIA 事件只能用于读取或辅助确认选区，不能单独触发浮窗。

## 资料与事实

1. Microsoft 对 `TextPatternIdentifiers.TextSelectionChangedEvent` 的定义是：文本选区被修改时触发。有些控件把插入点视为零宽选区，因此仅移动光标也可能触发事件。
   - https://learn.microsoft.com/en-us/dotnet/api/system.windows.automation.textpatternidentifiers.textselectionchangedevent?view=windowsdesktop-10.0

2. Microsoft 的 UI Automation 文本事件文档同样只把该事件定义为文本被选中或取消选中，并未提供鼠标、键盘、复制粘贴或程序调用等“行为来源”。
   - https://learn.microsoft.com/en-us/windows/win32/winauto/uiauto-handlingtextrelatedevents

3. UI Automation 的 Text/TextRange 模式本身支持客户端以编程方式导航和操作文本范围。因此，选区事件不能作为用户意图的充分证据。
   - https://learn.microsoft.com/en-us/windows/win32/winauto/uiauto-about-text-and-textrange-patterns

4. Win32 `RAWMOUSE` 明确定义了 `RI_MOUSE_LEFT_BUTTON_DOWN` 和 `RI_MOUSE_LEFT_BUTTON_UP` 作为鼠标左键状态转换，并提供原始移动增量。它适合作为用户鼠标手势的独立证据。
   - https://learn.microsoft.com/en-us/windows/win32/api/winuser/ns-winuser-rawmouse

## 对本项目的设计约束

- 单独收到 UIA 选区变化：不弹窗。
- 复制、粘贴、`Ctrl+A`、`Shift+方向键` 或应用程序自行设置选区：不自动弹窗。
- 外部应用中完成有效鼠标划词手势：延迟一个很短的稳定窗口后读取 UIA 选区。普通单击保持静默；拖选以及符合 Windows 当前双击时间和范围设置的双击/三击才算明确选择手势。
- 鼠标按下和抬起必须属于同一个外部进程；翻译器自身的鼠标操作不能授权自动弹窗。
- 授权只消费一次，并校验读取结果仍来自该外部进程，避免焦点切换后的错误弹窗。
- 在短暂的选区稳定等待期间，任何更新的键盘输入或新的鼠标按下都取消旧授权，确保后发生的用户操作优先。
- `Ctrl+Alt+T` 和 `Ctrl+C+C` 是明确的用户命令，继续作为主动兜底入口。

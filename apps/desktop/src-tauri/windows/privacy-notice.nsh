!macro NSIS_HOOK_PREINSTALL
  MessageBox MB_OKCANCEL|MB_ICONINFORMATION|MB_DEFBUTTON2 "Privacy notice / 隐私提示$\r$\n$\r$\nSelection is processed locally. Text may be sent directly to DeepSeek only after you click Translate or Explain Terms, or deliberately use the double-copy translation gesture. The project operates no telemetry or translation proxy. Normal uninstall keeps local settings.$\r$\n$\r$\n选区在本机处理。只有主动点击“翻译”或“解释术语”，或主动使用双复制翻译手势后，文本才可能直接发送给 DeepSeek。项目不发送遥测，也不运营翻译中转服务；普通卸载会保留本地设置。$\r$\n$\r$\nContinuing means you accept Apache-2.0 and the privacy policy:$\r$\nhttps://github.com/jklinkj/paper-float-translator/blob/main/PRIVACY.md$\r$\n$\r$\n继续安装表示接受 Apache-2.0 许可证与上述隐私政策。" IDOK +2
  Abort
  Nop
!macroend

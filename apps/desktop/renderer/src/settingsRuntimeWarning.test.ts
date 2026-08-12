import { describe, expect, it } from "vitest";
import { DEFAULT_SETTINGS } from "@paper-float-translator/core";
import { getSettingsRuntimeWarning } from "./settingsRuntimeWarning";
import type { SettingsPayload } from "./desktopApi";

const runningStatus = {
  available: true,
  running: true,
  message: "已运行。"
};

function settingsPayload(overrides: Partial<SettingsPayload> = {}): SettingsPayload {
  return {
    settings: DEFAULT_SETTINGS,
    hasApiKey: false,
    apiKeyStatus: "missing",
    apiKeyStorage: "system_keychain",
    runtimePlatform: "macos",
    doubleCopyStatus: runningStatus,
    selectionStatus: runningStatus,
    ...overrides
  } as SettingsPayload;
}

describe("settings runtime warnings", () => {
  it("keeps Keychain and listener failures visible at the same time", () => {
    const warning = getSettingsRuntimeWarning(
      settingsPayload({
        apiKeyStatus: "unavailable",
        runtimeWarning: "无法在后台安全读取旧 API Key。",
        doubleCopyStatus: {
          available: true,
          running: false,
          message: "键盘监听未就绪。"
        },
        selectionStatus: {
          available: true,
          running: false,
          message: "鼠标监听未就绪。"
        }
      })
    );

    expect(warning).toContain("无法在后台安全读取旧 API Key");
    expect(warning).toContain("Cmd+C+C 监听当前不可用：键盘监听未就绪");
    expect(warning).toContain("自动划词当前不可用：鼠标监听未就绪");
  });

  it("returns no warning when the Key is merely missing and both watchers are healthy", () => {
    expect(getSettingsRuntimeWarning(settingsPayload())).toBeNull();
  });

  it("does not warn when Windows selection and double-copy are intentionally disabled together", () => {
    expect(
      getSettingsRuntimeWarning(
        settingsPayload({
          runtimePlatform: "windows",
          settings: { ...DEFAULT_SETTINGS, enableSelectionPopup: false },
          doubleCopyStatus: {
            available: true,
            running: false,
            code: "selection_disabled",
            message: "Ctrl+C+C 双复制取词已在设置中关闭。"
          },
          selectionStatus: {
            available: true,
            running: false,
            code: "selection_disabled_by_setting",
            message: "Windows 选区取词已关闭。"
          }
        })
      )
    ).toBeNull();
  });

  it("uses the Windows shortcut name for a real listener failure", () => {
    const warning = getSettingsRuntimeWarning(
      settingsPayload({
        runtimePlatform: "windows",
        doubleCopyStatus: {
          available: false,
          running: false,
          code: "windows_raw_input_unavailable",
          message: "后台输入监听未能启动。"
        }
      })
    );

    expect(warning).toContain("Ctrl+C+C 监听当前不可用");
    expect(warning).not.toContain("Cmd+C+C");
  });
});

import { describe, expect, it } from "vitest";
import { getSettingsRuntimeWarning } from "./settingsRuntimeWarning";
import type { SettingsPayload } from "./desktopApi";

const runningStatus = {
  available: true,
  running: true,
  message: "已运行。"
};

function settingsPayload(overrides: Partial<SettingsPayload> = {}): SettingsPayload {
  return {
    settings: { enableSelectionPopup: true },
    hasApiKey: false,
    apiKeyStatus: "missing",
    apiKeyStorage: "system_keychain",
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
});

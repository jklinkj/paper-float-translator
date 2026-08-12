import { describe, expect, it } from "vitest";
import {
  getCapabilityPermissionActions,
  getDoubleCopyCapabilityPresentation,
  getSelectionCapabilityPresentation,
  type CapabilitySnapshot
} from "./types";

function snapshot(overrides: Partial<CapabilitySnapshot> = {}): CapabilitySnapshot {
  return {
    accessibility: { grant: "granted", health: "ready" },
    listenEvent: { grant: "granted", health: "ready" },
    mouseTap: { health: "ready", statusCode: "selection_sources_ready" },
    keyTap: { health: "ready", statusCode: "ready" },
    axSelectedTextObserver: { health: "ready", statusCode: "selection_sources_ready" },
    directSelectionRead: { health: "ready", statusCode: "selection_sources_ready" },
    clipboardDoubleCopyFallback: { health: "ready", statusCode: "ready" },
    ...overrides
  };
}

describe("capability permission actions", () => {
  it("maps each denied grant to its exact System Settings action", () => {
    expect(
      getCapabilityPermissionActions(
        snapshot({
          accessibility: { grant: "denied", health: "unavailable" },
          listenEvent: { grant: "denied", health: "unavailable" }
        })
      )
    ).toEqual(["open_accessibility_settings", "open_input_monitoring_settings"]);
  });

  it("does not send users to permissions for a granted runtime failure", () => {
    expect(
      getCapabilityPermissionActions(
        snapshot({
          mouseTap: { health: "disabled", statusCode: "selection_mouse_tap_disabled_timeout_fallback_ax" }
        })
      )
    ).toEqual([]);
  });
});

describe("capability presentation", () => {
  it("reports recovered sources as fully ready", () => {
    expect(
      getSelectionCapabilityPresentation(
        snapshot({ mouseTap: { health: "ready", statusCode: "selection_mouse_tap_disabled_timeout_recovered" } })
      ).value
    ).toBe("完整可用");
  });

  it("reports AX fallback as degraded rather than unavailable", () => {
    const presentation = getSelectionCapabilityPresentation(
      snapshot({ mouseTap: { health: "disabled", statusCode: "selection_mouse_tap_disabled_timeout_fallback_ax" } })
    );

    expect(presentation.value).toBe("降级可用");
    expect(presentation.tone).toBe("warning");
  });

  it("does not call the mouse selection path degraded while Settings is frontmost", () => {
    const presentation = getSelectionCapabilityPresentation(
      snapshot({
        mouseTap: {
          health: "ready",
          statusCode: "selection_mouse_ready_ax_observer_limited"
        },
        axSelectedTextObserver: {
          health: "degraded",
          statusCode: "selection_mouse_ready_ax_observer_limited"
        },
        directSelectionRead: {
          health: "ready",
          statusCode: "selection_mouse_ready_ax_observer_limited"
        }
      })
    );

    expect(presentation.value).toBe("划词就绪");
    expect(presentation.tone).toBe("ok");
    expect(presentation.detail).toContain("切换到文档后");
  });

  it("makes an ambiguous pasteboard delta visible", () => {
    const presentation = getDoubleCopyCapabilityPresentation(
      snapshot({
        keyTap: { health: "unavailable", statusCode: "key_tap_unavailable_fallback_ready" },
        clipboardDoubleCopyFallback: {
          health: "degraded",
          statusCode: "pasteboard_fallback_delta_ambiguous"
        }
      })
    );

    expect(presentation.value).toBe("需再次复制");
    expect(presentation.detail).toContain("无法证明");
  });

  it("distinguishes the healthy key tap from the inactive clipboard fallback", () => {
    const presentation = getDoubleCopyCapabilityPresentation(
      snapshot({
        keyTap: { health: "ready", statusCode: "ready" },
        clipboardDoubleCopyFallback: { health: "disabled", statusCode: "inactive_key_tap_ready" }
      })
    );

    expect(presentation.value).toBe("按键监听可用");
    expect(presentation.detail).toContain("无需启用");
  });

  it("presents the registered Windows UIA shortcut as ready when automatic selection is off", () => {
    const presentation = getSelectionCapabilityPresentation(
      snapshot({
        accessibility: {
          grant: "granted",
          health: "ready",
          statusCode: "windows_uia_no_consent_required"
        },
        mouseTap: {
          health: "disabled",
          statusCode: "windows_auto_selection_disabled_by_setting"
        },
        axSelectedTextObserver: {
          health: "disabled",
          statusCode: "windows_auto_selection_disabled_by_setting"
        },
        directSelectionRead: { health: "unknown", statusCode: "windows_uia_not_probed" }
      })
    );

    expect(presentation.value).toBe("Ctrl+Alt+T 可用");
    expect(presentation.tone).toBe("ok");
    expect(presentation.detail).toContain("全局快捷键已注册");
  });

  it("reports Windows automatic selection only when both maintained trigger paths are ready", () => {
    const presentation = getSelectionCapabilityPresentation(
      snapshot({
        accessibility: {
          grant: "granted",
          health: "ready",
          statusCode: "windows_uia_no_consent_required"
        },
        mouseTap: { health: "ready", statusCode: "windows_raw_mouse_ready" },
        axSelectedTextObserver: { health: "ready", statusCode: "windows_uia_observer_ready" },
        directSelectionRead: { health: "unknown", statusCode: "windows_uia_not_probed" }
      })
    );

    expect(presentation.value).toBe("鼠标划词 + Ctrl+Alt+T 可用");
    expect(presentation.tone).toBe("ok");
    expect(presentation.detail).toContain("复制粘贴与键盘选择不会自动弹出");
    expect(presentation.detail).toContain("不会改写剪贴板");
  });

  it("describes Windows double-copy as observed user input rather than a clipboard polling fallback", () => {
    const presentation = getDoubleCopyCapabilityPresentation(
      snapshot({
        accessibility: {
          grant: "granted",
          health: "ready",
          statusCode: "windows_uia_no_consent_required"
        },
        keyTap: { health: "ready", statusCode: "windows_raw_input_ready" },
        clipboardDoubleCopyFallback: { health: "ready", statusCode: "windows_double_copy_ready" }
      })
    );

    expect(presentation.value).toBe("Ctrl+C+C 可用");
    expect(presentation.tone).toBe("ok");
    expect(presentation.detail).toContain("不会模拟按键或轮询剪贴板");
  });
});

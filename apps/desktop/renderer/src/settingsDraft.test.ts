import { describe, expect, it } from "vitest";
import { DEFAULT_SETTINGS } from "@paper-float-translator/core";
import { hasUnsavedSettingsChanges } from "./settingsDraft";

describe("settings draft state", () => {
  it("keeps the loaded settings clean", () => {
    expect(
      hasUnsavedSettingsChanges(
        DEFAULT_SETTINGS,
        { ...DEFAULT_SETTINGS },
        String(DEFAULT_SETTINGS.popupWidth),
        ""
      )
    ).toBe(false);
  });

  it("detects an unsaved field without treating it as active", () => {
    expect(
      hasUnsavedSettingsChanges(
        DEFAULT_SETTINGS,
        { ...DEFAULT_SETTINGS, enableSelectionPopup: !DEFAULT_SETTINGS.enableSelectionPopup },
        String(DEFAULT_SETTINGS.popupWidth),
        ""
      )
    ).toBe(true);
  });

  it("detects invalid or changed width input and a pending key", () => {
    expect(hasUnsavedSettingsChanges(DEFAULT_SETTINGS, DEFAULT_SETTINGS, "", "")).toBe(true);
    expect(hasUnsavedSettingsChanges(DEFAULT_SETTINGS, DEFAULT_SETTINGS, "520", "")).toBe(true);
    expect(
      hasUnsavedSettingsChanges(
        DEFAULT_SETTINGS,
        DEFAULT_SETTINGS,
        String(DEFAULT_SETTINGS.popupWidth),
        "  sk-pending  "
      )
    ).toBe(true);
  });

  it("ignores whitespace-only key input", () => {
    expect(
      hasUnsavedSettingsChanges(
        DEFAULT_SETTINGS,
        DEFAULT_SETTINGS,
        String(DEFAULT_SETTINGS.popupWidth),
        "   "
      )
    ).toBe(false);
  });
});

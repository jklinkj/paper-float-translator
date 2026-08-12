import type { SettingsPayload } from "./desktopApi";

export function getSettingsRuntimeWarning(payload: SettingsPayload): string | null {
  const warnings = new Set<string>();
  const copyShortcut = payload.runtimePlatform === "windows" ? "Ctrl+C+C" : "Cmd+C+C";
  if (payload.runtimeWarning) {
    warnings.add(payload.runtimeWarning);
  }
  const intentionallyDisabled =
    !payload.settings.enableSelectionPopup && payload.doubleCopyStatus.code === "selection_disabled";
  if (!payload.doubleCopyStatus.running && !intentionallyDisabled) {
    warnings.add(`${copyShortcut} 监听当前不可用：${payload.doubleCopyStatus.message}`);
  }
  if (
    !payload.settings.enableSelectionPopup &&
    payload.selectionStatus.code !== "selection_disabled_by_setting"
  ) {
    warnings.add(`自动划词停用尚未确认：${payload.selectionStatus.message}`);
  }
  if (payload.settings.enableSelectionPopup && !payload.selectionStatus.running) {
    warnings.add(`自动划词当前不可用：${payload.selectionStatus.message}`);
  }
  return warnings.size > 0 ? [...warnings].join("；") : null;
}

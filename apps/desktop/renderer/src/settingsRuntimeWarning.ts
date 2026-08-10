import type { SettingsPayload } from "./desktopApi";

export function getSettingsRuntimeWarning(payload: SettingsPayload): string | null {
  const warnings = new Set<string>();
  if (payload.runtimeWarning) {
    warnings.add(payload.runtimeWarning);
  }
  if (!payload.doubleCopyStatus.running) {
    warnings.add(`Cmd+C+C 监听当前不可用：${payload.doubleCopyStatus.message}`);
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

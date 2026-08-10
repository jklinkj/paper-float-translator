import type { AppSettings } from "@paper-float-translator/core/renderer";

export function hasUnsavedSettingsChanges(
  saved: AppSettings,
  draft: AppSettings,
  popupWidthInput: string,
  pendingApiKey: string
): boolean {
  return (
    pendingApiKey.trim().length > 0 ||
    popupWidthInput.trim() !== String(saved.popupWidth) ||
    saved.model !== draft.model ||
    saved.mode !== draft.mode ||
    saved.cleanPdfText !== draft.cleanPdfText ||
    saved.enableCache !== draft.enableCache ||
    saved.enableSelectionPopup !== draft.enableSelectionPopup ||
    saved.targetLanguage !== draft.targetLanguage
  );
}

import type { PopupErrorKind } from "@paper-float-translator/core/renderer";

export function compactPopupPreview(value?: string, maxGraphemes = 96): string {
  const normalized = (value ?? "").replace(/\s+/g, " ").trim();
  if (!normalized || maxGraphemes <= 0) {
    return "";
  }

  const graphemes = segmentGraphemes(normalized);
  return graphemes.length > maxGraphemes
    ? `${graphemes.slice(0, maxGraphemes).join("")}...`
    : normalized;
}

export function popupErrorTitle(kind?: PopupErrorKind): string {
  switch (kind) {
    case "configuration":
      return "需要完成设置";
    case "permission":
      return "需要系统权限";
    case "selection":
      return "无法读取所选内容";
    case "translation":
      return "翻译失败";
    case "protocol":
      return "浮窗连接异常";
    default:
      return "操作失败";
  }
}

function segmentGraphemes(value: string): string[] {
  if (typeof Intl.Segmenter === "function") {
    const segmenter = new Intl.Segmenter(undefined, { granularity: "grapheme" });
    return Array.from(segmenter.segment(value), ({ segment }) => segment);
  }
  return Array.from(value);
}

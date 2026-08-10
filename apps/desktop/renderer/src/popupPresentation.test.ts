import { describe, expect, it } from "vitest";
import { compactPopupPreview, popupErrorTitle } from "./popupPresentation";

describe("popup presentation", () => {
  it("normalizes whitespace without truncating short text", () => {
    expect(compactPopupPreview("  one\n\n two\tthree  ")).toBe("one two three");
  });

  it("truncates by grapheme without splitting emoji sequences", () => {
    expect(compactPopupPreview("A👩‍🔬B", 2)).toBe("A👩‍🔬...");
    expect(compactPopupPreview("😀😀😀", 2)).toBe("😀😀...");
  });

  it("uses explicit user-facing error categories", () => {
    expect(popupErrorTitle("configuration")).toBe("需要完成设置");
    expect(popupErrorTitle("permission")).toBe("需要系统权限");
    expect(popupErrorTitle("selection")).toBe("无法读取所选内容");
    expect(popupErrorTitle("translation")).toBe("翻译失败");
    expect(popupErrorTitle("protocol")).toBe("浮窗连接异常");
    expect(popupErrorTitle("unknown")).toBe("操作失败");
  });
});

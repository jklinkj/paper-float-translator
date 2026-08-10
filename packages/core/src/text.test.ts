import { describe, expect, it } from "vitest";
import { cleanSelectedText } from "./text";

describe("cleanSelectedText", () => {
  it("repairs PDF hyphenation", () => {
    expect(cleanSelectedText("The pro-\nposed method works.")).toBe("The proposed method works.");
  });

  it("keeps real hyphenated compounds when the right side is also hyphenated", () => {
    expect(cleanSelectedText("state-\nof-the-art performance")).toBe("state-of-the-art performance");
  });

  it("collapses newlines and repeated spaces", () => {
    expect(cleanSelectedText("line one\n  line two    line three")).toBe("line one line two line three");
  });

  it("can disable cleaning", () => {
    expect(cleanSelectedText("line one\nline two", { enabled: false })).toBe("line one\nline two");
  });

  it("normalizes CRLF and bare CR before repairing words", () => {
    expect(cleanSelectedText("pro-\r\nposed and con-\rcluded")).toBe("proposed and concluded");
  });

  it("removes discretionary soft hyphens", () => {
    expect(cleanSelectedText("inter\u00ad\nnational co\u00adoperate")).toBe(
      "international cooperate"
    );
  });

  it("preserves visible Unicode hyphens across a line break", () => {
    expect(cleanSelectedText("well\u2010\nbeing and non\u2011\nbreaking")).toBe(
      "well\u2010being and non\u2011breaking"
    );
  });

  it("keeps known compound boundaries", () => {
    expect(cleanSelectedText("evidence-\nbased and risk-\naware")).toBe(
      "evidence-based and risk-aware"
    );
    expect(cleanSelectedText("section-\na")).toBe("section-a");
  });

  it("does not remove hyphens outside the supported English word shape", () => {
    expect(cleanSelectedText("x-\naxis 中文-\n换行")).toBe("x- axis 中文- 换行");
  });

  it("handles Chinese paragraphs, punctuation, and empty input", () => {
    expect(cleanSelectedText("第一段，\n  包含标点。\n\n第二段！")).toBe(
      "第一段， 包含标点。 第二段！"
    );
    expect(cleanSelectedText(" \t\r\n\f ")).toBe("");
  });

  it("keeps disabled cleaning to a trim-only operation", () => {
    expect(cleanSelectedText(" \tfirst\r\n  second\v  ", { enabled: false })).toBe(
      "first\r\n  second"
    );
  });
});

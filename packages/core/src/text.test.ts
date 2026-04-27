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
});

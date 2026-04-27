import { describe, expect, it } from "vitest";
import { buildSystemPrompt, normalizeTerminologyOutput } from "./prompt";

describe("buildSystemPrompt", () => {
  it("constrains terminology output to plain text lines", () => {
    const prompt = buildSystemPrompt("terminology");

    expect(prompt).toContain("不要使用 Markdown");
    expect(prompt).toContain("每行固定使用格式：英文术语：中文译名。说明：一句话解释。");
    expect(prompt).toContain("最多输出 8 条");
  });
});

describe("normalizeTerminologyOutput", () => {
  it("removes markdown wrappers and normalizes parenthesized translations", () => {
    const output = normalizeTerminologyOutput(`
**关键术语解释与推荐中文译名：**

1. **baseline (基线模型)**：指研究中的基准理论或参照框架。
2. **analytically parsimonious (分析上简洁/简练)**：模型用最少的假设捕捉核心机制。

**推荐译法**：上述术语中文译名已按学术惯例标注，可直用。
`);

    expect(output).toBe(
      [
        "baseline：基线模型。说明：指研究中的基准理论或参照框架。",
        "analytically parsimonious：分析上简洁/简练。说明：模型用最少的假设捕捉核心机制。"
      ].join("\n")
    );
  });

  it("keeps useful fallback lines after markdown cleanup", () => {
    const output = normalizeTerminologyOutput("- **implementability**：可实施性，指机制能否真实激励参与者。");

    expect(output).toBe("implementability：可实施性，指机制能否真实激励参与者。");
  });

  it("keeps already normalized lines stable", () => {
    const output = normalizeTerminologyOutput("baseline：基线模型。说明：研究中的基准理论。");

    expect(output).toBe("baseline：基线模型。说明：研究中的基准理论。");
  });
});

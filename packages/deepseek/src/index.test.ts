import { describe, expect, it } from "vitest";
import { DeepSeekClient, DeepSeekApiError } from "./index";

describe("DeepSeekClient", () => {
  it("sends chat completion requests with thinking disabled", async () => {
    let requestBody: unknown;
    const fetchImpl = async (_url: string | URL | Request, init?: RequestInit): Promise<Response> => {
      requestBody = JSON.parse(String(init?.body));
      return new Response(
        JSON.stringify({
          choices: [{ message: { role: "assistant", content: "译文" } }]
        }),
        { status: 200, headers: { "Content-Type": "application/json" } }
      );
    };

    const client = new DeepSeekClient({ apiKey: "test-key", fetchImpl });
    const result = await client.translate({
      text: "This is a paper.",
      model: "deepseek-v4-flash",
      mode: "academic_zh",
      targetLanguage: "中文"
    });

    expect(result.translation).toBe("译文");
    expect(requestBody).toMatchObject({
      model: "deepseek-v4-flash",
      thinking: { type: "disabled" },
      stream: false
    });
  });

  it("builds the system prompt with target language and no English source requirement", async () => {
    let requestBody: { messages?: Array<{ role: string; content: string }> } | undefined;
    const fetchImpl = async (_url: string | URL | Request, init?: RequestInit): Promise<Response> => {
      requestBody = JSON.parse(String(init?.body));
      return new Response(
        JSON.stringify({
          choices: [{ message: { role: "assistant", content: "翻訳" } }]
        }),
        { status: 200, headers: { "Content-Type": "application/json" } }
      );
    };

    const client = new DeepSeekClient({ apiKey: "test-key", fetchImpl });
    await client.translate({
      text: "这是一段中文内容。",
      model: "deepseek-v4-flash",
      mode: "academic_zh",
      targetLanguage: "日文"
    });

    expect(requestBody?.messages?.[0]?.content).toContain("自动识别用户提供的原文语言");
    expect(requestBody?.messages?.[0]?.content).toContain("翻译为日文");
    expect(requestBody?.messages?.[0]?.content).not.toContain("英文论文内容");
    expect(requestBody?.messages?.[0]?.content).not.toContain("请提供英文");
  });

  it("maps auth errors to readable errors", async () => {
    const fetchImpl = async (): Promise<Response> => {
      return new Response("unauthorized", { status: 401 });
    };

    const client = new DeepSeekClient({ apiKey: "bad-key", fetchImpl });

    await expect(
      client.translate({
        text: "hello",
        model: "deepseek-v4-flash",
        mode: "academic_zh",
        targetLanguage: "中文"
      })
    ).rejects.toBeInstanceOf(DeepSeekApiError);
  });

  it("normalizes terminology responses before returning them", async () => {
    const fetchImpl = async (): Promise<Response> => {
      return new Response(
        JSON.stringify({
          choices: [
            {
              message: {
                role: "assistant",
                content: "**关键术语解释：**\n1. **baseline (基线模型)**：指研究中的参照框架。"
              }
            }
          ]
        }),
        { status: 200, headers: { "Content-Type": "application/json" } }
      );
    };

    const client = new DeepSeekClient({ apiKey: "test-key", fetchImpl });
    const result = await client.translate({
      text: "baseline",
      model: "deepseek-v4-flash",
      mode: "terminology",
      targetLanguage: "中文"
    });

    expect(result.translation).toBe("baseline：基线模型。说明：指研究中的参照框架。");
  });

  it("does not normalize markdown in non-terminology responses", async () => {
    const fetchImpl = async (): Promise<Response> => {
      return new Response(
        JSON.stringify({
          choices: [{ message: { role: "assistant", content: "**译文**" } }]
        }),
        { status: 200, headers: { "Content-Type": "application/json" } }
      );
    };

    const client = new DeepSeekClient({ apiKey: "test-key", fetchImpl });
    const result = await client.translate({
      text: "This is a paper.",
      model: "deepseek-v4-flash",
      mode: "academic_zh",
      targetLanguage: "中文"
    });

    expect(result.translation).toBe("**译文**");
  });
});

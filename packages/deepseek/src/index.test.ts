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
      mode: "academic_zh"
    });

    expect(result.translation).toBe("译文");
    expect(requestBody).toMatchObject({
      model: "deepseek-v4-flash",
      thinking: { type: "disabled" },
      stream: false
    });
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
        mode: "academic_zh"
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
      mode: "terminology"
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
      mode: "academic_zh"
    });

    expect(result.translation).toBe("**译文**");
  });
});

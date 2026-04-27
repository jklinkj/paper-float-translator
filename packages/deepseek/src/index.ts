import {
  buildSystemPrompt,
  normalizeTargetLanguage,
  normalizeTerminologyOutput,
  type TranslateRequest,
  type TranslateResult
} from "@paper-float-translator/core";

export const DEEPSEEK_BASE_URL = "https://api.deepseek.com";

export interface DeepSeekClientOptions {
  apiKey: string;
  baseUrl?: string;
  fetchImpl?: typeof fetch;
}

interface DeepSeekMessage {
  role: "system" | "user" | "assistant";
  content: string;
}

interface DeepSeekChatChoice {
  message?: DeepSeekMessage;
}

interface DeepSeekChatCompletionResponse {
  choices?: DeepSeekChatChoice[];
}

export class DeepSeekApiError extends Error {
  constructor(
    readonly status: number | undefined,
    message: string
  ) {
    super(message);
    this.name = "DeepSeekApiError";
  }
}

export class DeepSeekClient {
  private readonly apiKey: string;
  private readonly baseUrl: string;
  private readonly fetchImpl: typeof fetch;

  constructor(options: DeepSeekClientOptions) {
    this.apiKey = options.apiKey;
    this.baseUrl = (options.baseUrl ?? DEEPSEEK_BASE_URL).replace(/\/+$/, "");
    this.fetchImpl = options.fetchImpl ?? globalThis.fetch;

    if (!this.fetchImpl) {
      throw new DeepSeekApiError(undefined, "当前运行环境不支持 fetch。");
    }
  }

  async translate(request: TranslateRequest): Promise<TranslateResult> {
    const cleanedText = request.text.trim();
    const targetLanguage = normalizeTargetLanguage(request.targetLanguage);

    if (!cleanedText) {
      return {
        cleanedText,
        translation: "",
        cached: false
      };
    }

    const response = await this.fetchImpl(`${this.baseUrl}/chat/completions`, {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        Authorization: `Bearer ${this.apiKey}`
      },
      body: JSON.stringify({
        model: request.model,
        thinking: { type: "disabled" },
        stream: false,
        messages: [
          {
            role: "system",
            content: buildSystemPrompt(request.mode, targetLanguage, request.glossary)
          },
          {
            role: "user",
            content: cleanedText
          }
        ]
      })
    });

    if (!response.ok) {
      const errorBody = await safeReadResponseText(response);
      throw new DeepSeekApiError(response.status, buildReadableError(response.status, errorBody));
    }

    const data = (await response.json()) as DeepSeekChatCompletionResponse;
    const rawTranslation = data.choices?.[0]?.message?.content?.trim();
    const translation =
      request.mode === "terminology" && rawTranslation
        ? normalizeTerminologyOutput(rawTranslation)
        : rawTranslation;

    if (!translation) {
      throw new DeepSeekApiError(response.status, "DeepSeek 响应中没有可用译文。");
    }

    return {
      cleanedText,
      translation,
      cached: false
    };
  }
}

async function safeReadResponseText(response: Response): Promise<string> {
  try {
    return await response.text();
  } catch {
    return "";
  }
}

function buildReadableError(status: number, body: string): string {
  if (status === 401 || status === 403) {
    return "DeepSeek API Key 无效或没有权限。";
  }

  if (status === 429) {
    return "DeepSeek 请求过于频繁，请稍后再试。";
  }

  if (status >= 500) {
    return "DeepSeek 服务暂时不可用，请稍后再试。";
  }

  return body ? `DeepSeek 请求失败：${body}` : `DeepSeek 请求失败，状态码 ${status}。`;
}

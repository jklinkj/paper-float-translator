import type { Glossary, TranslateMode } from "./types";

const MODE_INSTRUCTIONS: Record<TranslateMode, string> = {
  academic_zh:
    "请将英文论文内容翻译为准确、自然、适合中文学术阅读的中文。只输出译文，不要解释。",
  bilingual:
    "请输出中英对照。先给中文译文，再保留英文原文。格式保持简洁，适合论文精读。",
  literal:
    "请尽量贴近原文结构直译，同时保证中文可读。只输出译文，不要解释。",
  natural:
    "请以自然流畅的中文重述原文含义，适合快速阅读论文。只输出译文，不要解释。",
  terminology:
    "请解释原文中的关键学术术语，并给出推荐中文译名。输出应简洁。"
};

export function buildSystemPrompt(mode: TranslateMode, glossary?: Glossary): string {
  const glossaryBlock = buildGlossaryBlock(glossary);

  return [
    "你是专业的英文学术论文翻译助手。",
    "保留公式、变量名、引用编号、专有名词。",
    "必要时在中文译名后用括号保留英文术语。",
    MODE_INSTRUCTIONS[mode],
    glossaryBlock
  ]
    .filter(Boolean)
    .join("\n");
}

function buildGlossaryBlock(glossary?: Glossary): string {
  const entries = Object.entries(glossary ?? {}).filter(([source, target]) => {
    return source.trim().length > 0 && target.trim().length > 0;
  });

  if (entries.length === 0) {
    return "";
  }

  const lines = entries.map(([source, target]) => `- ${source}: ${target}`);
  return ["请优先使用以下术语表：", ...lines].join("\n");
}

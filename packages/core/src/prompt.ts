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
    [
      "请解释原文中的关键学术术语，并给出推荐中文译名。",
      "最多输出 8 条。",
      "不要使用 Markdown、标题、加粗、编号、项目符号或表格。",
      "不要输出开头说明、结尾总结或“推荐译法”等额外内容。",
      "每行固定使用格式：英文术语：中文译名。说明：一句话解释。",
      "没有关键术语时只输出：未发现需要解释的关键术语。"
    ].join("\n")
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

export function normalizeTerminologyOutput(output: string): string {
  return output
    .split(/\r?\n/)
    .map((line) => normalizeTerminologyLine(line))
    .filter((line): line is string => Boolean(line))
    .join("\n")
    .trim();
}

function normalizeTerminologyLine(line: string): string | null {
  const cleanedLine = stripMarkdownSyntax(line);

  if (!cleanedLine || isTerminologyWrapperLine(cleanedLine)) {
    return null;
  }

  const parenthesizedTranslation = cleanedLine.match(/^(.+?)\s*[（(]([^()（）]+)[）)]\s*[：:]\s*(.+)$/);

  if (parenthesizedTranslation) {
    const [, source, target, explanation] = parenthesizedTranslation;
    return formatTerminologyLine(source, target, explanation);
  }

  const alreadyNormalized = cleanedLine.match(/^(.+?)\s*[：:]\s*(.+?)。说明[：:]\s*(.+)$/);

  if (alreadyNormalized) {
    const [, source, target, explanation] = alreadyNormalized;
    return formatTerminologyLine(source, target, explanation);
  }

  return cleanedLine;
}

function stripMarkdownSyntax(line: string): string {
  return line
    .trim()
    .replace(/^#{1,6}\s+/, "")
    .replace(/^[-*+]\s+/, "")
    .replace(/^\d+[.)、]\s*/, "")
    .replace(/\*\*/g, "")
    .replace(/[`_]/g, "")
    .replace(/^\|+|\|+$/g, "")
    .replace(/\s*\|\s*/g, " ")
    .replace(/\s+/g, " ")
    .trim();
}

function isTerminologyWrapperLine(line: string): boolean {
  const normalized = line.replace(/[：:。.\s]/g, "");

  if (!normalized) {
    return true;
  }

  if (/^-+$/.test(normalized)) {
    return true;
  }

  return [
    /关键术语.*解释/,
    /推荐.*译法/,
    /推荐.*译名/,
    /上述术语/,
    /可直用/,
    /总结/
  ].some((pattern) => pattern.test(normalized));
}

function formatTerminologyLine(source: string, target: string, explanation: string): string {
  const normalizedSource = source.trim();
  const normalizedTarget = stripTrailingSentencePunctuation(target);
  const normalizedExplanation = stripLeadingExplanationLabel(explanation);

  return `${normalizedSource}：${normalizedTarget}。说明：${normalizedExplanation}`;
}

function stripTrailingSentencePunctuation(value: string): string {
  return value.trim().replace(/[。.!！?？]+$/g, "");
}

function stripLeadingExplanationLabel(value: string): string {
  return value
    .trim()
    .replace(/^说明\s*[：:]\s*/, "")
    .replace(/^解释\s*[：:]\s*/, "");
}

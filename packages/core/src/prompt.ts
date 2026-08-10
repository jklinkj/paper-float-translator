import { DEFAULT_TARGET_LANGUAGE, normalizeTargetLanguage, type Glossary, type TranslateMode } from "./types";

const MODE_INSTRUCTIONS: Record<TranslateMode, (targetLanguage: string) => string> = {
  academic_zh: (targetLanguage) =>
    `请将用户提供的原文翻译为准确、自然、适合${targetLanguage}学术阅读的${targetLanguage}。只输出译文，不要解释。`,
  bilingual: (targetLanguage) =>
    `请输出${targetLanguage}译文与原文对照。先给${targetLanguage}译文，再保留用户提供的原文。格式保持简洁，适合论文精读。`,
  terminology: (targetLanguage) =>
    [
      `请解释原文中的关键学术术语，并给出推荐${targetLanguage}译名。`,
      "最多输出 8 条。",
      "不要使用 Markdown、标题、加粗、编号、项目符号或表格。",
      "不要输出开头说明、结尾总结或“推荐译法”等额外内容。",
      `每行固定使用格式：原文术语：${targetLanguage}译名。说明：用${targetLanguage}一句话解释。`,
      "没有关键术语时只输出：未发现需要解释的关键术语。"
    ].join("\n")
};

export function buildSystemPrompt(
  mode: TranslateMode,
  targetLanguage: string = DEFAULT_TARGET_LANGUAGE,
  glossary?: Glossary
): string {
  const normalizedTargetLanguage = normalizeTargetLanguage(targetLanguage);
  const glossaryBlock = buildGlossaryBlock(glossary);

  return [
    "你是专业的学术内容翻译助手。",
    `自动识别用户提供的原文语言，并将其翻译为${normalizedTargetLanguage}。`,
    "不要要求用户重新提供英文文本，也不要因为原文不是英文而拒绝翻译。",
    "保留公式、变量名、引用编号、专有名词。",
    `必要时在${normalizedTargetLanguage}译名后用括号保留原文术语。`,
    MODE_INSTRUCTIONS[mode](normalizedTargetLanguage),
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

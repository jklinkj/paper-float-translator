export interface CleanTextOptions {
  enabled?: boolean;
}

const KEEP_HYPHEN_RIGHT_PATTERNS = [
  /^[a-z]+-[a-z-]+$/i,
  /^(of|the|and|or|to|in|on|based|driven|free|aware|level|scale|specific|related)\b/i
];

export function cleanSelectedText(input: string, options: CleanTextOptions = {}): string {
  if (options.enabled === false) {
    return input.trim();
  }

  return input
    .replace(/\r\n?/g, "\n")
    .replace(/([A-Za-z]{2,})-\s*\n\s*([A-Za-z][A-Za-z-]*)/g, (_match, left: string, right: string) => {
      return shouldKeepHyphen(left, right) ? `${left}-${right}` : `${left}${right}`;
    })
    .replace(/[ \t]*\n+[ \t]*/g, " ")
    .replace(/[ \t\f\v]+/g, " ")
    .trim();
}

function shouldKeepHyphen(left: string, right: string): boolean {
  if (left.length <= 1 || right.length <= 1) {
    return true;
  }

  return KEEP_HYPHEN_RIGHT_PATTERNS.some((pattern) => pattern.test(right));
}

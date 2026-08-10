export interface CleanTextOptions {
  enabled?: boolean;
}

const KEEP_HYPHEN_RIGHT_PATTERNS = [
  /^[a-z]+-[a-z-]+$/i,
  /^(of|the|and|or|to|in|on|based|driven|free|aware|level|scale|specific|related)\b/i
];

const WRAPPING_CONNECTOR_PATTERN =
  /([A-Za-z]{2,})([-\u00ad\u058a\u2010\u2011\u2e17\u30a0\ufe63\uff0d])\s*\n\s*([A-Za-z][A-Za-z-]*)/g;

export function cleanSelectedText(input: string, options: CleanTextOptions = {}): string {
  if (options.enabled === false) {
    return input.trim();
  }

  return input
    .replace(/\r\n?/g, "\n")
    .replace(
      WRAPPING_CONNECTOR_PATTERN,
      (_match, left: string, connector: string, right: string) => {
        if (connector === "\u00ad") {
          return `${left}${right}`;
        }

        if (connector !== "-") {
          return `${left}${connector}${right}`;
        }

        return shouldKeepHyphen(left, right) ? `${left}-${right}` : `${left}${right}`;
      }
    )
    .replace(/\u00ad/g, "")
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

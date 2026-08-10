import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const DESKTOP_API_PATH = fileURLToPath(
  new URL("../../renderer/src/desktopApi.ts", import.meta.url)
);
const SETTINGS_VIEW_PATH = fileURLToPath(
  new URL("../../renderer/src/SettingsView.tsx", import.meta.url)
);
const POPUP_VIEW_PATH = fileURLToPath(
  new URL("../../renderer/src/PopupView.tsx", import.meta.url)
);

function sourceBetween(source: string, startMarker: string, endMarker: string): string {
  const start = source.indexOf(startMarker);
  const end = source.indexOf(endMarker, start + startMarker.length);
  expect(start, `missing start marker: ${startMarker}`).toBeGreaterThanOrEqual(0);
  expect(end, `missing end marker after: ${startMarker}`).toBeGreaterThan(start);
  return source.slice(start, end);
}

function trustedInteractionEffect(source: string): string {
  const reporter = source.indexOf("const reportTrustedSelfInteraction = (");
  expect(reporter, "trusted self-interaction reporter is missing").toBeGreaterThanOrEqual(0);
  const effectStart = source.lastIndexOf("useEffect(() => {", reporter);
  const effectEndMarker = "  }, []);";
  const effectEnd = source.indexOf(effectEndMarker, reporter);
  expect(effectStart, "reporter must live inside a React effect").toBeGreaterThanOrEqual(0);
  expect(effectEnd, "trusted self-interaction effect must have deterministic cleanup").toBeGreaterThan(
    reporter
  );
  return source.slice(effectStart, effectEnd + effectEndMarker.length);
}

describe("acceptance self-interaction privacy contract", () => {
  it("invokes the backend with only the coarse event kind and keeps the browser stub inert", () => {
    const source = readFileSync(DESKTOP_API_PATH, "utf8");
    const method = sourceBetween(
      source,
      "  recordAcceptanceSelfInteraction:",
      "  injectAcceptanceTapDisabled:"
    );

    expect(method).toMatch(
      /recordAcceptanceSelfInteraction:\s*\(\s*eventKind: "pointer" \| "keyboard"\s*\): Promise<void> =>\s*isTauriRuntime\(\)\s*\? invoke\("record_acceptance_self_interaction", \{ eventKind \}\)\s*:\s*Promise\.resolve\(\),/s
    );

    const invocation = method.match(
      /invoke\("record_acceptance_self_interaction",\s*(\{[^}]*\})\)/s
    );
    expect(invocation, "backend invocation is missing").not.toBeNull();
    expect(invocation?.[1].replace(/\s/g, "")).toBe("{eventKind}");
    expect(method).not.toMatch(/browserResult|browserShouldFail|windowLabel|window_label|coordinates/);
  });

  it.each([
    ["settings", SETTINGS_VIEW_PATH],
    ["popup", POPUP_VIEW_PATH]
  ])("reports only trusted pointer and keyboard activity from the %s window", (_name, path) => {
    const source = readFileSync(path, "utf8");
    const effect = trustedInteractionEffect(source);

    expect(effect).toContain("if (event.isTrusted !== true)");
    expect(effect).toContain(
      "void desktopApi.recordAcceptanceSelfInteraction(eventKind).catch(() => undefined);"
    );
    expect(source.match(/recordAcceptanceSelfInteraction\(/g)).toHaveLength(1);
    expect(effect).toContain('reportTrustedSelfInteraction(event, "pointer");');
    expect(effect).toContain('reportTrustedSelfInteraction(event, "keyboard");');
    expect(effect).toContain('window.addEventListener("pointerdown", handlePointerDown, true);');
    expect(effect).toContain('window.addEventListener("keydown", handleKeyDown, true);');
    expect(effect).toContain('window.removeEventListener("pointerdown", handlePointerDown, true);');
    expect(effect).toContain('window.removeEventListener("keydown", handleKeyDown, true);');
    expect(effect).not.toMatch(
      /event\.(?:key|code|target|currentTarget|clientX|clientY|screenX|screenY)|windowLabel|window_label/
    );
  });
});

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import ts from "typescript";
import { describe, expect, it } from "vitest";

const DESKTOP_API_PATH = fileURLToPath(
  new URL("../../renderer/src/desktopApi.ts", import.meta.url)
);
const CAPABILITY_PATH = fileURLToPath(
  new URL("../../src-tauri/capabilities/default.json", import.meta.url)
);

const WINDOW_PERMISSION_BY_METHOD: Readonly<Record<string, string>> = {
  show: "core:window:allow-show",
  startDragging: "core:window:allow-start-dragging"
};

function collectCurrentWindowMethods(sourceText: string): string[] {
  const source = ts.createSourceFile(
    DESKTOP_API_PATH,
    sourceText,
    ts.ScriptTarget.Latest,
    true,
    ts.ScriptKind.TS
  );
  const currentWindowFactories = new Set<string>();
  const currentWindowBindings = new Set<string>();

  for (const statement of source.statements) {
    if (
      !ts.isImportDeclaration(statement) ||
      statement.moduleSpecifier.getText(source) !== '"@tauri-apps/api/window"'
    ) {
      continue;
    }

    const bindings = statement.importClause?.namedBindings;
    if (!bindings || !ts.isNamedImports(bindings)) {
      continue;
    }

    for (const element of bindings.elements) {
      if ((element.propertyName ?? element.name).text === "getCurrentWindow") {
        currentWindowFactories.add(element.name.text);
      }
    }
  }

  const isCurrentWindowFactoryCall = (node: ts.Node): node is ts.CallExpression =>
    ts.isCallExpression(node) &&
    ts.isIdentifier(node.expression) &&
    currentWindowFactories.has(node.expression.text);

  const discoverBindings = (node: ts.Node): void => {
    if (
      ts.isVariableDeclaration(node) &&
      ts.isIdentifier(node.name) &&
      node.initializer &&
      isCurrentWindowFactoryCall(node.initializer)
    ) {
      currentWindowBindings.add(node.name.text);
    }
    ts.forEachChild(node, discoverBindings);
  };
  discoverBindings(source);

  const methods = new Set<string>();
  const discoverMethods = (node: ts.Node): void => {
    if (ts.isCallExpression(node) && ts.isPropertyAccessExpression(node.expression)) {
      const receiver = node.expression.expression;
      if (
        isCurrentWindowFactoryCall(receiver) ||
        (ts.isIdentifier(receiver) && currentWindowBindings.has(receiver.text))
      ) {
        methods.add(node.expression.name.text);
      }
    }
    ts.forEachChild(node, discoverMethods);
  };
  discoverMethods(source);

  return [...methods].sort();
}

describe("Tauri window ACL contract", () => {
  it("grants every Tauri current-window API used by the renderer", () => {
    const methods = collectCurrentWindowMethods(readFileSync(DESKTOP_API_PATH, "utf8"));
    const capability = JSON.parse(readFileSync(CAPABILITY_PATH, "utf8")) as {
      permissions?: unknown;
    };
    const permissions = Array.isArray(capability.permissions)
      ? capability.permissions.filter((permission): permission is string => typeof permission === "string")
      : [];

    expect(methods, "desktopApi.ts must continue to expose the current Tauri window calls").not.toEqual([]);

    const unmappedMethods = methods.filter((method) => !WINDOW_PERMISSION_BY_METHOD[method]);
    expect(
      unmappedMethods,
      "Map every new getCurrentWindow() method to its explicit Tauri capability permission"
    ).toEqual([]);

    const missingPermissions = methods
      .map((method) => WINDOW_PERMISSION_BY_METHOD[method])
      .filter((permission) => !permissions.includes(permission));
    expect(
      missingPermissions,
      "default.json is missing an explicit permission required by desktopApi.ts"
    ).toEqual([]);
  });
});

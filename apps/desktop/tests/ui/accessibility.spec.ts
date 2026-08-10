import AxeBuilder from "@axe-core/playwright";
import { expect, test, type Page } from "@playwright/test";

type AccessibilityCase = {
  name: string;
  path: string;
  ready: (page: Page) => Promise<void>;
};

const cases: AccessibilityCase[] = [
  {
    name: "settings ready",
    path: "/",
    ready: async (page) => {
      await expect(page.getByRole("heading", { name: "AI 翻译助手" })).toBeVisible();
    }
  },
  {
    name: "settings permission denied",
    path: "/?permission=both-denied",
    ready: async (page) => {
      await expect(page.getByRole("button", { name: "打开辅助功能权限" })).toBeVisible();
    }
  },
  {
    name: "settings load failure",
    path: "/?fail=load",
    ready: async (page) => {
      await expect(page.getByRole("alert")).toContainText("无法读取当前设置");
    }
  },
  {
    name: "settings acceptance runner",
    path: "/?acceptance=1",
    ready: async (page) => {
      await expect(page.getByRole("heading", { name: "划词可靠性验收" })).toBeVisible();
    }
  },
  {
    name: "popup pending",
    path: "/?view=popup&state=pending&fixture=long",
    ready: async (page) => {
      await expect(page.getByRole("status")).toContainText("正在读取所选内容");
    }
  },
  {
    name: "popup selection ready",
    path: "/?view=popup&state=selection&fixture=emoji",
    ready: async (page) => {
      await expect(page.getByRole("button", { name: "翻译" })).toBeVisible();
    }
  },
  {
    name: "popup translating",
    path: "/?view=popup&state=loading&streaming=1",
    ready: async (page) => {
      await expect(page.getByText("正在接收译文", { exact: true })).toBeVisible();
    }
  },
  {
    name: "popup translated RTL",
    path: "/?view=popup&state=translated&fixture=rtl",
    ready: async (page) => {
      await expect(page.locator("article.translation-text")).toBeVisible();
    }
  },
  {
    name: "popup recoverable error",
    path: "/?view=popup&state=error&error=permission-accessibility",
    ready: async (page) => {
      await expect(page.getByRole("button", { name: "打开辅助功能权限" })).toBeVisible();
    }
  },
  {
    name: "popup protocol error",
    path: "/?view=popup&state=translated&fail=protocol",
    ready: async (page) => {
      await expect(page.getByRole("alert")).toContainText("浮窗连接异常");
    }
  }
];

for (const entry of cases) {
  test(`${entry.name} has no automated accessibility violations`, async ({ page }) => {
    await page.goto(entry.path);
    await entry.ready(page);

    const results = await new AxeBuilder({ page }).analyze();
    const violations = results.violations.map((violation) => ({
      id: violation.id,
      impact: violation.impact,
      help: violation.help,
      targets: violation.nodes.flatMap((node) => node.target.map(String))
    }));

    expect(violations).toEqual([]);
  });
}

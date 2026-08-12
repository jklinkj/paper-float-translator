import { expect, test, type Page } from "@playwright/test";

const pageErrors = new WeakMap<Page, string[]>();

test.beforeEach(async ({ page }) => {
  const errors: string[] = [];
  pageErrors.set(page, errors);
  page.on("console", (message) => {
    if (message.type() === "error") {
      errors.push(message.text());
    }
  });
  page.on("pageerror", (error) => errors.push(error.message));
});

test.afterEach(async ({ page }) => {
  expect(pageErrors.get(page) ?? []).toEqual([]);
});

test("renders the complete explicit popup-state matrix", async ({ page }) => {
  await page.setViewportSize({ width: 440, height: 560 });

  await page.goto("/?view=popup&state=hidden");
  await expect(page.locator("main")).toHaveAttribute("data-status", "hidden");
  await expect(page.locator("main")).toHaveAttribute("data-visible", "false");

  await page.goto("/?view=popup&state=pending&fixture=long");
  await expect(page.getByRole("status")).toContainText("正在读取所选内容");
  await expect(page.getByRole("button", { name: "关闭" })).toBeVisible();
  await expectPopupWithinViewport(page);

  await page.goto("/?view=popup&state=selection&fixture=emoji");
  await expect(page.getByRole("button", { name: "复制" })).toBeVisible();
  await expect(page.getByRole("button", { name: "翻译" })).toBeVisible();
  await expect(page.getByRole("button", { name: "关闭" })).toBeVisible();

  await page.goto("/?view=popup&state=loading");
  await expect(page.locator("main")).toHaveAttribute("aria-busy", "true");
  await expect(page.getByText("正在翻译", { exact: true })).toBeVisible();
  await expect(page.getByRole("button", { name: "重译" })).toBeDisabled();

  await page.goto("/?view=popup&state=loading&streaming=1");
  await expect(page.getByText("正在接收译文", { exact: true })).toBeVisible();
  await expect(page.locator("article.streaming")).not.toBeEmpty();

  await page.goto("/?view=popup&state=translated&fixture=rtl");
  await expect(page.locator("article.translation-text")).toHaveAttribute("dir", "auto");
  await expect(page.locator(".source-preview p")).toHaveAttribute("dir", "auto");
  await expect(page.getByRole("button", { name: "固定窗口" })).toHaveAttribute("aria-pressed", "false");

  await page.goto("/?view=popup&state=error&fixture=long");
  await expect(page.getByRole("alert")).toContainText("翻译失败");
  await expect(page.getByRole("button", { name: "重译" })).toBeEnabled();
  await expectPopupWithinViewport(page);
});

test("uses exact recovery actions and exposes protocol/action failures", async ({ page }) => {
  await page.goto("/?view=popup&state=error&error=configuration");
  await expect(page.getByRole("button", { name: "打开设置" })).toBeVisible();
  await expect(page.getByRole("button", { name: "重译" })).toHaveCount(0);

  await page.goto("/?view=popup&state=error&error=permission-accessibility");
  await expect(page.getByRole("button", { name: "打开辅助功能权限" })).toBeVisible();
  await expect(page.getByRole("button", { name: "打开输入监听权限" })).toHaveCount(0);

  await page.goto("/?view=popup&state=error&error=permission-input");
  await expect(page.getByRole("button", { name: "打开输入监听权限" })).toBeVisible();
  await expect(page.getByRole("button", { name: "打开辅助功能权限" })).toHaveCount(0);

  await page.goto("/?view=popup&state=selection&fail=copy");
  await page.getByRole("button", { name: "复制" }).click();
  await expect(page.getByRole("status")).toContainText("操作失败");
  await expect(page.getByRole("status")).toContainText("固定测试：copy 操作失败");

  await page.goto("/?view=popup&state=translated&fail=protocol");
  await expect(page.getByRole("alert")).toContainText("浮窗连接异常");
  await expect(page.getByRole("button", { name: "重新连接" })).toBeVisible();
  await expect(page.getByRole("button", { name: "关闭" })).toBeVisible();
});

test("supports keyboard traversal, activation and Escape", async ({ page }) => {
  await page.goto("/?view=popup&state=translated");
  const pin = page.getByRole("button", { name: "固定窗口" });
  await page.keyboard.press("Tab");
  await expect(pin).toBeFocused();
  await page.keyboard.press("Space");
  await expect(page.getByRole("button", { name: "取消固定" })).toHaveAttribute("aria-pressed", "true");
  await page.keyboard.press("Escape");
  await expect(page.locator("main")).toHaveAttribute("data-visible", "false");

  await page.goto("/?view=popup&state=selection");
  await page.keyboard.press("Tab");
  await expect(page.getByRole("button", { name: "复制" })).toBeFocused();
  await page.keyboard.press("Tab");
  await expect(page.getByRole("button", { name: "翻译" })).toBeFocused();
  await page.keyboard.press("Tab");
  await expect(page.getByRole("button", { name: "关闭" })).toBeFocused();
  await page.keyboard.press("Shift+Tab");
  await expect(page.getByRole("button", { name: "翻译" })).toBeFocused();
  await page.keyboard.press("Escape");
  await expect(page.locator("main")).toHaveAttribute("data-visible", "false");
});

test("routes revisioned external Tab and Shift+Tab entry to the first and last controls", async ({ page }) => {
  await page.goto("/?view=popup&state=selection");

  await page.evaluate(() => {
    window.dispatchEvent(
      new CustomEvent("paper-float-popup-keyboard-entry", {
        detail: { revision: 0, selectionRevision: 1, direction: "forward" }
      })
    );
  });
  await expect(page.getByRole("button", { name: "复制" })).not.toBeFocused();

  await page.evaluate(() => {
    window.dispatchEvent(
      new CustomEvent("paper-float-popup-keyboard-entry", {
        detail: { revision: 1, selectionRevision: 1, direction: "forward" }
      })
    );
  });
  await expect(page.getByRole("button", { name: "复制" })).toBeFocused();

  await page.evaluate(() => {
    window.dispatchEvent(
      new CustomEvent("paper-float-popup-keyboard-entry", {
        detail: { revision: 1, selectionRevision: 1, direction: "backward" }
      })
    );
  });
  await expect(page.getByRole("button", { name: "关闭" })).toBeFocused();
  await page.keyboard.press("Escape");
  await expect(page.locator("main")).toHaveAttribute("data-visible", "false");
});

for (const width of [320, 379, 380, 420, 640]) {
  test(`keeps pending and ready controls in bounds at ${width}px`, async ({ page }) => {
    await page.setViewportSize({ width, height: 260 });
    await page.goto("/?view=popup&state=pending&fixture=unbroken");
    await expectPopupWithinViewport(page);
    await expect(page.getByRole("button", { name: "关闭" })).toBeVisible();

    await page.goto("/?view=popup&state=selection&fixture=unbroken");
    await expectPopupWithinViewport(page);
    for (const name of ["复制", "翻译", "关闭"]) {
      await expect(page.getByRole("button", { name })).toBeVisible();
    }
  });
}

test("keeps the native 440 by 66 selection popup entirely scrollbar-free", async ({ page }) => {
  await page.setViewportSize({ width: 440, height: 66 });
  await page.goto("/?view=popup&state=selection&fixture=unbroken");

  const metrics = await page.evaluate(() => {
    const shell = document.querySelector<HTMLElement>(".selection-shell");
    const actions = document.querySelector<HTMLElement>(".selection-actions");
    if (!shell || !actions) {
      return null;
    }
    const elements = [document.documentElement, document.body, document.getElementById("root"), shell, actions]
      .filter((element): element is HTMLElement => element instanceof HTMLElement);
    return {
      elementsFit: elements.every(
        (element) =>
          element.scrollWidth <= element.clientWidth + 1 &&
          element.scrollHeight <= element.clientHeight + 1
      ),
      dimensions: elements.map((element) => ({
        name:
          element === document.documentElement
            ? "html"
            : element === document.body
              ? "body"
              : element.id || element.className,
        clientWidth: element.clientWidth,
        scrollWidth: element.scrollWidth,
        clientHeight: element.clientHeight,
        scrollHeight: element.scrollHeight
      })),
      viewportFits:
        document.documentElement.scrollWidth <= window.innerWidth + 1 &&
        document.documentElement.scrollHeight <= window.innerHeight + 1,
      shellOverflow: getComputedStyle(shell).overflow,
      documentOverflow: getComputedStyle(document.documentElement).overflow
    };
  });

  expect(metrics?.elementsFit, JSON.stringify(metrics?.dimensions)).toBe(true);
  expect(metrics).toMatchObject({
    viewportFits: true,
    shellOverflow: "hidden",
    documentOverflow: "hidden"
  });
  for (const name of ["复制", "翻译", "关闭"]) {
    await expect(page.getByRole("button", { name })).toBeVisible();
  }
});

for (const fixture of ["long", "unbroken", "cjk", "emoji", "multiline", "rtl"]) {
  test(`keeps ${fixture} translated content bounded`, async ({ page }) => {
    await page.setViewportSize({ width: 420, height: 560 });
    await page.goto(`/?view=popup&state=translated&fixture=${fixture}`);
    await expectPopupWithinViewport(page);
    const image = await page.screenshot();
    expect(image.byteLength).toBeGreaterThan(1_000);
  });
}

test("requests a smaller height after closing a tall popup", async ({ page }) => {
  await page.setViewportSize({ width: 420, height: 700 });
  await page.goto("/?view=popup&state=translated&fixture=long");
  await expect(page.locator("html")).toHaveAttribute("data-popup-height", /\d+/);
  const tallHeight = Number(await page.locator("html").getAttribute("data-popup-height"));

  await page.keyboard.press("Escape");
  await expect(page.locator("main")).toHaveAttribute("data-visible", "false");
  const hiddenHeight = Number(await page.locator("html").getAttribute("data-popup-height"));

  expect(tallHeight).toBeGreaterThan(hiddenHeight);
});

test("bounds guard rejects shell and control bottom-overflow sentinels", async ({ page }) => {
  await page.setViewportSize({ width: 420, height: 560 });
  await page.goto("/?view=popup&state=selection&fixture=long");
  await expectPopupWithinViewport(page);

  await page.locator(".popup-shell").evaluate((shell) => {
    shell.style.transform = "translateY(600px)";
  });
  expect((await readPopupBounds(page)).shellInsideViewport).toBe(false);

  await page.goto("/?view=popup&state=selection&fixture=long");
  await expectPopupWithinViewport(page);
  await page.getByRole("button", { name: "关闭" }).evaluate((control) => {
    control.style.setProperty("transition", "none", "important");
    control.style.setProperty("transform", "translateY(600px)", "important");
  });
  expect((await readPopupBounds(page)).controlsInsideShell).toBe(false);
});

type PopupBounds = {
  found: boolean;
  documentFits: boolean;
  shellFits: boolean;
  shellInsideViewport: boolean;
  controlsInsideShell: boolean;
};

async function readPopupBounds(page: Page): Promise<PopupBounds> {
  return page.evaluate(() => {
    const shell = document.querySelector<HTMLElement>(".popup-shell");
    if (!shell) {
      return {
        found: false,
        documentFits: false,
        shellFits: false,
        shellInsideViewport: false,
        controlsInsideShell: false
      };
    }
    const shellRect = shell.getBoundingClientRect();
    const controls = Array.from(shell.querySelectorAll<HTMLElement>("button, select, input"));
    return {
      found: true,
      documentFits: document.documentElement.scrollWidth <= window.innerWidth + 1,
      shellFits: shell.scrollWidth <= shell.clientWidth + 1 && shell.scrollHeight <= shell.clientHeight + 1,
      shellInsideViewport:
        shellRect.left >= -1 &&
        shellRect.right <= window.innerWidth + 1 &&
        shellRect.top >= -1 &&
        shellRect.bottom <= window.innerHeight + 1,
      controlsInsideShell: controls.every((control) => {
        const rect = control.getBoundingClientRect();
        return (
          rect.left >= shellRect.left - 1 &&
          rect.right <= shellRect.right + 1 &&
          rect.top >= shellRect.top - 1 &&
          rect.bottom <= shellRect.bottom + 1
        );
      })
    };
  });
}

async function expectPopupWithinViewport(page: Page): Promise<void> {
  const bounds = await readPopupBounds(page);

  expect(bounds).toEqual({
    found: true,
    documentFits: true,
    shellFits: true,
    shellInsideViewport: true,
    controlsInsideShell: true
  });
}

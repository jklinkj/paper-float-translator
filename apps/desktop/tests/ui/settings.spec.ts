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

test("blocks editing after load failure and retries the authoritative load", async ({ page }) => {
  await page.goto("/?fail=load-once");
  await expect(page.getByRole("alert")).toContainText("无法读取当前设置");
  await expect(page.getByLabel("API Key")).toHaveCount(0);

  await page.getByRole("button", { name: "重新读取" }).click();
  await expect(page.getByRole("heading", { name: "AI 翻译助手" })).toBeVisible();
  await expect(page.getByLabel("API Key")).toBeVisible();
});

test("recovers a persistent load mismatch by safely applying the disk snapshot", async ({ page }) => {
  await page.goto("/?fail=load&runtime=external-settings-on-refresh");
  await expect(page.getByRole("alert")).toContainText("无法读取当前设置");

  await page.getByRole("button", { name: "刷新并应用磁盘设置" }).click();
  await expect(page.getByRole("heading", { name: "AI 翻译助手" })).toBeVisible();
  await expect(page.getByLabel(/拖选后显示操作浮窗/)).not.toBeChecked();
  await expect(page.getByRole("spinbutton", { name: /浮窗宽度/ })).toHaveValue("500");
});

test("applies an external authoritative refresh while preserving an unsaved local draft", async ({ page }) => {
  await page.goto("/?runtime=external-settings-on-refresh");
  const width = page.getByRole("spinbutton", { name: /浮窗宽度/ });
  await width.fill("460");

  await page.getByRole("button", { name: "刷新诊断" }).click();

  await expect(page.getByRole("alert")).toContainText("外部设置已经生效");
  await expect(width).toHaveValue("460");
  await expect(page.getByLabel(/拖选后显示操作浮窗/)).toBeChecked();
  await expect(page.getByRole("region", { name: "运行状态" })).toContainText("已关闭");
  await expect(page.getByText("草稿尚未保存")).toBeVisible();
});

test("keeps draft, validation, cancel and effective status separate", async ({ page }) => {
  await page.goto("/");
  const width = page.getByRole("spinbutton", { name: /浮窗宽度/ });
  await width.fill("700");
  await expect(page.getByText("草稿尚未保存")).toBeVisible();
  await page.getByRole("button", { name: "保存设置" }).click();
  await expect(page.getByRole("alert")).toContainText("320 到 640");

  await page.getByRole("button", { name: "取消更改" }).click();
  await expect(width).toHaveValue("420");
  await expect(page.getByText("草稿与当前生效设置一致")).toBeVisible();
  await expect(page.getByRole("button", { name: "保存设置" })).toBeDisabled();
  await expect(page.getByRole("button", { name: "取消更改" })).toBeDisabled();

  const selectionToggle = page.getByLabel(/拖选后显示操作浮窗/);
  await page.locator("label.switch-row").filter({ hasText: "拖选后显示操作浮窗" }).click();
  await expect(selectionToggle).not.toBeChecked();
  await expect(page.getByText("草稿尚未保存")).toBeVisible();
  await expect(page.getByRole("region", { name: "运行状态" })).toContainText("完整可用");
});

test("shows disabled selection as an effective source shutdown while keeping Cmd+C+C", async ({ page }) => {
  await page.goto("/");
  await page.locator("label.switch-row").filter({ hasText: "拖选后显示操作浮窗" }).click();
  await page.getByRole("button", { name: "保存设置" }).click();

  await expect(page.getByRole("region", { name: "运行状态" })).toContainText("已关闭");
  await expect(page.getByText("拖选监听已确认关闭，Cmd+C+C 仍会在后台运行。", { exact: false })).toBeVisible();
  await expect(page.getByRole("region", { name: "运行状态" })).toContainText(
    "拖选浮窗已确认关闭，仍可使用 Cmd+C+C。"
  );
  await expect(page.getByText("鼠标 tap", { exact: true })).toHaveCount(0);

  await page.locator("label.switch-row").filter({ hasText: "拖选后显示操作浮窗" }).click();
  await page.getByRole("button", { name: "保存设置" }).click();
  await expect(page.getByRole("region", { name: "运行状态" })).toContainText("完整可用");
  await expect(page.getByText("鼠标 tap", { exact: true })).toBeVisible();
});

test("shows the mouse selection path as ready while Settings cannot bind an external AX observer", async ({ page }) => {
  await page.goto("/?runtime=observer-limited");

  const runtimeStatus = page.getByRole("region", { name: "运行状态" });
  await expect(runtimeStatus).toContainText("划词就绪");
  await expect(runtimeStatus).toContainText("切换到文档后会自动重新绑定 AX 选区通知");
  await expect(runtimeStatus).not.toContainText("降级可用");
  await expect(
    page.locator("label.switch-row").filter({ hasText: "拖选后显示操作浮窗" })
  ).toContainText("切换到文档后会自动重新绑定 AX 选区通知");
});

test("does not claim selection shutdown when native release is unconfirmed", async ({ page }) => {
  await page.goto("/?runtime=disable-unconfirmed");
  await page.locator("label.switch-row").filter({ hasText: "拖选后显示操作浮窗" }).click();
  await page.getByRole("button", { name: "保存设置" }).click();

  await expect(page.getByRole("alert")).toContainText("自动划词停用尚未确认");
  await expect(page.getByRole("region", { name: "运行状态" })).toContainText("停用待确认");
  await expect(page.getByRole("region", { name: "运行状态" })).toContainText(
    "无法确认所有原生选区资源"
  );
  await expect(page.getByRole("button", { name: "刷新诊断" })).toBeVisible();
  await expect(page.getByText("Cmd+C+C 仍会在后台运行", { exact: false })).toHaveCount(0);
});

test("shows fail-closed double-copy loss and recovery instead of a healthy disabled state", async ({ page }) => {
  await page.goto("/?runtime=disable-fail-closed");
  await page.locator("label.switch-row").filter({ hasText: "拖选后显示操作浮窗" }).click();
  await page.getByRole("button", { name: "保存设置" }).click();

  await expect(page.getByRole("alert")).toContainText("Cmd+C+C 监听当前不可用");
  await expect(page.getByRole("region", { name: "运行状态" })).toContainText("已关闭 / 兜底停用");
  await expect(page.getByRole("region", { name: "运行状态" })).toContainText(
    "Cmd+C+C 监听也已暂时停止"
  );
  await expect(page.getByRole("button", { name: "刷新诊断" })).toBeVisible();
  await expect(page.getByText("Cmd+C+C 仍会在后台运行", { exact: false })).toHaveCount(0);
});

test("uses the save response as the effective watcher state without a second refresh", async ({ page }) => {
  await page.goto("/?fail=refresh");
  await page.getByRole("spinbutton", { name: /浮窗宽度/ }).fill("500");
  await page.getByRole("button", { name: "保存设置" }).click();

  await expect(page.getByText("设置已保存并已应用。", { exact: true })).toBeVisible();
  await expect(page.getByRole("spinbutton", { name: /浮窗宽度/ })).toHaveValue("500");
  await expect(page.getByRole("button", { name: "取消更改" })).toBeDisabled();
});

test("locks every draft control while an asynchronous save is in flight", async ({ page }) => {
  await page.goto("/?delay=save");
  const width = page.getByRole("spinbutton", { name: /浮窗宽度/ });
  await width.fill("500");
  await page.getByRole("button", { name: "保存设置" }).click();

  await expect(width).toBeDisabled();
  await expect(page.getByLabel(/拖选后显示操作浮窗/)).toBeDisabled();
  await expect(page.getByRole("combobox", { name: "默认模型" })).toBeDisabled();
  await expect(page.getByRole("combobox", { name: "默认目标语言" })).toBeDisabled();
  await expect(page.getByText("设置已保存并已应用。", { exact: true })).toBeVisible();
  await expect(width).toBeEnabled();
  await expect(width).toHaveValue("500");
});

test("reports an external documentation launch failure in the settings status", async ({ page }) => {
  await page.goto("/?fail=external");
  await page.getByRole("button", { name: "打开 DeepSeek API 文档" }).click();
  await expect(page.getByRole("alert")).toContainText("外部链接打开失败");
});

test("keeps the privacy-preserving acceptance runner hidden unless explicitly enabled", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByRole("heading", { name: "划词可靠性验收" })).toHaveCount(0);

  await page.goto("/?acceptance=1");
  await expect(page.getByRole("heading", { name: "划词可靠性验收" })).toBeVisible();
  await expect(page.getByText("不保存任何文本内容", { exact: false })).toBeVisible();
  await page.getByRole("button", { name: "开始固定验收会话" }).click();
  await expect(page.getByLabel("验收会话状态")).toContainText("记录中");

  await page.getByRole("combobox", { name: "验收场景" }).selectOption("rapidAThenB");
  await expect(page.getByText("完成 A 手势后不要等待浮窗", { exact: false })).toBeVisible();
  await expect(page.getByText("立即在 300 ms 内真实选择固定 B", { exact: false })).toBeVisible();

  await page.getByRole("combobox", { name: "验收场景" }).selectOption("tapDisabledRecovery");
  await page.getByRole("button", { name: "准备单次" }).click();
  await page.getByRole("button", { name: "注入一次 tap-disabled" }).click();
  await expect(page.getByText("恢复或明确降级时延", { exact: false })).toBeVisible();

  await page.getByRole("combobox", { name: "验收场景" }).selectOption("safariFixture");
  await page.getByRole("button", { name: "准备单次" }).click();
  await expect(page.getByText("当前已准备：Safari 固定 HTML", { exact: false })).toBeVisible();

  await page.getByRole("button", { name: "结束并汇总" }).click();
  await expect(page.getByText("固定验收未通过", { exact: true })).toBeVisible();
  await expect(page.getByText("多击安静窗口 Q：530 ms · 证据版本 4", { exact: true })).toBeVisible();
  await page.getByRole("button", { name: "导出无内容 JSON" }).click();
  await expect(page.getByText("selection-acceptance-report.json", { exact: false })).toBeVisible();
  await page.getByRole("button", { name: "清除会话" }).click();
  await expect(page.getByRole("button", { name: "确认清除" })).toBeVisible();
  await page.getByRole("button", { name: "确认清除" }).click();
  await expect(page.getByRole("button", { name: "开始固定验收会话" })).toBeVisible();
});

test("runs the continuous tap recovery path from a fresh acceptance session", async ({ page }) => {
  await page.goto("/?acceptance=1");
  await page.getByRole("button", { name: "开始固定验收会话" }).click();
  await page.getByRole("combobox", { name: "验收场景" }).selectOption("tapDisabledRecovery");
  await page.getByRole("button", { name: "连续记录至固定次数" }).click();

  await expect(page.getByText("事件监听恢复 的固定次数已全部记录", { exact: false })).toBeVisible();
});

test("retries a transient acceptance status IPC failure instead of silently hiding the tool", async ({ page }) => {
  await page.goto("/?acceptance=1&fail=acceptance-status-once");
  await expect(page.getByRole("heading", { name: "验收工具不可用" })).toBeVisible();
  await expect(page.getByRole("alert")).toContainText("acceptance-status 操作失败");

  await page.getByRole("button", { name: "重试读取验收状态" }).click();
  await expect(page.getByRole("heading", { name: "划词可靠性验收" })).toBeVisible();
});

test("executes the watcher restart continuous UI path for all 50 fixed ordinals", async ({ page }) => {
  test.setTimeout(60_000);
  await page.goto("/?acceptance=1");
  await page.getByRole("button", { name: "开始固定验收会话" }).click();
  await page.getByRole("combobox", { name: "验收场景" }).selectOption("watcherRestart");
  await page
    .getByRole("button", { name: "自动执行剩余重启" })
    .evaluate((button: HTMLButtonElement) => button.click());

  await expect(page.getByText("监听重启 50 次 的固定次数已全部记录", { exact: false })).toBeVisible({
    timeout: 45_000
  });
});

test("keeps the explicit acceptance runner usable in a narrow settings window", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto("/?acceptance=1");
  await expect(page.getByRole("heading", { name: "划词可靠性验收" })).toBeVisible();
  const horizontalOverflow = await page.evaluate(
    () => document.documentElement.scrollWidth > window.innerWidth + 1
  );
  expect(horizontalOverflow).toBe(false);
  const panel = await page.locator(".acceptance-panel").boundingBox();
  expect(panel).not.toBeNull();
  expect(panel?.x ?? -1).toBeGreaterThanOrEqual(0);
  expect((panel?.x ?? 0) + (panel?.width ?? 0)).toBeLessThanOrEqual(390);
  const image = await page.screenshot({ fullPage: true });
  expect(image.byteLength).toBeGreaterThan(1_000);
});

test("fails a continuous tap recovery run when injection never reaches a terminal state", async ({ page }) => {
  await page.goto("/?acceptance=1&fail=acceptance-tap-stuck");
  await page.getByRole("button", { name: "开始固定验收会话" }).click();
  await page.getByRole("combobox", { name: "验收场景" }).selectOption("tapDisabledRecovery");
  await page.getByRole("button", { name: "连续记录至固定次数" }).click();

  await expect(page.getByRole("alert")).toContainText(
    "tap-disabled 注入后 2.5 秒内没有得到终态",
    { timeout: 4_000 }
  );
  await expect(page.getByRole("button", { name: "连续记录至固定次数" })).toBeEnabled();
});

test("reports full failure and both partial-success stages truthfully", async ({ page }) => {
  await page.goto("/?fail=save");
  await page.getByRole("spinbutton", { name: /浮窗宽度/ }).fill("500");
  await page.getByRole("button", { name: "保存设置" }).click();
  await expect(page.getByRole("alert")).toContainText("设置未保存");
  await expect(page.getByRole("button", { name: "取消更改" })).toBeEnabled();

  await page.goto("/?fail=key");
  await page.getByRole("spinbutton", { name: /浮窗宽度/ }).fill("500");
  await page.getByLabel("API Key").fill("test-only-key");
  await page.getByRole("button", { name: "保存设置" }).click();
  await expect(page.getByRole("alert")).toContainText("设置已保存，但 API Key 保存失败");
  await expect(page.getByRole("spinbutton", { name: /浮窗宽度/ })).toHaveValue("500");
  await expect(page.getByLabel("API Key")).toHaveValue("test-only-key");

  await page.goto("/?fail=save-runtime");
  await page.getByRole("spinbutton", { name: /浮窗宽度/ }).fill("500");
  await page.getByRole("button", { name: "保存设置" }).click();
  await expect(page.getByRole("alert")).toContainText("设置已保存，但自动划词监听切换没有在 2 秒内完成");
  await expect(page.getByRole("button", { name: "取消更改" })).toBeDisabled();
});

test("uses the authoritative post-write Keychain status and keeps unavailable visible", async ({ page }) => {
  await page.goto("/");
  await page.getByLabel("API Key").fill("test-only-key");
  await page.getByRole("button", { name: "保存设置" }).click();
  await expect(page.getByRole("region", { name: "运行状态" })).toContainText("已连接");
  await expect(page.getByLabel("API Key")).toHaveValue("");
  await expect(page.getByText("设置已保存并已应用。")).toBeVisible();

  await page.goto("/?runtime=keychain-unavailable");
  await expect(page.getByRole("region", { name: "运行状态" })).toContainText("暂不可读");
  await page.getByLabel("API Key").fill("test-only-key");
  await page.getByRole("button", { name: "保存设置" }).click();
  await expect(page.getByRole("region", { name: "运行状态" })).toContainText("暂不可读");
  await expect(page.getByRole("alert")).toContainText("若保存后仍显示“暂不可读”，需改用稳定签名构建");
  await expect(page.getByLabel("API Key")).toHaveValue("");
});

test("shows the exact permission CTA for each independent capability", async ({ page }) => {
  await page.goto("/?permission=accessibility-denied");
  await expect(page.getByRole("button", { name: "打开辅助功能权限" })).toBeVisible();
  await expect(page.getByRole("button", { name: "打开输入监听权限" })).toHaveCount(0);

  await page.goto("/?permission=input-denied");
  await expect(page.getByRole("button", { name: "打开输入监听权限" })).toBeVisible();
  await expect(page.getByRole("button", { name: "打开辅助功能权限" })).toHaveCount(0);

  await page.goto("/?permission=both-denied");
  await expect(page.getByRole("button", { name: "打开辅助功能权限" })).toBeVisible();
  await expect(page.getByRole("button", { name: "打开输入监听权限" })).toBeVisible();

  await page.goto("/?permission=runtime-degraded");
  await expect(page.getByRole("region", { name: "运行状态" })).toContainText(
    "权限已授权，但运行时监听正在恢复。"
  );
  await expect(page.getByRole("button", { name: "刷新诊断" })).toBeVisible();
});

test("refreshes after an armed permission return but ignores ordinary focus changes", async ({ page }) => {
  await page.goto("/?fail=refresh");
  await page.evaluate(() => {
    window.dispatchEvent(new Event("blur"));
    window.dispatchEvent(new Event("focus"));
  });
  await expect(page.getByRole("alert")).toHaveCount(0);

  await page.goto("/?permission=accessibility-denied&fail=refresh");
  await page.getByRole("button", { name: "打开辅助功能权限" }).click();
  await page.evaluate(() => {
    window.dispatchEvent(new Event("blur"));
    window.dispatchEvent(new Event("focus"));
  });
  await expect(page.getByRole("alert")).toContainText("固定测试：refresh 操作失败");
});

test("uses a non-overlapping action bar in a narrow window", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto("/");
  await expect(page.locator(".settings-actions")).toHaveCSS("position", "static");
  const horizontalOverflow = await page.evaluate(
    () => document.documentElement.scrollWidth > window.innerWidth + 1
  );
  expect(horizontalOverflow).toBe(false);
  const image = await page.screenshot({ fullPage: true });
  expect(image.byteLength).toBeGreaterThan(1_000);
});

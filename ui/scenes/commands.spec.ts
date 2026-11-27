import { expect, test, type Page } from "@playwright/test";

const sent = (page: Page, command: string) =>
  page.evaluate((name) => window.sent.filter((call) => call.command === name), command);

test.beforeEach(async ({ page }) => {
  await page.goto("/scene.html?fixture=apps&commands=1");
  await page.waitForLoadState("load");
  await page.locator(".row.selected").waitFor();
});

test("the commands sit under Commands on the empty window", async ({ page }) => {
  await expect(page.locator(".sec").last()).toHaveText("Commands");
  await expect(page.locator(".row").nth(-5)).toContainText("Fix grammarSelected textAI");
  await expect(page.locator(".row").nth(-4)).toContainText("Clipboard HistoryRaidenCommand");
  await expect(page.locator(".row").nth(-3)).toContainText(
    "Pull RequestsGitHub1 needs your review",
  );
  await expect(page.locator(".row").nth(-2)).toContainText("SettingsRaiden⌘ ,");
  await expect(page.locator(".row").last()).toContainText("Open Settings Fileraiden.tomlCommand");
});

test("typing fix and Enter runs Fix grammar with its query and row", async ({ page }) => {
  await page.keyboard.type("fix");
  await expect(page.locator(".row.selected .title")).toHaveText("Fix grammar");
  await expect(page.locator("#ctx")).toHaveText("AI");
  await page.keyboard.press("Enter");
  await expect
    .poll(() => sent(page, "fix_grammar"))
    .toEqual([{ command: "fix_grammar", args: { query: "fix", row: 0 } }]);
  expect(await sent(page, "open")).toEqual([]);
});

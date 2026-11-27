import { expect, test, type Page } from "@playwright/test";

const sent = (page: Page, command: string) =>
  page.evaluate((name) => window.sent.filter((call) => call.command === name), command);

test.describe("when searching", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/scene.html?fixture=apps&commands=1&settings=1");
    await page.waitForLoadState("load");
    await page.locator(".row.selected").waitFor();
  });

  test("settings pages stay out of the empty window", async ({ page }) => {
    await expect(page.locator(".row", { hasText: "Display" })).toHaveCount(0);
    await expect(page.locator(".sec").last()).toHaveText("Commands");
  });

  test("typing a page's name finds it with the pane it lives in", async ({ page }) => {
    await page.keyboard.type("display");
    await expect(page.locator(".row.selected .title")).toHaveText("Display");
    await expect(page.locator(".row.selected .sub")).toHaveText("Accessibility");
    await expect(page.locator("#ctx")).toHaveText("System Settings");
    await expect(page.locator(".row", { hasText: "Displays" }).locator(".sub")).toHaveText(
      "System Settings",
    );
  });

  test("Enter opens the page's link", async ({ page }) => {
    await page.keyboard.type("display");
    await page.keyboard.press("Enter");
    await expect
      .poll(() => sent(page, "open"))
      .toEqual([
        {
          command: "open",
          args: {
            path: "x-apple.systempreferences:com.apple.Accessibility-Settings.extension?display",
            query: "display",
            row: 0,
          },
        },
      ]);
  });
});

test("a page picked before shows under Suggestions and the headings still count right", async ({
  page,
}) => {
  await page.goto("/scene.html?fixture=apps&commands=1&settings=1&picked=setting");
  await page.waitForLoadState("load");
  await expect(page.locator(".sec, .row .title")).toHaveText([
    "Suggestions",
    "Safari",
    "Display",
    "Zed",
    "Messages",
    "Terminal",
    "Applications",
    "Calendar",
    "Finder",
    "Mail",
    "Music",
    "Notes",
    "System Settings",
    "Commands",
    "Fix grammar",
    "Clipboard History",
    "Pull Requests",
    "Settings",
  ]);
  await expect(page.locator(".row", { hasText: "Display" }).locator(".sub")).toHaveText(
    "Accessibility",
  );
});

import { expect, test, type Page } from "@playwright/test";
import { EXPANDED_VIEWPORT, SCENE_TIME } from "./scenes";

test.use({ viewport: EXPANDED_VIEWPORT });

const sent = (page: Page, command: string) =>
  page.evaluate((name) => window.sent.filter((call) => call.command === name), command);

const size = (page: Page) =>
  page.locator("#app").evaluate((app: HTMLElement) => [app.offsetWidth, app.offsetHeight]);

const COMPACT = [720, 452];
const EXPANDED = [900, 660];

const openPulls = async (page: Page, more = "") => {
  await page.clock.setFixedTime(new Date(SCENE_TIME));
  await page.goto(`/scene.html?fixture=pulls&commands=1${more}`);
  await page.waitForLoadState("networkidle");
  await page.locator(".scopes").waitFor();
};

const openSearch = async (page: Page, more = "") => {
  await page.goto(`/scene.html?fixture=apps&commands=1${more}`);
  await page.waitForLoadState("networkidle");
  await page.locator(".row.selected").waitFor();
};

const hideAndReopen = async (page: Page) => {
  await page.evaluate(() => window.dispatchEvent(new Event("blur")));
  await page.evaluate(() => window.dispatchEvent(new Event("focus")));
};

const lastAction = (page: Page) => page.locator("#actions .act").last();

const menuOffers = async (page: Page, title: string) => {
  await page.keyboard.press("Meta+k");
  await expect(lastAction(page).locator(".act-t")).toHaveText(title);
  await page.keyboard.press("Escape");
  await expect(page.locator("#actions")).toBeHidden();
};

test.describe("the expanded size", () => {
  test("⌘⇧E expands Pull Requests, and ⌘K offers to shrink it again", async ({ page }) => {
    await openPulls(page);
    await expect.poll(() => size(page)).toEqual(COMPACT);
    await page.keyboard.press("Meta+Shift+E");
    await expect.poll(() => size(page)).toEqual(EXPANDED);
    await expect
      .poll(() => sent(page, "expand_window"))
      .toEqual([{ command: "expand_window", args: { expanded: true } }]);
    await page.keyboard.press("Meta+k");
    await expect(lastAction(page).locator(".act-t")).toHaveText("Shrink Window");
    await expect(lastAction(page).locator("kbd")).toHaveText(["⌘", "⇧", "E"]);
    await page.keyboard.type("shrink");
    await expect(page.locator("#actions .act")).toHaveCount(1);
    await page.keyboard.press("Enter");
    await expect(page.locator("#actions")).toBeHidden();
    await expect.poll(() => size(page)).toEqual(COMPACT);
    await page.keyboard.press("Meta+k");
    await expect(lastAction(page).locator(".act-t")).toHaveText("Expand Window");
    await page.keyboard.press("Meta+Shift+E");
    await expect(page.locator("#actions")).toBeHidden();
    await expect.poll(() => size(page)).toEqual(EXPANDED);
  });

  test("expanded in Pull Requests, going back keeps search expanded", async ({ page }) => {
    await openPulls(page);
    await page.keyboard.press("Meta+Shift+E");
    await expect.poll(() => size(page)).toEqual(EXPANDED);
    await page.keyboard.press("Escape");
    await expect(page.locator("#chip")).toBeHidden();
    await page.locator(".row.selected").waitFor();
    await page.waitForTimeout(200);
    expect(await size(page)).toEqual(EXPANDED);
    await menuOffers(page, "Shrink Window");
    await page.evaluate(() => window.openClipboard?.(false));
    await expect(page.locator("#chip")).toHaveText("Clipboard History");
    expect(await size(page)).toEqual(EXPANDED);
    await page.keyboard.press("Escape");
    await page.keyboard.press("Meta+Comma");
    await expect(page.locator("#chip")).toHaveText("Settings");
    expect(await size(page)).toEqual(EXPANDED);
    expect(await sent(page, "expand_window")).toEqual([
      { command: "expand_window", args: { expanded: true } },
    ]);
  });

  test("hiding and reopening keeps it expanded, in search and in a view", async ({ page }) => {
    await openPulls(page);
    await page.keyboard.press("Meta+Shift+E");
    await expect.poll(() => size(page)).toEqual(EXPANDED);
    await hideAndReopen(page);
    await expect(page.locator("#chip")).toBeHidden();
    await page.locator(".row.selected").waitFor();
    await page.waitForTimeout(200);
    expect(await size(page)).toEqual(EXPANDED);
    await menuOffers(page, "Shrink Window");
    await page.keyboard.type("pull");
    await expect(page.locator(".row.selected .title")).toHaveText("Pull Requests");
    await page.keyboard.press("Enter");
    await expect(page.locator("#chip")).toHaveText("Pull Requests");
    expect(await size(page)).toEqual(EXPANDED);
    await menuOffers(page, "Shrink Window");
    expect(await sent(page, "expand_window")).toHaveLength(1);
  });

  test("shrinking makes everything compact again, until it is expanded again", async ({ page }) => {
    await openSearch(page, "&size=expanded");
    await expect.poll(() => size(page)).toEqual(EXPANDED);
    await page.keyboard.press("Meta+Shift+E");
    await expect.poll(() => size(page)).toEqual(COMPACT);
    await menuOffers(page, "Expand Window");
    await page.keyboard.type("pull");
    await expect(page.locator(".row.selected .title")).toHaveText("Pull Requests");
    await page.keyboard.press("Enter");
    await expect(page.locator("#chip")).toHaveText("Pull Requests");
    await page.locator(".scopes").waitFor();
    expect(await size(page)).toEqual(COMPACT);
    await menuOffers(page, "Expand Window");
    await page.keyboard.press("Escape");
    await page.evaluate(() => window.openClipboard?.(false));
    await expect(page.locator("#chip")).toHaveText("Clipboard History");
    expect(await size(page)).toEqual(COMPACT);
    await hideAndReopen(page);
    await page.locator(".row.selected").waitFor();
    expect(await size(page)).toEqual(COMPACT);
    expect(await sent(page, "expand_window")).toEqual([
      { command: "expand_window", args: { expanded: false } },
    ]);
  });

  test("⌘K in search offers the size after the row's own actions", async ({ page }) => {
    await openSearch(page);
    await page.keyboard.press("Meta+k");
    await expect(page.locator("#actions .act-t")).toHaveText([
      "Open",
      "Show in Finder",
      "Copy path",
      "Expand Window",
    ]);
    await page.keyboard.type("expand");
    await page.keyboard.press("Enter");
    await expect(page.locator("#actions")).toBeHidden();
    await expect.poll(() => size(page)).toEqual(EXPANDED);
  });

  test("the search list fills the taller window", async ({ page }) => {
    const shownRows = () =>
      page.evaluate(() => {
        const list = document.querySelector<HTMLElement>(".list");
        const bottom = list?.getBoundingClientRect().bottom ?? 0;
        return [...document.querySelectorAll<HTMLElement>(".row")].filter(
          (row) => row.getBoundingClientRect().bottom <= bottom,
        ).length;
      });
    const listHeight = () =>
      page.locator(".list").evaluate((list: HTMLElement) => list.offsetHeight);
    await openSearch(page);
    const compactRows = await shownRows();
    const compactHeight = await listHeight();
    await page.keyboard.press("Meta+Shift+E");
    await expect.poll(() => size(page)).toEqual(EXPANDED);
    await expect.poll(listHeight).toBe(compactHeight + 208);
    await expect.poll(shownRows).toBeGreaterThan(compactRows + 3);
  });

  test("Ask AI and Settings share the one size", async ({ page }) => {
    await page.goto("/scene.html?fixture=answer");
    await page.waitForLoadState("networkidle");
    await page.keyboard.type("how do I land a change?");
    await page.keyboard.press("Tab");
    await expect(page.locator("#chip")).toHaveText("Ask AI");
    await page.keyboard.press("Meta+k");
    await expect(lastAction(page).locator(".act-t")).toHaveText("Expand Window");
    await page.keyboard.press("Meta+Shift+E");
    await expect(page.locator("#actions")).toBeHidden();
    await expect.poll(() => size(page)).toEqual(EXPANDED);
    await page.keyboard.press("Meta+Comma");
    await expect(page.locator("#chip")).toHaveText("Settings");
    expect(await size(page)).toEqual(EXPANDED);
    await page.keyboard.press("Escape");
    await expect(page.locator("#chip")).toHaveText("Ask AI");
    expect(await size(page)).toEqual(EXPANDED);
  });

  test("a past conversation reopens at the chosen size", async ({ page }) => {
    await page.goto("/scene.html?fixture=recent-answers&size=expanded");
    await page.waitForLoadState("networkidle");
    await page.locator(".row", { hasText: "How do I land a change?" }).click();
    await expect(page.locator("#chip")).toHaveText("Ask AI");
    expect(await size(page)).toEqual(EXPANDED);
  });

  test("an empty Clipboard history still offers the size in ⌘K", async ({ page }) => {
    await openSearch(page);
    await page.evaluate(() => window.openClipboard?.(false));
    await expect(page.locator("#chip")).toHaveText("Clipboard History");
    await page.keyboard.press("Meta+k");
    await expect(page.locator("#actions .act-t")).toHaveText(["Expand Window"]);
  });

  test("a saved expanded choice opens search expanded after a restart", async ({ page }) => {
    await openSearch(page, "&size=expanded");
    expect(await size(page)).toEqual(EXPANDED);
    await menuOffers(page, "Shrink Window");
    expect(await sent(page, "window_size")).toContainEqual({ command: "window_size" });
    expect(await sent(page, "expand_window")).toEqual([]);
  });
});

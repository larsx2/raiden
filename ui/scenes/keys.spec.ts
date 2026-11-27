import { expect, test, type Page } from "@playwright/test";

const sent = (page: Page, command: string) =>
  page.evaluate((name) => window.sent.filter((call) => call.command === name), command);

test.beforeEach(async ({ page }) => {
  await page.goto("/scene.html?fixture=apps");
  await page.waitForLoadState("load");
  await page.locator(".row.selected").waitFor();
});

test("⌘K, cop and Enter on the Safari row copies Safari's path", async ({ page }) => {
  await page.keyboard.type("saf");
  await expect(page.locator(".row.selected")).toHaveText("Safari");
  await page.keyboard.press("Meta+k");
  await expect(page.locator("#actions .head")).toHaveText("Safari");
  await page.keyboard.type("cop");
  await expect(page.locator(".act")).toHaveText(["Copy path⌘⇧C"]);
  await page.keyboard.press("Enter");
  await expect
    .poll(() => sent(page, "copy_path"))
    .toEqual([{ command: "copy_path", args: { path: "/Applications/Safari.app" } }]);
  await expect(page.locator("#actions")).toBeHidden();
  await expect(page.locator("#q")).toBeFocused();
});

test("Enter opens the selected row with its query and row", async ({ page }) => {
  await page.keyboard.type("saf");
  await page.keyboard.press("Enter");
  await expect
    .poll(() => sent(page, "open"))
    .toEqual([
      { command: "open", args: { path: "/Applications/Safari.app", query: "saf", row: 0 } },
    ]);
});

test("opening an app clears the search once the panel hides", async ({ page }) => {
  await page.keyboard.type("saf");
  await page.keyboard.press("Enter");
  await expect.poll(() => sent(page, "open")).toHaveLength(1);
  await expect(page.locator("#q")).toHaveValue("saf");
  await page.evaluate(() => window.dispatchEvent(new Event("blur")));
  await expect(page.locator("#q")).toHaveValue("");
});

test("the footer shows Open and Actions, and its buttons work", async ({ page }) => {
  await page.keyboard.type("saf");
  await expect(page.locator("#primary")).toContainText("Open");
  await page.locator("#openActions").click();
  await expect(page.locator("#actions")).toBeVisible();
  await page.locator("#openActions").click();
  await expect(page.locator("#actions")).toBeHidden();
  await page.locator("#primary").click();
  await expect.poll(() => sent(page, "open")).toHaveLength(1);
});

test("⌘↵ on a row shows it in Finder without opening the menu", async ({ page }) => {
  await page.keyboard.type("saf");
  await page.keyboard.press("Meta+Enter");
  await expect
    .poll(() => sent(page, "show_in_finder"))
    .toEqual([{ command: "show_in_finder", args: { path: "/Applications/Safari.app" } }]);
  await expect(page.locator("#actions")).toBeHidden();
});

test("the arrows move through the actions and stop at the ends", async ({ page }) => {
  await page.keyboard.press("Meta+k");
  await page.keyboard.press("ArrowUp");
  await expect(page.locator(".act.sel")).toHaveText("Open↵");
  for (const _ of [1, 2, 3, 4, 5]) {
    await page.keyboard.press("ArrowDown");
  }
  await expect(page.locator(".act.sel")).toHaveText("Expand Window⌘⇧E");
});

test("⌘↵ and ⌘⇧C in the menu run their own actions, not Open", async ({ page }) => {
  await page.keyboard.type("saf");
  await page.keyboard.press("Meta+k");
  await page.keyboard.press("Meta+Shift+c");
  await expect
    .poll(() => sent(page, "copy_path"))
    .toEqual([{ command: "copy_path", args: { path: "/Applications/Safari.app" } }]);
  await expect(page.locator("#actions")).toBeHidden();
  await page.keyboard.press("Meta+k");
  await page.keyboard.press("Meta+Enter");
  await expect
    .poll(() => sent(page, "show_in_finder"))
    .toEqual([{ command: "show_in_finder", args: { path: "/Applications/Safari.app" } }]);
  expect(await sent(page, "open")).toEqual([]);
});

test("Esc during the flash cancels a pending action", async ({ page }) => {
  await page.keyboard.press("Meta+k");
  await page.keyboard.type("cop");
  const pending = await page.evaluate(() => {
    const press = (key: string) =>
      document.activeElement?.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true }));
    press("Enter");
    const flashing = document.querySelectorAll(".act.press").length;
    press("Escape");
    return { flashing, left: document.querySelectorAll(".act.press").length };
  });
  expect(pending).toEqual({ flashing: 1, left: 0 });
  await page.waitForTimeout(300);
  expect(await sent(page, "copy_path")).toEqual([]);
  await expect(page.locator("#actions")).toBeVisible();
});

test("Enter opens at once, flashing the row only as feedback", async ({ page }) => {
  await page.keyboard.type("saf");
  const atPress = await page.evaluate(() => {
    document.activeElement?.dispatchEvent(
      new KeyboardEvent("keydown", { key: "Enter", bubbles: true }),
    );
    return {
      opened: window.sent.filter((call) => call.command === "open").length,
      flashing: document.querySelectorAll(".row.selected.press").length,
    };
  });
  expect(atPress).toEqual({ opened: 1, flashing: 1 });
});

test("clicking a row while the menu is open puts focus back in the search", async ({ page }) => {
  await page.keyboard.press("Meta+k");
  await expect(page.locator("#aq")).toBeFocused();
  await page.locator(".row").nth(2).click();
  await expect(page.locator("#actions")).toBeHidden();
  await expect(page.locator("#q")).toBeFocused();
});

test("Esc clears the command's search, then leaves the command, then hides", async ({ page }) => {
  await page.keyboard.press("Meta+k");
  await page.keyboard.type("x");
  await expect(page.locator("#aq")).toHaveValue("x");
  await page.keyboard.press("Escape");
  await expect(page.locator("#aq")).toHaveValue("");
  await expect(page.locator("#actions")).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(page.locator("#actions")).toBeHidden();
  await expect(page.locator("#q")).toBeFocused();
  expect(await sent(page, "hide")).toEqual([]);
  await page.keyboard.press("Escape");
  await expect.poll(() => sent(page, "hide")).toEqual([{ command: "hide" }]);
});

test("Esc clears the search before hiding", async ({ page }) => {
  await page.keyboard.type("saf");
  await page.keyboard.press("Escape");
  await expect(page.locator("#q")).toHaveValue("");
  await expect(page.locator(".row")).toHaveCount(10);
  expect(await sent(page, "hide")).toEqual([]);
  await page.keyboard.press("Escape");
  await expect.poll(() => sent(page, "hide")).toEqual([{ command: "hide" }]);
});

test("Backspace on the command's empty search leaves it", async ({ page }) => {
  await page.keyboard.type("saf");
  await page.keyboard.press("Meta+k");
  await page.keyboard.type("x");
  await page.keyboard.press("Backspace");
  await expect(page.locator("#actions")).toBeVisible();
  await page.keyboard.press("Backspace");
  await expect(page.locator("#actions")).toBeHidden();
  await expect(page.locator("#q")).toHaveValue("saf");
});

test("the back button leaves the command", async ({ page }) => {
  await expect(page.locator("#back")).toBeHidden();
  await page.keyboard.press("Meta+k");
  await page.locator("#back").click();
  await expect(page.locator("#actions")).toBeHidden();
  await expect(page.locator("#back")).toBeHidden();
  await expect(page.locator("#q")).toBeFocused();
  expect(await sent(page, "hide")).toEqual([]);
});

test("holding Backspace in the menu's empty search closes it once and keeps the search", async ({
  page,
}) => {
  await page.keyboard.type("saf");
  await page.keyboard.press("Meta+k");
  const after = await page.evaluate(() => {
    const press = (repeat: boolean) =>
      document.activeElement?.dispatchEvent(
        new KeyboardEvent("keydown", { key: "Backspace", repeat, bubbles: true, cancelable: true }),
      );
    press(false);
    const closed = document.querySelector("#actions") === null;
    const repeatsAllowed = [1, 2, 3].map(() => press(true));
    return { closed, repeatsAllowed };
  });
  expect(after).toEqual({ closed: true, repeatsAllowed: [false, false, false] });
  await expect(page.locator("#q")).toHaveValue("saf");
});

test("holding Esc takes one step back, not three", async ({ page }) => {
  await page.keyboard.press("Meta+k");
  await page.keyboard.type("x");
  await page.evaluate(() => {
    const press = (repeat: boolean) =>
      document.activeElement?.dispatchEvent(
        new KeyboardEvent("keydown", { key: "Escape", repeat, bubbles: true }),
      );
    press(false);
    press(true);
    press(true);
  });
  await expect(page.locator("#aq")).toHaveValue("");
  await expect(page.locator("#actions")).toBeVisible();
  expect(await sent(page, "hide")).toEqual([]);
});

const selectedRow = (page: Page) => page.locator(".row.selected");
const listTop = (page: Page) => page.locator(".list").evaluate((list) => list.scrollTop);

const hoverRow = async (page: Page, name: string) => {
  const box = await page.locator(".row", { hasText: name }).boundingBox();
  if (!box) {
    throw new Error(`no row named ${name}`);
  }
  const at = { x: box.x + 60, y: box.y + box.height / 2 };
  await page.mouse.move(at.x, at.y);
  await page.mouse.move(at.x + 4, at.y);
  return { x: at.x + 4, y: at.y };
};

test("with nothing typed, Suggestions lists the most-used apps over the rest", async ({ page }) => {
  await expect(page.locator(".sec")).toHaveText(["Suggestions", "Applications"]);
  await expect(page.locator(".row").first()).toHaveText("Safari");
  await page.keyboard.type("s");
  await expect(page.locator(".sec")).toHaveText(["Results", "Use “s” with"]);
});

test("the arrows stop at the ends of the list and step over headings", async ({ page }) => {
  await page.keyboard.press("ArrowUp");
  await expect(selectedRow(page)).toHaveText("Safari");
  await expect.poll(() => listTop(page)).toBe(0);
  for (const _ of [1, 2, 3, 4, 5]) {
    await page.keyboard.press("ArrowDown");
  }
  await expect(selectedRow(page)).toHaveText("Calendar");
});

test("Page Down and Page Up stop at the ends and show the first heading", async ({ page }) => {
  await page.keyboard.press("PageDown");
  await expect(selectedRow(page)).toHaveText("Notes");
  await page.keyboard.press("PageDown");
  await expect(selectedRow(page)).toHaveText("System Settings");
  await expect(selectedRow(page)).toBeInViewport({ ratio: 1 });
  await page.keyboard.press("PageUp");
  await page.keyboard.press("PageUp");
  await expect(selectedRow(page)).toHaveText("Safari");
  await expect.poll(() => listTop(page)).toBe(0);
});

test("hovering a row selects it, and Enter and ⌘K act on it", async ({ page }) => {
  await hoverRow(page, "Zed");
  await expect(selectedRow(page)).toHaveText("Zed");
  await page.keyboard.press("Meta+k");
  await expect(page.locator("#actions .head")).toHaveText("Zed");
  await page.keyboard.press("Escape");
  await page.keyboard.press("Enter");
  await expect
    .poll(() => sent(page, "open"))
    .toEqual([{ command: "open", args: { path: "/Applications/Zed.app", query: "", row: 1 } }]);
});

test("hovering a heading selects nothing", async ({ page }) => {
  const box = await page.locator(".sec", { hasText: "Applications" }).boundingBox();
  await page.mouse.move((box?.x ?? 0) + 60, (box?.y ?? 0) + 10);
  await page.mouse.move((box?.x ?? 0) + 64, (box?.y ?? 0) + 10);
  await expect(selectedRow(page)).toHaveText("Safari");
});

test("the keyboard keeps the selection while the list scrolls under a still pointer", async ({
  page,
}) => {
  const pointer = await hoverRow(page, "Zed");
  await expect(selectedRow(page)).toHaveText("Zed");
  for (const _ of [1, 2, 3, 4, 5, 6, 7, 8]) {
    await page.keyboard.press("ArrowDown");
  }
  await expect(selectedRow(page)).toHaveText("System Settings");
  await expect.poll(() => listTop(page)).toBeGreaterThan(0);
  await page.mouse.move(pointer.x, pointer.y);
  await expect(selectedRow(page)).toHaveText("System Settings");
});

test("the letters a search matched are bold", async ({ page }) => {
  await page.keyboard.type("sys set");
  await expect(page.locator(".row.selected .title b")).toHaveText(["Sys", "Set"]);
  await page.keyboard.press("Escape");
  await expect(page.locator(".title b")).toHaveCount(0);
});

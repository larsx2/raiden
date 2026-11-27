import { expect, test, type Page } from "@playwright/test";
import { SCENE_TIME } from "./scenes";

const sent = (page: Page, command: string) =>
  page.evaluate((name) => window.sent.filter((call) => call.command === name), command);

const openHistory = async (page: Page) => {
  await page.clock.setFixedTime(new Date(SCENE_TIME));
  await page.goto("/scene.html?fixture=clipboard");
  await page.waitForLoadState("load");
  await expect(page.locator(".clip").first()).toBeVisible();
};

test.describe("getting there", () => {
  test("typing clip finds Clipboard History under Raiden, and Enter opens it", async ({ page }) => {
    await page.goto("/scene.html?fixture=apps&commands=1");
    await page.waitForLoadState("load");
    await page.locator(".row.selected").waitFor();
    await page.keyboard.type("clip");
    await expect(page.locator(".row.selected .title")).toHaveText("Clipboard History");
    await expect(page.locator(".row.selected .sub")).toHaveText("Raiden");
    await expect(page.locator("#primary")).toContainText("Open Clipboard History");
    await page.keyboard.press("Enter");
    await expect(page.locator("#chip")).toHaveText("Clipboard History");
    await expect(page.locator("#q")).toHaveAttribute("placeholder", "Search clipboard");
    await expect(page.locator("#q")).toHaveValue("");
    await expect
      .poll(() => sent(page, "clipboard_items"))
      .toEqual([{ command: "clipboard_items", args: { query: "clip", row: 0 } }]);
    await page.keyboard.press("Escape");
    await expect(page.locator("#chip")).toBeHidden();
    await expect(page.locator("#q")).toHaveValue("clip");
  });

  test("the hotkey opens it, and pressing it again while it shows hides the panel", async ({
    page,
  }) => {
    await page.goto("/scene.html?fixture=apps&commands=1");
    await page.waitForLoadState("load");
    await page.evaluate(() => window.openClipboard?.(false));
    await expect(page.locator("#chip")).toHaveText("Clipboard History");
    expect(await sent(page, "hide")).toEqual([]);
    await page.evaluate(() => window.openClipboard?.(true));
    await expect.poll(() => sent(page, "hide")).toEqual([{ command: "hide" }]);
  });

  test("the hotkey over Settings switches to Clipboard history", async ({ page }) => {
    await page.goto("/scene.html?fixture=apps&commands=1");
    await page.waitForLoadState("load");
    await page.keyboard.press("Meta+Comma");
    await expect(page.locator("#chip")).toHaveText("Settings");
    await page.evaluate(() => window.openClipboard?.(true));
    await expect(page.locator("#chip")).toHaveText("Clipboard History");
    await expect(page.locator(".set[data-row]")).toHaveCount(0);
    expect(await sent(page, "hide")).toEqual([]);
  });
});

test.describe("keys typed right after the hotkey", () => {
  const typed = (keys: string) => [...keys].map((key) => ({ key, text: key }));

  const historyHolds = async (page: Page, query: string) => {
    await expect(page.locator("#chip")).toHaveText("Clipboard History");
    await expect(page.locator("#q")).toHaveValue(query);
    await expect(page.locator(".clip .name")).toHaveText(["just land"]);
  };

  const backOnMainSearch = async (page: Page) => {
    await page.keyboard.press("Escape");
    await page.keyboard.press("Escape");
    await expect(page.locator("#chip")).toBeHidden();
    await expect(page.locator("#q")).toHaveValue("");
  };

  test("land in its search, in order, when the hotkey opens the hidden panel", async ({ page }) => {
    await page.goto("/scene.html?fixture=apps&clips=1");
    await page.waitForLoadState("load");
    await page.evaluate(
      (batches) => {
        window.typedAhead = batches;
        window.openClipboard?.(false);
        window.dispatchEvent(new Event("focus"));
      },
      [typed("ju"), typed("st")],
    );
    await historyHolds(page, "just");
    await backOnMainSearch(page);
  });

  test("land in its search when the hotkey opens it over the shown panel", async ({ page }) => {
    await page.goto("/scene.html?fixture=apps&clips=1");
    await page.waitForLoadState("load");
    await page.evaluate(
      (batches) => {
        window.typedAhead = batches;
        window.openClipboard?.(true);
      },
      [typed("jus"), typed("t")],
    );
    await historyHolds(page, "just");
    await backOnMainSearch(page);
  });
});

test.describe("the list", () => {
  test("shows the newest first, by day, with what each is and how old", async ({ page }) => {
    await openHistory(page);
    await expect(page.locator(".clips .sec, .clip .name")).toHaveText([
      "Today",
      "git worktree add .cache/launcher -b launcher origin/main",
      "https://gpui.rs/",
      "Image, 1280 × 720",
      "~/Code/lantern/AGENTS.md",
      "Yesterday",
      "just land",
      "NSWindowCollectionBehavior.fullScreenAuxiliary",
    ]);
    await expect(page.locator(".clip .hint").nth(1)).toHaveText("Link from Safari, 9 min ago");
    await expect(page.locator(".clip").nth(2).locator("img.thumb")).toBeVisible();
    await expect(page.locator("#ctx")).toHaveText("Kept for 7 days, never passwords");
    await expect(page.locator("#primary")).toContainText("Paste into Zed");
  });

  test("typing filters it, and a search with no match says how to clear it", async ({ page }) => {
    await openHistory(page);
    await page.keyboard.type("terminal");
    await expect(page.locator(".clip .name")).toHaveText([
      "git worktree add .cache/launcher -b launcher origin/main",
      "just land",
    ]);
    await page.keyboard.press("Meta+a");
    await page.keyboard.type("zzzz");
    await expect(page.locator(".clips .none")).toHaveText(
      "No clipboard items match “zzzz”Press Esc to clear the search.",
    );
    await page.keyboard.press("Escape");
    await expect(page.locator("#q")).toHaveValue("");
    await expect(page.locator(".clip")).toHaveCount(6);
    await expect(page.locator("#chip")).toHaveText("Clipboard History");
  });
});

test("the arrows stop at the first and last item", async ({ page }) => {
  await openHistory(page);
  const first = await page.locator(".clip.selected .name").textContent();
  await page.keyboard.press("ArrowUp");
  await expect(page.locator(".clip.selected .name")).toHaveText(first ?? "");
  await expect(page.locator(".clip").first()).toHaveClass(/selected/);
  for (const _ of [1, 2, 3, 4, 5, 6, 7, 8]) {
    await page.keyboard.press("ArrowDown");
  }
  await expect(page.locator(".clip").last()).toHaveClass(/selected/);
});

test.describe("acting on an item", () => {
  test("Enter pastes the selected item into the app you came from", async ({ page }) => {
    await openHistory(page);
    await page.keyboard.press("ArrowDown");
    await expect(page.locator(".clip.selected .name")).toHaveText("https://gpui.rs/");
    await page.keyboard.press("Enter");
    await expect
      .poll(() => sent(page, "clipboard_paste"))
      .toEqual([{ command: "clipboard_paste", args: { id: "clip-1" } }]);
  });

  test("⌘C copies it and says so, and ⌘⌫ deletes it", async ({ page }) => {
    await openHistory(page);
    await page.keyboard.press("Meta+c");
    await expect(page.locator("#ctx")).toHaveText("Copied to the clipboard");
    await page.keyboard.press("Meta+Backspace");
    await expect(page.locator(".clip")).toHaveCount(5);
    await expect(page.locator(".clip.selected .name")).toHaveText("https://gpui.rs/");
    await expect(page.locator("#ctx")).toHaveText("Deleted from Clipboard history");
    expect(await sent(page, "clipboard_copy")).toEqual([
      { command: "clipboard_copy", args: { id: "clip-0" } },
    ]);
    expect(await sent(page, "clipboard_delete")).toEqual([
      { command: "clipboard_delete", args: { id: "clip-0" } },
    ]);
  });

  test("while a search is typed, ⌘⌫ edits it and ⌘C on selected text copies the text", async ({
    page,
  }) => {
    await openHistory(page);
    await page.keyboard.type("just");
    await page.keyboard.press("Meta+Backspace");
    await expect(page.locator("#q")).toHaveValue("");
    await expect(page.locator(".clip")).toHaveCount(6);
    await page.keyboard.type("just");
    await page.keyboard.press("Meta+a");
    await page.keyboard.press("Meta+c");
    expect(await sent(page, "clipboard_copy")).toEqual([]);
    expect(await sent(page, "clipboard_delete")).toEqual([]);
    await page.keyboard.press("Meta+k");
    await page.keyboard.type("del");
    await page.keyboard.press("Meta+Backspace");
    await expect(page.locator("#aq")).toHaveValue("");
    expect(await sent(page, "clipboard_delete")).toEqual([]);
  });

  test("a second Enter while pasting pastes once", async ({ page }) => {
    await page.clock.setFixedTime(new Date(SCENE_TIME));
    await page.goto("/scene.html?fixture=clipboard&pastefails=busy");
    await page.waitForLoadState("load");
    await expect(page.locator(".clip").first()).toBeVisible();
    await page.keyboard.press("Enter");
    await page.keyboard.press("Enter");
    await expect(page.locator("#ctx")).toHaveText("busy");
    expect(await sent(page, "clipboard_paste")).toHaveLength(1);
  });

  test("Page Down stops at the last item and Page Up at the first", async ({ page }) => {
    await openHistory(page);
    await page.keyboard.press("PageDown");
    await page.keyboard.press("PageDown");
    await expect(page.locator(".clip.selected .name")).toHaveText(
      "NSWindowCollectionBehavior.fullScreenAuxiliary",
    );
    await page.keyboard.press("PageUp");
    await page.keyboard.press("PageUp");
    await expect(page.locator(".clip.selected .name")).toHaveText(
      "git worktree add .cache/launcher -b launcher origin/main",
    );
  });

  test("⌘C moves the item to the top of the list", async ({ page }) => {
    await openHistory(page);
    await page.keyboard.press("ArrowDown");
    await page.keyboard.press("ArrowDown");
    await page.keyboard.press("ArrowDown");
    await page.keyboard.press("Meta+c");
    await expect(page.locator(".clip .name").first()).toHaveText("~/Code/lantern/AGENTS.md");
    await expect(page.locator(".clip.selected .hint")).toHaveText("File from Finder, just now");
    await expect(page.locator(".clip")).toHaveCount(6);
  });

  test("⌘K lists paste, copy and delete with their keys", async ({ page }) => {
    await openHistory(page);
    await page.keyboard.press("Meta+k");
    await expect(page.locator("#actions .head")).toHaveText(
      "git worktree add .cache/launcher -b launcher origin/main",
    );
    await expect(page.locator(".act")).toHaveText([
      "Paste into Zed↵",
      "Copy⌘C",
      "Delete⌘⌫",
      "Expand Window⌘⇧E",
    ]);
    await page.keyboard.press("Meta+Backspace");
    await expect(page.locator("#actions")).toBeHidden();
    await expect(page.locator(".clip")).toHaveCount(5);
  });

  test("Backspace on an empty search goes back, like Esc", async ({ page }) => {
    await openHistory(page);
    await page.keyboard.press("Backspace");
    await expect(page.locator("#chip")).toBeHidden();
    await expect(page.locator("#q")).toHaveAttribute("placeholder", "Search apps and commands");
  });
});

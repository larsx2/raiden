import { expect, test, type Page } from "@playwright/test";

const open = async (page: Page, fixture: string, extra = "") => {
  await page.goto(`/scene.html?fixture=${fixture}${extra}`);
  await page.waitForLoadState("load");
};

test("the main search says nothing matches, offers Tab for Ask AI, and Esc clears it", async ({
  page,
}) => {
  await open(page, "apps");
  await page.locator(".row.selected").waitFor();
  await page.keyboard.type("zzzz");
  await expect(page.locator(".list .none")).toHaveText(
    "Nothing matches “zzzz”Press Tab to ask AI about it, or Esc to clear the search.",
  );
  await expect(page.locator(".row.selected .title")).toHaveText("Ask AI");
  await page.keyboard.press("Escape");
  await expect(page.locator("#q")).toHaveValue("");
  await expect(page.locator(".list .none")).toHaveCount(0);
});

test("a short search with no match says so at once", async ({ page }) => {
  await open(page, "apps");
  await page.locator(".row.selected").waitFor();
  await page.keyboard.type("qq");
  await expect(page.locator(".list .none")).toContainText("Nothing matches “qq”");
});

test("with Spotlight's index off it says files are not searched", async ({ page }) => {
  await open(page, "apps", "&indexoff=1");
  await page.locator(".row.selected").waitFor();
  await page.keyboard.type("zzzz");
  await expect(page.locator(".list .none span").last()).toHaveText(
    "Files are not searched while Spotlight's index is off.",
  );
});

test("Settings says no settings match and how to clear the search", async ({ page }) => {
  await open(page, "settings");
  await page.locator(".set").first().waitFor();
  await page.keyboard.type("zzzz");
  await expect(page.locator(".prefs .none")).toHaveText(
    "No settings match “zzzz”Press Esc to clear the search.",
  );
});

test("Conversation history says no conversations match", async ({ page }) => {
  await open(page, "answer", "&moment=done&history=1");
  await expect(page.locator("#ai")).toHaveAttribute("data-phase", "done");
  await page.keyboard.press("Meta+y");
  await page.keyboard.type("zzzz");
  await expect(page.locator("#actions .none")).toHaveText(
    "No conversations match “zzzz”Press Esc to clear the search.",
  );
  await page.keyboard.press("Escape");
  await expect(page.locator("#aq")).toHaveValue("");
  await expect(page.locator("#actions .none")).toHaveCount(0);
});

test("the actions menu says no actions match", async ({ page }) => {
  await open(page, "apps");
  await page.locator(".row.selected").waitFor();
  await page.keyboard.press("Meta+k");
  await page.keyboard.type("zzzz");
  await expect(page.locator("#actions .none")).toHaveText(
    "No actions match “zzzz”Press Esc to clear the search.",
  );
});

test("the message stays up while the next letter's file search runs", async ({ page }) => {
  await open(page, "apps", "&fileswait=1&filesnowait=zzz");
  await page.locator(".row.selected").waitFor();
  await page.keyboard.type("zzz");
  await expect(page.locator(".list .none")).toContainText("Nothing matches “zzz”");
  await page.keyboard.type("z");
  await expect(page.locator(".list .none")).toContainText("Nothing matches “zzzz”");
});

test("no match is not claimed before files were searched", async ({ page }) => {
  await open(page, "apps", "&fileswait=1");
  await page.locator(".row.selected").waitFor();
  await page.keyboard.type("zz");
  await expect(page.locator(".list .none")).toContainText("Nothing matches “zz”");
  await page.keyboard.type("z");
  await expect(page.locator(".list .none")).toHaveCount(0);
  await page.evaluate(() => window.finishFiles?.());
  await expect(page.locator(".list .none")).toContainText("Nothing matches “zzz”");
});

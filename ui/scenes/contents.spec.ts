import { expect, test, type Page } from "@playwright/test";

const sent = (page: Page, command: string) =>
  page.evaluate((name) => window.sent.filter((call) => call.command === name), command);

const open = async (page: Page, extra = "") => {
  await page.goto(`/scene.html?fixture=contents${extra}`);
  await page.waitForLoadState("networkidle");
  await page.locator(".row.selected").waitFor();
};

const NOTES = "/Users/me/Documents/notes.md";

test("a quoted search finds the line inside the file, and Enter opens it", async ({ page }) => {
  await open(page);
  await page.keyboard.type('"quarterly plan');
  await expect(page.locator(".sec")).toHaveText(["Text in files", 'Use “"quarterly plan” with']);
  await expect(page.locator(".row .title")).toHaveText(["notes.md", "Ask AI"]);
  await expect(page.locator(".row.selected .sub")).toHaveText(
    "~/Documents:3The quarterly plan is due Friday.",
  );
  await expect(page.locator(".row.selected .sub .line b")).toHaveText(["quarterly", "plan"]);
  await expect(page.locator(".row.selected img.icon")).toHaveAttribute(
    "src",
    "/scenes/icons/file-md.png",
  );
  await expect(page.locator("#ctx")).toHaveText("Text in files");
  expect(await sent(page, "files")).toEqual([]);
  await expect
    .poll(() => sent(page, "contents"))
    .toContainEqual({
      command: "contents",
      args: { query: "quarterly plan", asked: expect.any(Number) },
    });
  await page.keyboard.press("Enter");
  await expect
    .poll(() => sent(page, "open_file"))
    .toEqual([{ command: "open_file", args: { path: NOTES } }]);
});

test("⌘Enter shows the file in Finder, and ⌘K lists the file actions", async ({ page }) => {
  await open(page);
  await page.keyboard.type('"fn main"');
  await expect(page.locator(".row.selected .title")).toHaveText("main.rs");
  await expect(page.locator(".row.selected .sub")).toHaveText("~/Code/lantern/src:3fn main() {");
  await page.keyboard.press("Meta+k");
  await expect(page.locator("#actions .head")).toHaveText("main.rs");
  await expect(page.locator(".act .act-t")).toHaveText([
    "Open",
    "Show in Finder",
    "Copy path",
    "Open in Terminal",
    "Expand Window",
  ]);
  await page.keyboard.press("Escape");
  await page.keyboard.press("Meta+Enter");
  await expect
    .poll(() => sent(page, "show_in_finder"))
    .toEqual([{ command: "show_in_finder", args: { path: "/Users/me/Code/lantern/src/main.rs" } }]);
});

test("a plain search looks only at names, and apps come back", async ({ page }) => {
  await open(page);
  await page.keyboard.type("notes");
  await expect(page.locator(".row .title").first()).toHaveText("Notes");
  await expect(page.locator(".sec")).not.toContainText(["Text in files"]);
  await expect.poll(() => sent(page, "files")).not.toEqual([]);
  expect(await sent(page, "contents")).toEqual([]);
});

test("two letters after the quote search nothing yet", async ({ page }) => {
  await open(page);
  await page.keyboard.type('"pl');
  await page.waitForTimeout(100);
  expect(await sent(page, "contents")).toEqual([]);
  expect(await sent(page, "files")).toEqual([]);
});

test("an answer for an older quoted search is dropped", async ({ page }) => {
  await open(page, "&contentswait=1&contentsnowait=TODO");
  await page.keyboard.type('"quarterly');
  await page.keyboard.press("Meta+a");
  await page.keyboard.type('"TODO');
  await expect(page.locator(".row .title")).toHaveText(["search.ts", "Ask AI"]);
  await page.evaluate(() => window.finishContents?.("quarterly"));
  await page.waitForTimeout(100);
  await expect(page.locator(".row .title")).toHaveText(["search.ts", "Ask AI"]);
  await expect(page.locator(".row.selected .sub .line")).toHaveText(
    "// TODO: rank recent files first",
  );
});

test("while the text index is built, a note says results are coming", async ({ page }) => {
  await open(page, "&contentstate=building");
  await page.keyboard.type('"quarterly plan');
  await expect(page.locator(".row .title")).toHaveText(["notes.md", "Ask AI"]);
  await expect(page.locator(".list .note")).toHaveText(
    "Raiden is reading your files to search their text. Results will come in as it finishes.",
  );
  await page.keyboard.press("Meta+a");
  await page.keyboard.type("notes");
  await expect(page.locator(".list .note")).toHaveCount(0);
});

test("on battery, the note says the text is read while plugged in", async ({ page }) => {
  await open(page, "&contentstate=waiting");
  await page.keyboard.type('"quarterly plan');
  await expect(page.locator(".list .none")).toContainText('Nothing matches “"quarterly plan”');
  await expect(page.locator(".list .none span").last()).toHaveText(
    "Raiden reads your files' text while your Mac is plugged in.",
  );
});

test("no “Nothing matches” while the text search is still coming", async ({ page }) => {
  await open(page, "&contentswait=1");
  await page.keyboard.type('"ab');
  await expect(page.locator(".row .title")).toHaveText(["Ask AI"]);
  await expect(page.locator(".list .none")).toHaveCount(0);
  await page.keyboard.type("c");
  await expect
    .poll(() => sent(page, "contents"))
    .toContainEqual({ command: "contents", args: { query: "abc", asked: expect.any(Number) } });
  await page.waitForTimeout(100);
  await expect(page.locator(".list .none")).toHaveCount(0);
  await page.evaluate(() => window.finishContents?.("abc"));
  await expect(page.locator(".list .none")).toContainText('Nothing matches “"abc”');
});

test("rows the earlier search found stay while the next one runs", async ({ page }) => {
  await open(page, "&contentswait=1&contentsnowait=quarterly");
  await page.keyboard.type('"quarterly');
  await expect(page.locator(".row .title")).toHaveText(["notes.md", "Release notes.txt", "Ask AI"]);
  await page.keyboard.type(" plan");
  await expect(page.locator(".row .title")).toHaveText(["notes.md", "Ask AI"]);
  await expect(page.locator(".list .none")).toHaveCount(0);
});

test("a quoted search typed while the text is read fills in once it is done", async ({ page }) => {
  await open(page, "&contentstate=building&buildingempty=1");
  await page.keyboard.type('"quarterly plan');
  await expect(page.locator(".list .none")).toContainText(
    "Raiden is reading your files to search their text.",
  );
  const before = (await sent(page, "contents")).length;
  await page.evaluate(() => {
    window.finishBuilding?.();
    window.contentsReady?.();
  });
  await expect(page.locator(".row .title")).toHaveText(["notes.md", "Ask AI"]);
  await expect(page.locator(".list .note")).toHaveCount(0);
  await expect(page.locator(".list .none")).toHaveCount(0);
  const after = await sent(page, "contents");
  expect(after.slice(before)).toEqual([
    { command: "contents", args: { query: "quarterly plan", asked: expect.any(Number) } },
  ]);
});

test("the finished text index leaves a plain search alone", async ({ page }) => {
  await open(page);
  await page.keyboard.type("notes");
  await expect(page.locator(".row .title").first()).toHaveText("Notes");
  await page.evaluate(() => window.contentsReady?.());
  await page.waitForTimeout(100);
  expect(await sent(page, "contents")).toEqual([]);
});

import { expect, test, type Page } from "@playwright/test";

const sent = (page: Page, command: string) =>
  page.evaluate((name) => window.sent.filter((call) => call.command === name), command);

const open = async (page: Page, extra = "") => {
  await page.goto(`/scene.html?fixture=files${extra}`);
  await page.waitForLoadState("networkidle");
  await page.locator(".row.selected").waitFor();
};

const AGENTS = "/Users/me/Code/lantern/AGENTS.md";

test("AGENTS.md finds the file with its folder from home, and ⌘Enter reveals it", async ({
  page,
}) => {
  await open(page);
  await page.keyboard.type("AGENTS.md");
  await expect(page.locator(".sec")).toHaveText(["Files", "Use “AGENTS.md” with"]);
  await expect(page.locator(".row.selected .title")).toHaveText("AGENTS.md");
  await expect(page.locator(".row.selected .sub")).toHaveText("~/Code/lantern");
  await expect(page.locator(".row.selected img.icon")).toHaveAttribute(
    "src",
    "/scenes/icons/file-md.png",
  );
  await expect(page.locator("#ctx")).toHaveText("Files");
  await expect(page.locator("#primary")).toContainText("Open");
  await page.keyboard.press("Meta+Enter");
  await expect
    .poll(() => sent(page, "show_in_finder"))
    .toEqual([{ command: "show_in_finder", args: { path: AGENTS } }]);
});

test("Enter opens the file, and ⌘K lists Open, Show in Finder, Quick Look, Copy path, Open in Terminal and the size", async ({
  page,
}) => {
  await open(page);
  await page.keyboard.type("agents");
  await expect(page.locator(".row.selected .title")).toHaveText("AGENTS.md");
  await page.keyboard.press("Meta+k");
  await expect(page.locator("#actions .head")).toHaveText("AGENTS.md");
  await expect(page.locator(".act .act-t")).toHaveText([
    "Open",
    "Show in Finder",
    "Quick Look",
    "Copy path",
    "Open in Terminal",
    "Expand Window",
  ]);
  await page.keyboard.type("copy");
  await page.keyboard.press("Enter");
  await expect
    .poll(() => sent(page, "copy_path"))
    .toEqual([{ command: "copy_path", args: { path: AGENTS } }]);
  await page.keyboard.press("Enter");
  await expect
    .poll(() => sent(page, "open_file"))
    .toEqual([{ command: "open_file", args: { path: AGENTS } }]);
});

test("files come after apps, with the matched letters in bold", async ({ page }) => {
  await open(page);
  await page.keyboard.type("notes");
  await expect(page.locator(".sec")).toHaveText(["Results", "Files", "Use “notes” with"]);
  await expect(page.locator(".row .title")).toHaveText([
    "Notes",
    "notes.md",
    "Release notes.txt",
    "Meeting notes",
    "Ask AI",
  ]);
  await expect(page.locator(".row .title b").nth(2)).toHaveText("notes");
  await expect(page.locator(".row.selected .title")).toHaveText("Notes");
});

test("one or two letters search no files", async ({ page }) => {
  await open(page);
  await page.keyboard.type("no");
  await expect(page.locator(".row.selected .title")).toHaveText("Notes");
  expect(await sent(page, "files")).toEqual([]);
  await page.keyboard.type("t");
  await expect
    .poll(() => sent(page, "files"))
    .toEqual([{ command: "files", args: { query: "not", asked: expect.any(Number) } }]);
  await page.keyboard.press("Backspace");
  await expect(page.locator(".sec")).toHaveText(["Results", "Use “no” with"]);
});

test("an answer for an older query is dropped", async ({ page }) => {
  await open(page, "&fileswait=1&filesnowait=meeting");
  await page.keyboard.type("release");
  await page.keyboard.press("Meta+a");
  await page.keyboard.type("meeting");
  await expect(page.locator(".sec")).toHaveText(["Files", "Use “meeting” with"]);
  await page.evaluate(() => window.finishFiles?.("release"));
  await page.waitForTimeout(100);
  await expect(page.locator(".row .title")).toHaveText(["Meeting notes", "Ask AI"]);
});

test("files arriving late keep the row you moved to, or else the top row", async ({ page }) => {
  await open(page, "&fileswait=1");
  await page.keyboard.type("notes");
  await expect(page.locator(".row .title")).toHaveText(["Notes", "Ask AI"]);
  await page.keyboard.press("ArrowDown");
  await page.evaluate(() => window.finishFiles?.());
  await expect(page.locator(".row")).toHaveCount(5);
  await expect(page.locator(".row.selected .title")).toHaveText("Ask AI");
  await page.keyboard.press("Meta+a");
  await page.keyboard.type("meeting");
  await expect(page.locator(".row.selected .title")).toHaveText("Ask AI");
  await page.evaluate(() => window.finishFiles?.());
  await expect(page.locator(".row .title")).toHaveText(["Meeting notes", "Ask AI"]);
  await expect(page.locator(".row.selected .title")).toHaveText("Meeting notes");
});

test("files that arrive while Clipboard history covers the list show on the way back", async ({
  page,
}) => {
  await open(page, "&fileswait=1");
  await page.keyboard.type("meeting");
  await expect(page.locator(".row .title")).toHaveText(["Ask AI"]);
  await page.evaluate(() => window.openClipboard?.(false));
  await expect(page.locator("#chip")).toHaveText("Clipboard History");
  await page.evaluate(() => window.finishFiles?.());
  await page.keyboard.press("Escape");
  await expect(page.locator("#q")).toHaveValue("meeting");
  await expect(page.locator(".row .title")).toHaveText(["Meeting notes", "Ask AI"]);
});

test("⌘K on a file offers Open in Terminal, and ⌘⇧T opens its folder there", async ({ page }) => {
  await open(page);
  await page.keyboard.type("agents");
  await page.keyboard.press("Meta+k");
  await expect(page.locator(".act").nth(-2)).toHaveText("Open in Terminal⌘⇧T");
  await page.keyboard.press("Escape");
  await page.keyboard.press("Meta+Shift+t");
  await expect
    .poll(() => sent(page, "open_in_terminal"))
    .toEqual([{ command: "open_in_terminal", args: { path: AGENTS } }]);
});

test("a folder opens in the terminal too, and a failure is said in the footer", async ({
  page,
}) => {
  await open(page, "&terminalfails=1");
  await page.keyboard.type("meeting");
  await expect(page.locator(".row.selected .title")).toHaveText("Meeting notes");
  await page.keyboard.press("Meta+k");
  await page.keyboard.type("terminal");
  await page.keyboard.press("Enter");
  await expect
    .poll(() => sent(page, "open_in_terminal"))
    .toEqual([
      { command: "open_in_terminal", args: { path: "/Users/me/Documents/Meeting notes" } },
    ]);
  await expect(page.locator("#actions")).toBeHidden();
  await expect(page.locator("#ctx")).toHaveText("Ghostty did not open: no such folder.");
  await page.keyboard.press("ArrowDown");
  await expect(page.locator("#ctx")).not.toHaveText("Ghostty did not open: no such folder.");
});

const NOTES = "/Users/me/Code/lantern/docs/notes.md";
const RELEASE = "/Users/me/Documents/Release notes.txt";

test("Space after ↓ previews the file with Quick Look, follows the selection, and closes again", async ({
  page,
}) => {
  await open(page);
  await page.keyboard.type("notes");
  await expect(page.locator(".row")).toHaveCount(5);
  await page.keyboard.press("ArrowDown");
  await expect(page.locator(".row.selected .title")).toHaveText("notes.md");
  await page.keyboard.press("Space");
  await expect(page.locator("#q")).toHaveValue("notes");
  await expect
    .poll(() => sent(page, "quick_look"))
    .toEqual([{ command: "quick_look", args: { path: NOTES, follow: false } }]);
  await page.keyboard.press("ArrowDown");
  await expect
    .poll(() => sent(page, "quick_look"))
    .toContainEqual({ command: "quick_look", args: { path: RELEASE, follow: true } });
  await page.keyboard.press("Space");
  await expect.poll(() => sent(page, "close_quick_look")).toHaveLength(1);
  expect(await sent(page, "quick_look")).toHaveLength(2);
  await expect(page.locator("#q")).toHaveValue("notes");
});

test("the preview survives Raiden losing focus to it for a moment, and keeps following ↓", async ({
  page,
}) => {
  await open(page);
  await page.keyboard.type("notes");
  await page.keyboard.press("ArrowDown");
  await page.keyboard.press("Space");
  await expect
    .poll(() => sent(page, "quick_look"))
    .toEqual([{ command: "quick_look", args: { path: NOTES, follow: false } }]);
  await page.evaluate(() => {
    window.dispatchEvent(new Event("blur"));
    window.dispatchEvent(new Event("focus"));
  });
  await page.waitForTimeout(500);
  await expect(page.locator(".row.selected .title")).toHaveText("notes.md");
  await expect(page.locator("#q")).toHaveValue("notes");
  expect(await sent(page, "close_quick_look")).toEqual([]);
  await page.keyboard.press("ArrowDown");
  await expect
    .poll(() => sent(page, "quick_look"))
    .toContainEqual({ command: "quick_look", args: { path: RELEASE, follow: true } });
});

test("when focus does not come back, the preview ends and Raiden starts over as after any blur", async ({
  page,
}) => {
  await open(page);
  await page.keyboard.type("notes");
  await page.keyboard.press("ArrowDown");
  await page.keyboard.press("Space");
  await expect.poll(() => sent(page, "quick_look")).toHaveLength(1);
  await page.evaluate(() => window.dispatchEvent(new Event("blur")));
  await page.waitForTimeout(500);
  await page.evaluate(() => window.dispatchEvent(new Event("focus")));
  await page.keyboard.press("ArrowDown");
  await page.waitForTimeout(200);
  expect((await sent(page, "quick_look")).filter((call) => call.args?.["follow"] === true)).toEqual(
    [],
  );
  expect(await sent(page, "close_quick_look")).toHaveLength(1);
});

test("focus bouncing to the preview again and again never resets Raiden behind it", async ({
  page,
}) => {
  await open(page);
  await page.keyboard.type("notes");
  await page.keyboard.press("ArrowDown");
  await page.keyboard.press("Space");
  await expect.poll(() => sent(page, "quick_look")).toHaveLength(1);
  for (let bounce = 0; bounce < 4; bounce += 1) {
    await page.evaluate(() => {
      window.dispatchEvent(new Event("blur"));
      window.dispatchEvent(new Event("focus"));
    });
    await page.waitForTimeout(120);
  }
  await page.waitForTimeout(400);
  await expect(page.locator(".row.selected .title")).toHaveText("notes.md");
  await page.keyboard.press("ArrowDown");
  await expect
    .poll(() => sent(page, "quick_look"))
    .toContainEqual({ command: "quick_look", args: { path: RELEASE, follow: true } });
});

test("hiding while previewing starts over at once, even if Raiden is opened again right away", async ({
  page,
}) => {
  await open(page);
  await page.keyboard.type("notes");
  await page.keyboard.press("ArrowDown");
  await page.keyboard.press("Space");
  await expect.poll(() => sent(page, "quick_look")).toHaveLength(1);
  await page.evaluate(() => {
    window.dispatchEvent(new Event("blur"));
    window.previewEvent?.("hidden");
    window.dispatchEvent(new Event("focus"));
  });
  await page.keyboard.press("ArrowDown");
  await page.waitForTimeout(200);
  expect((await sent(page, "quick_look")).filter((call) => call.args?.["follow"] === true)).toEqual(
    [],
  );
});

test("after the preview's own close button, Space types again and Escape works as usual", async ({
  page,
}) => {
  await open(page);
  await page.keyboard.type("release");
  await page.keyboard.press("Meta+y");
  await expect.poll(() => sent(page, "quick_look")).toHaveLength(1);
  await page.evaluate(() => window.previewEvent?.("closed"));
  await page.keyboard.type(" notes");
  await expect(page.locator("#q")).toHaveValue("release notes");
  expect(await sent(page, "quick_look")).toHaveLength(1);
  await page.keyboard.press("Escape");
  await expect(page.locator("#q")).toHaveValue("");
  expect(await sent(page, "close_quick_look")).toEqual([]);
});

test("typing on after ⌘Y types its spaces and keeps the preview open", async ({ page }) => {
  await open(page);
  await page.keyboard.type("release");
  await page.keyboard.press("Meta+y");
  await expect.poll(() => sent(page, "quick_look")).toHaveLength(1);
  await page.keyboard.type(" notes");
  await expect(page.locator("#q")).toHaveValue("release notes");
  expect(await sent(page, "close_quick_look")).toEqual([]);
  expect(await sent(page, "quick_look")).toHaveLength(1);
});

test("the ⌘K menu keeps the keyboard when the preview gives it back", async ({ page }) => {
  await open(page);
  await page.keyboard.type("agents");
  await page.keyboard.press("Meta+y");
  await expect.poll(() => sent(page, "quick_look")).toHaveLength(1);
  await page.keyboard.press("Meta+k");
  await expect(page.locator(".act").first()).toBeVisible();
  await page.evaluate(() => {
    window.dispatchEvent(new Event("blur"));
    window.dispatchEvent(new Event("focus"));
  });
  await page.keyboard.type("copy");
  await expect(page.locator("#q")).toHaveValue("agents");
  await expect(page.locator(".act .act-t")).toHaveText(["Copy path"]);
});

test("open, close, open and ↓ typed fast still end on the file the selection is on", async ({
  page,
}) => {
  await open(page);
  await page.keyboard.type("notes");
  await page.keyboard.press("ArrowDown");
  await page.keyboard.press("Space");
  await page.keyboard.press("Space");
  await page.keyboard.press("Space");
  await page.keyboard.press("ArrowDown");
  await expect
    .poll(async () => (await sent(page, "quick_look")).at(-1))
    .toEqual({ command: "quick_look", args: { path: RELEASE, follow: true } });
  const opens = (await sent(page, "quick_look")).filter((call) => call.args?.["follow"] === false);
  expect(opens).toHaveLength(2);
});

test("moving the caret after ↓ makes Space type a space again", async ({ page }) => {
  await open(page);
  await page.keyboard.type("releasenotes");
  await page.keyboard.press("ArrowDown");
  for (let step = 0; step < 5; step += 1) {
    await page.keyboard.press("ArrowLeft");
  }
  await page.keyboard.press("Space");
  await expect(page.locator("#q")).toHaveValue("release notes");
  expect(await sent(page, "quick_look")).toEqual([]);
});

test("Space types a space in “release notes”, and again after a letter typed past ↓", async ({
  page,
}) => {
  await open(page);
  await page.keyboard.type("release notes");
  await expect(page.locator("#q")).toHaveValue("release notes");
  await page.keyboard.press("Meta+a");
  await page.keyboard.type("notes");
  await page.keyboard.press("ArrowDown");
  await page.keyboard.type("x y");
  await expect(page.locator("#q")).toHaveValue("notesx y");
  expect(await sent(page, "quick_look")).toEqual([]);
});

test("⌘Y previews while typing, Escape closes the preview without hiding, and ⌘K lists Quick Look", async ({
  page,
}) => {
  await open(page);
  await page.keyboard.type("agents");
  await expect(page.locator(".row.selected .title")).toHaveText("AGENTS.md");
  await page.keyboard.press("Meta+y");
  await expect
    .poll(() => sent(page, "quick_look"))
    .toEqual([{ command: "quick_look", args: { path: AGENTS, follow: false } }]);
  await page.keyboard.press("Escape");
  await expect.poll(() => sent(page, "close_quick_look")).toHaveLength(1);
  expect(await sent(page, "hide")).toEqual([]);
  await expect(page.locator("#q")).toHaveValue("agents");
  await page.keyboard.press("Meta+k");
  await expect(page.locator(".act .act-t")).toHaveText([
    "Open",
    "Show in Finder",
    "Quick Look",
    "Copy path",
    "Open in Terminal",
    "Expand Window",
  ]);
  await expect(page.locator(".act").nth(2)).toContainText("Y");
});

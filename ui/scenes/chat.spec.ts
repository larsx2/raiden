import { expect, test, type Page } from "@playwright/test";
import recorded from "./answer.json" with { type: "json" };

const openScene = async (page: Page, query: string) => {
  await page.goto(`/scene.html?fixture=answer${query}`);
  await page.waitForLoadState("load");
};

const box = async (page: Page, selector: string) => {
  const found = await page.locator(selector).first().boundingBox();
  if (!found) {
    throw new Error(`${selector} is not on screen`);
  }
  return found;
};

test("your question is a bubble on the right, and the answer takes the full width", async ({
  page,
}) => {
  await openScene(page, "&moment=done");
  await expect(page.locator("#ai")).toHaveAttribute("data-phase", "done");
  const thread = await box(page, "#ai");
  const bubble = await box(page, ".ai-q");
  const answer = await box(page, ".ai-a");
  expect(Math.round(thread.x + thread.width - (bubble.x + bubble.width))).toBe(24);
  expect(bubble.width).toBeLessThanOrEqual((thread.width - 48) * 0.7);
  expect(bubble.x).toBeGreaterThan(thread.x + thread.width / 2);
  expect(Math.round(answer.x - thread.x)).toBe(24);
  expect(Math.round(answer.width)).toBe(Math.round(thread.width - 48));
  await expect(page.locator(".ai-q")).toHaveCSS("border-radius", "16px 16px 5px");
});

test("Tab from the search puts the question in the thread and focuses the field at the bottom", async ({
  page,
}) => {
  await openScene(page, "");
  await page.locator(".row.selected").waitFor();
  await page.keyboard.type("why does it fail");
  await page.keyboard.press("Tab");
  await expect(page.locator(".ai-q")).toHaveText("why does it fail");
  await expect(page.locator("#q")).toBeFocused();
  await expect(page.locator("#q")).toHaveAttribute("placeholder", "Ask a follow-up");
  const header = await box(page, "header");
  const field = await box(page, "#q");
  const footer = await box(page, "footer");
  expect(field.y).toBeGreaterThan(header.y + header.height);
  expect(Math.round(footer.y - (field.y + field.height))).toBe(12);
  expect(Math.round(field.height)).toBe(44);
  await expect(page.locator("header #chip")).toHaveText("Ask AI");
  await expect(page.locator("header #back")).toBeVisible();
  await expect(page.locator("header input")).toHaveCount(0);
  const order = await page.evaluate(() =>
    ["#ai", ".cx-row", "#q", "footer"].map((selector) =>
      [...document.querySelectorAll("#app *")].indexOf(document.querySelector(selector) as Element),
    ),
  );
  expect(order.every((at, index) => at >= 0 && (index === 0 || at > (order[index - 1] ?? 0)))).toBe(
    true,
  );
  await page.keyboard.press("Escape");
  await page.keyboard.press("Escape");
  await expect(page.locator("#ai")).toHaveCount(0);
  await expect(page.locator("header #q")).toBeFocused();
});

test("keys typed right after Tab land in the field at the bottom, in order", async ({ page }) => {
  await openScene(page, "&hold=1&frontwaits=1");
  await page.locator(".row.selected").waitFor();
  await page.keyboard.type("how do I land");
  await page.keyboard.press("Tab");
  await page.keyboard.type("and after that?");
  await expect(page.locator("#q")).toHaveValue("and after that?");
  await page.evaluate(() => window.finishFront?.());
  await expect(page.locator("#ai")).toHaveAttribute("data-phase", "streaming");
  await expect(page.locator(".ai-a")).toContainText("Run just land");
  await expect(page.locator("#q")).toHaveValue("and after that?");
  await expect(page.locator(".ai-q")).toHaveText(["how do I land"]);
  await expect(page.locator("#q")).toBeFocused();
});

test("the context tag sits on its own line just above the field, and its button removes it", async ({
  page,
}) => {
  await openScene(page, "&selection=SELECT%201");
  await page.locator(".row.selected").waitFor();
  await page.keyboard.press("Tab");
  await expect(page.locator(".cx")).toHaveText(["AGENTS.md in Zed", "Selection from Zed"]);
  const thread = await box(page, "#ai");
  const tags = await box(page, ".cx-row");
  const tag = await box(page, ".cx");
  const field = await box(page, "#q");
  expect(tags.y).toBeGreaterThanOrEqual(thread.y + thread.height);
  expect(Math.round(field.y - (tag.y + tag.height))).toBe(8);
  expect(Math.round(tag.x)).toBe(Math.round(field.x));
  await page.locator(".cx-x").last().click();
  await expect(page.locator(".cx")).toHaveText(["AGENTS.md in Zed"]);
  await expect(page.locator("#q")).toBeFocused();
  await page.keyboard.press("Backspace");
  await expect(page.locator(".cx")).toHaveCount(0);
  await expect(page.locator(".cx-row")).toHaveCount(0);
  await expect(page.locator("#ai")).toHaveAttribute("data-phase", "waiting");
});

test("the footer shows the Vim mode, the engine and its model, then the primary action and Actions", async ({
  page,
}) => {
  await openScene(page, "&moment=streaming&hold=1&vim=1");
  await expect(page.locator("#ai")).toHaveAttribute("data-phase", "streaming");
  await expect(page.locator("#ctx")).toHaveText("INSERTClaude Haiku, on your Claude subscription");
  await expect(page.locator("#ctx .sr")).toHaveText(", on your Claude subscription");
  await expect(page.locator("#ctx .why")).toHaveCount(0);
  await expect(page.locator("#ctx > *").first()).toHaveClass(/nm/);
  await expect(page.locator("#ctx .ico.brand")).toBeVisible();
  await expect(page.locator("footer .right > *")).toHaveText(["Stop ⌘.", "", "Actions ⌘K"]);
  await expect(page.locator("footer .right .vsep")).toHaveCount(1);
  await page.keyboard.press("Meta+.");
  await expect(page.locator("#ai")).toHaveAttribute("data-phase", "stopped");
  await expect(page.locator("#primary")).toHaveText("Paste into Zed ↵");
});

test("a long thread stays scrolled to its newest words, just above the tags, with the top faded", async ({
  page,
}) => {
  await openScene(page, "&moment=thread&long=3");
  await expect(page.locator(".ai-q")).toHaveCount(2);
  await expect(page.locator("#ai")).toHaveAttribute("data-phase", "done");
  const thread = page.locator("#ai");
  await expect
    .poll(() =>
      thread.evaluate((element) => element.scrollHeight - element.scrollTop - element.clientHeight),
    )
    .toBeLessThanOrEqual(1);
  const view = await box(page, "#ai");
  const last = await box(page, ".ai-a >> nth=-1");
  expect(Math.round(view.y + view.height - (last.y + last.height))).toBe(14);
  await expect(thread).toHaveClass(/faded/);
  await thread.evaluate((element) => {
    element.scrollTop = 0;
  });
  await expect(thread).not.toHaveClass(/faded/);
  await thread.evaluate((element) => {
    element.scrollTop = 40;
  });
  await expect(thread).toHaveClass(/faded/);
  await page.keyboard.press("Meta+,");
  await expect(page.locator("header #chip")).toHaveText("Settings");
  await page.keyboard.press("Escape");
  await expect(page.locator("header #chip")).toHaveText("Ask AI");
  await expect
    .poll(() =>
      thread.evaluate((element) => element.classList.contains("faded") === element.scrollTop > 0),
    )
    .toBe(true);
});

test("a short first answer starts under the header, and nothing is faded", async ({ page }) => {
  await openScene(page, "&moment=streaming&hold=1");
  await expect(page.locator("#ai")).toHaveAttribute("data-phase", "streaming");
  const header = await box(page, "header");
  const bubble = await box(page, ".ai-q");
  expect(Math.round(bubble.y - (header.y + header.height))).toBe(16);
  await expect(page.locator(".ai-q")).toHaveText(recorded.question);
  await expect(page.locator("#ai")).not.toHaveClass(/faded/);
});

test("Ask AI with nothing asked shows the short text in the middle, with the tag and field below", async ({
  page,
}) => {
  await openScene(page, "&moment=waiting");
  await expect(page.locator("#ai")).toHaveAttribute("data-phase", "waiting");
  const thread = await box(page, "#ai");
  const empty = await box(page, ".ai-empty");
  const middle = empty.y + empty.height / 2;
  expect(Math.abs(middle - (thread.y + thread.height / 2))).toBeLessThanOrEqual(2);
  const tag = await box(page, ".cx");
  const field = await box(page, "#q");
  expect(tag.y).toBeGreaterThan(empty.y + empty.height);
  expect(field.y).toBeGreaterThan(tag.y + tag.height);
});

test("growing the window keeps a long thread at its newest words", async ({ page }) => {
  await openScene(page, "&moment=thread&long=3");
  await expect(page.locator(".ai-q")).toHaveCount(2);
  await expect(page.locator("#ai")).toHaveAttribute("data-phase", "done");
  const thread = page.locator("#ai");
  const below = () =>
    thread.evaluate((element) => element.scrollHeight - element.scrollTop - element.clientHeight);
  await expect.poll(below).toBeLessThanOrEqual(1);
  const before = await box(page, "#ai");
  await page.keyboard.press("Meta+Shift+E");
  await expect.poll(async () => (await box(page, "#ai")).height).toBeGreaterThan(before.height);
  await expect.poll(below).toBeLessThanOrEqual(1);
  await expect
    .poll(() =>
      thread.evaluate((element) => element.classList.contains("faded") === element.scrollTop > 0),
    )
    .toBe(true);
});

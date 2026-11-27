import { expect, test, type Page } from "@playwright/test";
import { SCENE_TIME } from "./scenes";

const sent = (page: Page, command: string) =>
  page.evaluate((name) => window.sent.filter((call) => call.command === name), command);

const openSlack = async (page: Page, params = "") => {
  await page.clock.setFixedTime(new Date(SCENE_TIME));
  await page.goto(`/scene.html?fixture=slack${params}`);
  await page.waitForLoadState("networkidle");
  await expect(page.locator("#chip")).toHaveText("Slack");
};

const names = (page: Page) => page.locator(".slackrow .name");

test.describe("getting there", () => {
  test("typing finds Slack under Slack, and Enter opens it", async ({ page }) => {
    await page.clock.setFixedTime(new Date(SCENE_TIME));
    await page.goto("/scene.html?fixture=apps&commands=1");
    await page.waitForLoadState("networkidle");
    await page.locator(".row.selected").waitFor();
    await page.keyboard.type("slack");
    await expect(page.locator(".row.selected .title")).toHaveText("Slack");
    await expect(page.locator(".row.selected .sub")).toHaveText("Slack");
    await expect(page.locator("#primary")).toContainText("Open Slack");
    await page.keyboard.press("Enter");
    await expect(page.locator("#chip")).toHaveText("Slack");
    await expect(page.locator("#q")).toHaveAttribute("placeholder", "Search Slack");
    await expect(page.locator("#q")).toHaveValue("");
    await expect
      .poll(() => sent(page, "slack_search"))
      .toEqual([{ command: "slack_search", args: { query: "" } }]);
    await page.keyboard.press("Escape");
    await expect(page.locator("#chip")).toBeHidden();
    await expect(page.locator("#q")).toHaveValue("slack");
  });

  test("Backspace on an empty search goes back, like Esc", async ({ page }) => {
    await openSlack(page);
    await page.keyboard.press("Backspace");
    await expect(page.locator("#chip")).toBeHidden();
    await expect(page.locator("#q")).toHaveAttribute("placeholder", "Search apps and commands");
  });
});

test.describe("results", () => {
  test("shows the catalog with unread direct messages first and unread counts", async ({
    page,
  }) => {
    await openSlack(page);
    await expect(names(page)).toHaveText([
      "Ana Lee",
      "Ana, Sam, You",
      "#general",
      "#random",
      "Sam Ortiz",
    ]);
    await expect(page.locator(".slackrow").first().locator(".badge.unread")).toHaveText("3");
  });

  test("the arrows stop at the first and last result", async ({ page }) => {
    await openSlack(page);
    await expect(page.locator(".slackrow.selected .name")).toHaveText("Ana Lee");
    await page.keyboard.press("ArrowUp");
    await expect(page.locator(".slackrow.selected .name")).toHaveText("Ana Lee");
    for (const _ of Array.from({ length: 10 })) {
      await page.keyboard.press("ArrowDown");
    }
    await expect(page.locator(".slackrow").last()).toHaveClass(/selected/);
  });

  test("typing filters people and channels, and asks the backend for the query", async ({
    page,
  }) => {
    await openSlack(page);
    await page.keyboard.type("sam");
    await expect(names(page)).toHaveText(["Ana, Sam, You", "Sam Ortiz"]);
    await expect
      .poll(() => sent(page, "slack_search"))
      .toContainEqual({ command: "slack_search", args: { query: "sam" } });
  });

  test("message matches appear only from three letters on", async ({ page }) => {
    await openSlack(page);
    await page.keyboard.type("sh");
    await expect(page.locator(".slackrow[data-kind='message']")).toHaveCount(0);
    await page.keyboard.type("i");
    await expect(page.locator(".slackrow[data-kind='message'] .name")).toHaveText(
      "ship the launcher tonight",
    );
  });

  test("Enter opens the selected conversation in Slack", async ({ page }) => {
    await openSlack(page);
    await page.keyboard.press("Enter");
    // slack_open validates the destination and hides the launcher natively.
    await expect
      .poll(() => sent(page, "slack_open"))
      .toEqual([{ command: "slack_open", args: { url: "slack://user?team=T1&id=U1" } }]);
  });

  test("⌘⌫ on an empty search signs out and returns to onboarding", async ({ page }) => {
    await openSlack(page);
    await page.keyboard.press("Meta+Backspace");
    await expect.poll(() => sent(page, "slack_sign_out")).toHaveLength(1);
    await expect(page.locator(".slack-gate")).toHaveCount(1);
    await expect(page.locator("#q")).toHaveValue("");
  });
});

test.describe("onboarding", () => {
  test("with no token, it shows install steps and takes a pasted token", async ({ page }) => {
    await openSlack(page, "&slacktoken=0");
    await expect(page.locator(".slack-gate .et")).toHaveText("Search Slack");
    await expect(page.locator(".slack-gate .steps li")).toHaveCount(4);
    await expect(page.locator("#q")).toHaveAttribute(
      "placeholder",
      /Paste your Slack User OAuth Token/,
    );
    await page.getByRole("button", { name: "Copy manifest" }).click();
    await expect.poll(() => sent(page, "slack_manifest")).toHaveLength(1);
    await page.keyboard.type('"xoxp-123-abc"');
    await page.keyboard.press("Enter");
    await expect
      .poll(() => sent(page, "slack_save_token"))
      .toEqual([{ command: "slack_save_token", args: { token: "xoxp-123-abc" } }]);
    await expect(page.locator(".slack-gate")).toHaveCount(0);
    await expect(names(page).first()).toHaveText("Ana Lee");
    await expect(page.locator("#q")).toHaveAttribute("placeholder", "Search Slack");
    // The field is cleared so the pasted token is never sent as a search.
    await expect(page.locator("#q")).toHaveValue("");
  });

  test("a rejected token keeps onboarding open and shows why", async ({ page }) => {
    await openSlack(page, "&slacktoken=0");
    await page.keyboard.type("not-a-real-token");
    await page.keyboard.press("Enter");
    await expect(page.locator(".slack-gate")).toHaveCount(1);
    await expect(page.locator(".slack-gate .ep")).toContainText("Slack rejected the token");
    await expect(page.locator("#q")).toHaveAttribute(
      "placeholder",
      /Paste your Slack User OAuth Token/,
    );
  });
});

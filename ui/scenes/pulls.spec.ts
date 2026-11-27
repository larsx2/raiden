import { expect, test, type Page } from "@playwright/test";
import { SCENE_TIME } from "./scenes";

const sent = (page: Page, command: string) =>
  page.evaluate((name) => window.sent.filter((call) => call.command === name), command);

const openPulls = async (page: Page, params = "") => {
  await page.clock.setFixedTime(new Date(SCENE_TIME));
  await page.goto(`/scene.html?fixture=pulls${params}`);
  await page.waitForLoadState("load");
  await expect(page.locator("#chip")).toHaveText("Pull Requests");
};

const rows = (page: Page) => page.locator(".pr .hint span:last-child");

const askAboutThese = async (page: Page) => {
  await page.keyboard.press("Meta+k");
  await page.keyboard.type("Ask AI About");
  await page.keyboard.press("Enter");
};

test("the arrows stop at the first and last pull request", async ({ page }) => {
  await openPulls(page, "");
  const first = await page.locator(".pr.selected .name").textContent();
  await page.keyboard.press("ArrowUp");
  await expect(page.locator(".pr.selected .name")).toHaveText(first ?? "");
  await expect(page.locator(".lst .pr").first()).toHaveClass(/selected/);
  for (const _ of Array.from({ length: 20 })) {
    await page.keyboard.press("ArrowDown");
  }
  await expect(page.locator(".lst .pr").last()).toHaveClass(/selected/);
});

test.describe("getting there", () => {
  test("typing finds Pull Requests under GitHub with what needs review, and Enter opens it", async ({
    page,
  }) => {
    await page.clock.setFixedTime(new Date(SCENE_TIME));
    await page.goto("/scene.html?fixture=apps&commands=1");
    await page.waitForLoadState("load");
    await page.locator(".row.selected").waitFor();
    await page.keyboard.type("pull");
    await expect(page.locator(".row.selected .title")).toHaveText("Pull Requests");
    await expect(page.locator(".row.selected .sub")).toHaveText("GitHub");
    await expect(page.locator(".row.selected .acc")).toHaveText("1 needs your review");
    await expect(page.locator(".row.selected .ico.gh")).toBeVisible();
    await expect(page.locator("#primary")).toContainText("Open Pull Requests");
    await page.keyboard.press("Enter");
    await expect(page.locator("#chip")).toHaveText("Pull Requests");
    await expect(page.locator("#chip .ico.gh")).toBeVisible();
    await expect(page.locator("#q")).toHaveAttribute("placeholder", "Filter pull requests");
    await expect(page.locator("#q")).toHaveValue("");
    await expect(page.locator("#hint")).toBeHidden();
    await expect
      .poll(() => sent(page, "pull_requests_refresh"))
      .toEqual([{ command: "pull_requests_refresh", args: { force: false } }]);
    expect(await sent(page, "pull_requests")).toEqual([
      { command: "pull_requests", args: { query: "pull", row: 0 } },
    ]);
    await page.keyboard.press("Escape");
    await expect(page.locator("#chip")).toBeHidden();
    await expect(page.locator("#q")).toHaveValue("pull");
  });

  test("Backspace on an empty filter goes back, like Esc", async ({ page }) => {
    await openPulls(page);
    await page.keyboard.press("Backspace");
    await expect(page.locator("#chip")).toBeHidden();
    await expect(page.locator("#q")).toHaveAttribute("placeholder", "Search apps and commands");
  });

  test("hiding the panel leaves the view, even from Ask AI over it", async ({ page }) => {
    await openPulls(page, "&only=fernhill/beacon");
    await askAboutThese(page);
    await expect(page.locator("#chip")).toHaveText("Ask AI");
    await page.evaluate(() => window.dispatchEvent(new Event("blur")));
    await expect(page.locator("#chip")).toBeHidden();
    await expect(page.locator("#q")).toHaveAttribute("placeholder", "Search apps and commands");
    await page.evaluate(() => window.dispatchEvent(new Event("focus")));
    await page.keyboard.press("Escape");
    await expect(page.locator("#chip")).toBeHidden();
  });

  test("the first time with no saved list, it says it is reading until the refresh returns", async ({
    page,
  }) => {
    await openPulls(page, "&fetched=none&refreshwaits=1");
    await expect(page.locator(".sc")).toHaveText(["All", "Failing", "Ready", "Drafts"]);
    await expect(page.locator(".sc .n")).toHaveCount(0);
    await expect(page.locator(".full .none")).toHaveText("Reading your pull requests through gh…");
    await expect(page.locator(".split")).toHaveCount(0);
    await expect(page.locator("header.loading")).toHaveCount(1);
    await page.evaluate(() => window.finishRefresh?.());
    await expect(page.locator(".pr")).toHaveCount(13);
    await expect(page.locator("header.loading")).toHaveCount(0);
    await expect(page.locator(".sc")).toHaveText(["All13", "Failing4", "Ready5", "Drafts3"]);
    await expect(page.locator(".full")).toHaveCount(0);
  });

  test("with no saved list, a refresh that fails says why in place of the list", async ({
    page,
  }) => {
    await openPulls(page, "&fetched=none&refreshfails=1&problem=failed");
    await expect(page.locator(".full .none")).toHaveText(
      "gh could not list your pull requests.⌘R tries again.",
    );
    await expect(page.locator(".full .none span")).toHaveText("⌘R tries again.");
    await expect(page.locator(".split")).toHaveCount(0);
    await expect(page.locator("#ctx")).toHaveText("");
  });

  test("shows the saved list at once and says how old it is while refreshing", async ({ page }) => {
    await openPulls(page, "&fetched=240&refreshwaits=1");
    await expect(page.locator(".pr")).toHaveCount(13);
    await expect(page.locator("#ctx")).toHaveText("");
  });
});

test.describe("the tabs", () => {
  test("count each state, and ⌘1 to ⌘4 switch between them", async ({ page }) => {
    await openPulls(page);
    await expect(page.locator(".sc")).toHaveText(["All13", "Failing4", "Ready5", "Drafts3"]);
    await expect(page.locator(".sc.on")).toHaveText("All13");
    await expect(page.locator("#ctx")).toHaveText("");
    await expect(page.locator(".pr.selected .name")).toHaveText(
      "Add a retry budget to the sync worker",
    );
    await expect(page.locator(".det .meta").first()).toHaveText(
      "fernhill/beacon #169, retry-budget into main, updated 1d ago",
    );
    await page.keyboard.press("Meta+2");
    await expect(page.locator(".sc.on")).toHaveText("Failing4");
    await expect(rows(page)).toHaveText(["#169", "#168", "#937", "#163"]);
    await expect(page.locator("#ctx")).toHaveText("");
    await page.keyboard.press("Meta+3");
    await expect(page.locator(".sc.on")).toHaveText("Ready5");
    await expect(page.locator("#ctx")).toHaveText("");
    await expect(page.locator(".blocker")).toHaveText("Ready to merge: checks pass, no conflicts");
    await page.keyboard.press("Meta+4");
    await expect(rows(page)).toHaveText(["#169", "#168", "#122"]);
    await page.keyboard.press("Meta+1");
    await expect(page.locator(".pr")).toHaveCount(13);
  });

  test("clicking a tab shows it", async ({ page }) => {
    await openPulls(page);
    await page.locator(".sc", { hasText: "Drafts" }).click();
    await expect(page.locator(".sc.on")).toHaveText("Drafts3");
    await expect(page.locator("#q")).toBeFocused();
  });
});

test.describe("filtering", () => {
  test("matches the number and the branch, and never changes the tab", async ({ page }) => {
    await openPulls(page, "&tab=failing");
    await page.keyboard.type("#163");
    await expect(rows(page)).toHaveText(["#163"]);
    await expect(page.locator(".sc.on")).toHaveText("Failing4");
    await page.keyboard.press("Meta+a");
    await page.keyboard.type("claims-gate");
    await expect(rows(page)).toHaveText(["#168"]);
    await page.keyboard.press("Meta+a");
    await page.keyboard.type("harbor");
    await expect(rows(page)).toHaveText(["#937"]);
    await expect(page.locator(".sc.on")).toHaveText("Failing4");
  });

  test("typing filters on screen and asks the backend for nothing but timings", async ({
    page,
  }) => {
    await openPulls(page);
    await expect(page.locator(".pr")).toHaveCount(13);
    await expect.poll(() => sent(page, "pull_requests_refresh")).toHaveLength(1);
    const before = await page.evaluate(() => window.sent.length);
    await page.keyboard.type("retry budget");
    await expect(rows(page)).toHaveText(["#169"]);
    await page.keyboard.press("Backspace");
    await page.keyboard.press("Meta+2");
    await page.keyboard.type("t");
    await expect(page.locator(".sc.on")).toHaveText("Failing4");
    const timings = ["keystroke", "frame"];
    const asked = await page.evaluate((from) => window.sent.slice(from), before);
    expect(asked.filter((call) => !timings.includes(call.command))).toEqual([]);
  });

  test("a filter with no match says how to clear it, and Esc clears it", async ({ page }) => {
    await openPulls(page);
    await page.keyboard.type("zzz");
    await expect(page.locator(".full .none")).toHaveText(
      "Nothing matches “zzz”.Press Esc to clear the search.",
    );
    await page.keyboard.press("Escape");
    await expect(page.locator("#q")).toHaveValue("");
    await expect(page.locator(".pr")).toHaveCount(13);
    await expect(page.locator("#chip")).toHaveText("Pull Requests");
  });

  test("inside a filter, no match says how many match outside it", async ({ page }) => {
    await openPulls(page, "&only=fernhill/beacon&query=release");
    await expect(page.locator(".full .none")).toHaveText(
      "Nothing matches “release”.2 match outside beacon; Esc clears the filter.",
    );
    await page.keyboard.press("Escape");
    await expect(page.locator(".fchip")).toHaveCount(0);
    await expect(page.locator("#q")).toHaveValue("release");
    await expect(page.locator(".pr .name")).toHaveText([
      "feat(webhooks): send release warnings once",
      "docs: rollout checklist for the March release",
    ]);
    await page.keyboard.press("Escape");
    await expect(page.locator("#q")).toHaveValue("");
    await expect(page.locator("#chip")).toHaveText("Pull Requests");
  });

  test("⌘K offers no Ask AI when no pull request is on screen", async ({ page }) => {
    await openPulls(page, "&only=fernhill/beacon&tab=ready");
    await expect(page.locator(".full .none")).toHaveText("Nothing ready to merge in beacon.");
    await page.keyboard.press("Meta+k");
    await expect(page.locator(".act", { hasText: "Ask AI About These" })).toHaveCount(0);
    await page.keyboard.press("Escape");
    await expect(page.locator("#chip")).toHaveText("Pull Requests");
    expect(await sent(page, "pull_requests_context")).toEqual([]);
  });

  test("an empty tab says so, inside the filter", async ({ page }) => {
    await openPulls(page, "&only=fernhill/beacon&tab=ready");
    await expect(page.locator(".full .none")).toHaveText("Nothing ready to merge in beacon.");
  });
});

test.describe("acting on a pull request", () => {
  test("Enter opens it in the browser and ⌘⇧C copies its link", async ({ page }) => {
    await openPulls(page);
    await page.keyboard.press("ArrowDown");
    await expect(page.locator(".pr.selected .name")).toHaveText(
      "ci(deps): bump the actions group with 4 updates",
    );
    await page.keyboard.press("Meta+Shift+c");
    await expect(page.locator("#ctx")).toHaveText("Copied the link");
    expect(await sent(page, "copy_text")).toEqual([
      { command: "copy_text", args: { text: "https://github.com/fernhill/ledger/pull/64" } },
    ]);
    await page.keyboard.press("Enter");
    await expect
      .poll(() => sent(page, "open_link"))
      .toEqual([
        { command: "open_link", args: { url: "https://github.com/fernhill/ledger/pull/64" } },
      ]);
  });

  test("a click on a row opens it like Enter, and the next opening starts with an empty search", async ({
    page,
  }) => {
    await page.clock.setFixedTime(new Date(SCENE_TIME));
    await page.goto("/scene.html?fixture=apps&commands=1");
    await page.waitForLoadState("load");
    await page.locator(".row.selected").waitFor();
    await page.keyboard.type("pull");
    await expect(page.locator(".row.selected .title")).toHaveText("Pull Requests");
    await page.keyboard.press("Enter");
    await page.locator(".pr", { hasText: "#64" }).click();
    await expect
      .poll(() => sent(page, "open_link"))
      .toEqual([
        { command: "open_link", args: { url: "https://github.com/fernhill/ledger/pull/64" } },
      ]);
    await page.evaluate(() => window.dispatchEvent(new Event("blur")));
    await page.evaluate(() => window.dispatchEvent(new Event("focus")));
    await expect(page.locator("#chip")).toBeHidden();
    await expect(page.locator("#q")).toHaveValue("");
  });

  test("after Enter opens one, the next opening starts with an empty search", async ({ page }) => {
    await page.clock.setFixedTime(new Date(SCENE_TIME));
    await page.goto("/scene.html?fixture=apps&commands=1");
    await page.waitForLoadState("load");
    await page.locator(".row.selected").waitFor();
    await page.keyboard.type("pull");
    await expect(page.locator(".row.selected .title")).toHaveText("Pull Requests");
    await page.keyboard.press("Enter");
    await expect(page.locator(".pr.selected")).toBeVisible();
    await page.keyboard.press("Enter");
    await expect.poll(() => sent(page, "open_link")).toHaveLength(1);
    await page.evaluate(() => window.dispatchEvent(new Event("blur")));
    await page.evaluate(() => window.dispatchEvent(new Event("focus")));
    await expect(page.locator("#chip")).toBeHidden();
    await expect(page.locator("#q")).toHaveValue("");
  });

  test("⌘R refreshes now", async ({ page }) => {
    await openPulls(page);
    await page.keyboard.press("Meta+r");
    await expect
      .poll(() => sent(page, "pull_requests_refresh"))
      .toContainEqual({ command: "pull_requests_refresh", args: { force: true } });
  });

  test("⌘K lists the pull request's actions, then ways to show only part of the list", async ({
    page,
  }) => {
    await openPulls(page);
    await page.keyboard.press("Meta+k");
    await expect(page.locator("#actions")).toHaveClass(/tall/);
    await expect(page.locator("#actions .head")).toHaveText(
      "#169 Add a retry budget to the sync worker",
    );
    await expect(page.locator("#actions .mh")).toHaveText(["Show only"]);
    await expect(page.locator(".act")).toHaveText([
      "Open in Browser↵",
      "Copy Link⌘⇧C",
      "Ask AI About These",
      "ledgerpinned⌘5",
      "tidewater/lanternpinned⌘6",
      "An organization…⌘O",
      "A repository…⌘⇧R",
      "Pin beacon⌘P",
      "Expand Window⌘⇧E",
    ]);
    await page.keyboard.press("Meta+5");
    await expect(page.locator("#actions")).toBeHidden();
    await expect(page.locator(".fchip")).toHaveText("ledger×");
    await expect(rows(page)).toHaveText(["#64", "#155"]);
  });

  test("⌘K acts on the pull request it names, and hovering rows under it selects nothing", async ({
    page,
  }) => {
    await openPulls(page);
    await page.keyboard.press("Meta+k");
    await expect(page.locator("#actions .head")).toHaveText(
      "#169 Add a retry budget to the sync worker",
    );
    const row = await page.locator(".pr").nth(2).boundingBox();
    const x = (row?.x ?? 0) + 40;
    const y = (row?.y ?? 0) + (row?.height ?? 0) / 2;
    await page.mouse.move(x, y);
    await page.mouse.move(x + 4, y);
    await expect(page.locator(".pr.selected .hint span:last-child")).toHaveText("#169");
    await page.keyboard.press("Meta+Shift+c");
    await expect
      .poll(() => sent(page, "copy_text"))
      .toEqual([
        { command: "copy_text", args: { text: "https://github.com/fernhill/beacon/pull/169" } },
      ]);
  });

  test("⌘P pins the selected pull request's repo, and again unpins it", async ({ page }) => {
    await openPulls(page);
    await page.keyboard.press("Meta+p");
    await expect
      .poll(() => sent(page, "pull_requests_pin"))
      .toEqual([{ command: "pull_requests_pin", args: { repo: "fernhill/beacon", pin: true } }]);
    await page.keyboard.press("Meta+k");
    await expect(page.locator(".act", { hasText: "pinned⌘7" })).toHaveText("beaconpinned⌘7");
    await expect(page.locator(".act").filter({ hasText: /pin beacon/i })).toHaveText(
      "Unpin beacon⌘P",
    );
    await page.keyboard.press("Escape");
    await page.keyboard.press("Meta+p");
    await expect
      .poll(() => sent(page, "pull_requests_pin"))
      .toContainEqual({
        command: "pull_requests_pin",
        args: { repo: "fernhill/beacon", pin: false },
      });
  });
});

test.describe("showing only part of the list", () => {
  test("⌘O lists organizations, Enter narrows, and Esc clears the filter, then leaves", async ({
    page,
  }) => {
    await openPulls(page);
    await page.keyboard.press("Meta+o");
    await expect(page.locator("#actions .head")).toHaveText("Show only an organization");
    await expect(page.locator(".act")).toHaveText([
      "fernhill10 open",
      "kestrel-io1 open",
      "larsx21 open",
      "tidewater1 open",
    ]);
    await expect(page.locator("#aq")).toHaveAttribute("placeholder", "Search organizations");
    await expect(page.locator("#primary")).toContainText("Show only");
    await page.keyboard.type("fern");
    await page.keyboard.press("Enter");
    await expect(page.locator("#actions")).toBeHidden();
    await expect(page.locator(".fchip")).toHaveText("fernhill×");
    await expect(page.locator(".sc")).toHaveText(["All10", "Failing4", "Ready4", "Drafts3"]);
    await expect(page.locator("#ctx")).toHaveText("");
    await page.keyboard.type("retry");
    await page.keyboard.press("Escape");
    await expect(page.locator("#q")).toHaveValue("");
    await expect(page.locator(".fchip")).toHaveText("fernhill×");
    await page.keyboard.press("Escape");
    await expect(page.locator(".fchip")).toBeHidden();
    await expect(page.locator("#chip")).toHaveText("Pull Requests");
    await page.keyboard.press("Escape");
    await expect(page.locator("#chip")).toBeHidden();
  });

  test("⌘⇧R lists repositories by shown name, and the chip's × clears the filter", async ({
    page,
  }) => {
    await openPulls(page);
    await page.keyboard.press("Meta+Shift+r");
    await expect(page.locator("#actions .head")).toHaveText("Show only a repository");
    await expect(page.locator(".act").first()).toHaveText("beacon5 open");
    await page.keyboard.type("lantern");
    await page.keyboard.press("Enter");
    await expect(page.locator(".fchip")).toHaveText("tidewater/lantern×");
    await expect(rows(page)).toHaveText(["#41"]);
    await expect(page.locator(".pr .tagc")).toHaveText("conflicts");
    await page.getByRole("button", { name: "Clear the filter" }).click();
    await expect(page.locator(".fchip")).toBeHidden();
    await expect(page.locator(".pr")).toHaveCount(13);
  });

  test("a pinned repo you lost access to says so, and ⌘⌫ unpins it", async ({ page }) => {
    await openPulls(page, "&lost=fernhill/old");
    await page.keyboard.press("Meta+7");
    await expect(page.locator(".full .none")).toHaveText(
      "You no longer have access to fernhill/old.⌘⌫ unpins.",
    );
    await page.keyboard.press("Meta+Backspace");
    await expect
      .poll(() => sent(page, "pull_requests_pin"))
      .toEqual([{ command: "pull_requests_pin", args: { repo: "fernhill/old", pin: false } }]);
    await expect(page.locator(".fchip")).toBeHidden();
    await expect(page.locator(".pr")).toHaveCount(13);
  });
});

test.describe("asking AI", () => {
  test("Tab and Shift Tab move between the tabs, stop at the ends, and never start Ask AI", async ({
    page,
  }) => {
    await openPulls(page);
    await page.keyboard.type("a");
    await page.keyboard.press("Shift+Tab");
    await expect(page.locator(".sc.on")).toHaveText(/^All/);
    for (const name of ["Failing", "Ready", "Drafts", "Drafts"]) {
      await page.keyboard.press("Tab");
      await expect(page.locator(".sc.on")).toHaveText(new RegExp(`^${name}`));
    }
    await page.keyboard.press("Shift+Tab");
    await expect(page.locator(".sc.on")).toHaveText(/^Ready/);
    await expect(page.locator("#chip")).toHaveText("Pull Requests");
    await expect(page.locator("#q")).toHaveValue("a");
    expect(await sent(page, "pull_requests_context")).toEqual([]);
  });

  test("Ask AI About These in ⌘K asks about the view, and Esc comes back to the same tab, filter and selection", async ({
    page,
  }) => {
    await openPulls(page, "&only=fernhill/beacon");
    await page.keyboard.type("a");
    await page.keyboard.press("ArrowDown");
    const picked = await page.locator(".pr.selected .name").textContent();
    await askAboutThese(page);
    await expect(page.locator("#chip")).toHaveText("Ask AI");
    await expect(page.locator(".cx")).toHaveText("Open pull requests in beacon matching “a”");
    await expect(page.locator(".cx .ico.gh")).toBeVisible();
    await expect(page.locator(".ai-empty")).toBeVisible();
    const urls = await page.evaluate(
      () => window.sent.find((call) => call.command === "pull_requests_context")?.args?.["urls"],
    );
    expect(urls).toHaveLength(5);
    expect(await sent(page, "pull_requests_context")).toEqual([
      {
        command: "pull_requests_context",
        args: { tab: "all", only: "fernhill/beacon", query: "a", urls },
      },
    ]);
    await page.keyboard.press("Escape");
    await expect(page.locator("#chip")).toHaveText("Pull Requests");
    await expect(page.locator(".fchip")).toHaveText("beacon×");
    await expect(page.locator("#q")).toHaveValue("a");
    await expect(page.locator(".sc.on")).toHaveText("All5");
    await expect(page.locator(".pr.selected .name")).toHaveText(picked ?? "");
  });

  test("the question goes with the pull requests attached", async ({ page }) => {
    await openPulls(page, "&tab=failing");
    await askAboutThese(page);
    await expect(page.locator(".cx")).toHaveText("Failing pull requests");
    expect(await sent(page, "pull_requests_context")).toEqual([
      { command: "pull_requests_context", args: { tab: "failing", only: null, query: "" } },
    ]);
    await page.keyboard.type("what is stuck?");
    await page.keyboard.press("Enter");
    await expect
      .poll(async () => (await sent(page, "ask")).map((call) => call.args?.["context"]))
      .toEqual([
        [
          expect.objectContaining({
            kind: "pulls",
            label: "Failing pull requests",
            text: expect.stringContaining("fernhill/beacon #169"),
          }),
        ],
      ]);
  });

  test("⌘K offers no Ask AI while the first list is still being read", async ({ page }) => {
    await openPulls(page, "&fetched=none&refreshwaits=1");
    await expect(page.locator(".full .none")).toHaveText("Reading your pull requests through gh…");
    await page.keyboard.press("Meta+k");
    await expect(page.locator(".act", { hasText: "Ask AI About These" })).toHaveCount(0);
    await page.keyboard.press("Escape");
    await expect(page.locator("#chip")).toHaveText("Pull Requests");
    expect(await sent(page, "pull_requests_context")).toEqual([]);
  });

  test("sends the typed text and only the pull requests on screen", async ({ page }) => {
    await openPulls(page, "&tab=failing");
    await page.keyboard.type("claims-gate");
    await expect(rows(page)).toHaveText(["#168"]);
    await askAboutThese(page);
    await expect(page.locator(".cx")).toHaveText("Failing pull requests matching “claims-gate”");
    expect(await sent(page, "pull_requests_context")).toEqual([
      {
        command: "pull_requests_context",
        args: {
          tab: "failing",
          only: null,
          query: "claims-gate",
          urls: ["https://github.com/fernhill/beacon/pull/168"],
        },
      },
    ]);
  });
});

test.describe("before the list", () => {
  test("the first time asks to continue, and Enter allows it", async ({ page }) => {
    await openPulls(page, "&allowed=0");
    await expect(page.locator(".empty .et")).toHaveText("Pull Requests");
    await expect(page.locator(".account .login")).toHaveText("larsx2");
    await expect(page.locator(".account .host")).toHaveText("Signed in to gh on github.com");
    await expect(page.locator(".account img.face")).toHaveAttribute(
      "src",
      /^data:image\/png;base64,/,
    );
    await expect(page.locator(".empty .ep")).toHaveText("Uses gh, never your token.");
    await expect(page.locator("#primary")).toContainText("Continue");
    await expect(page.locator("#notNow")).toHaveText("Not now esc");
    expect(await sent(page, "pull_requests_avatar")).toEqual([{ command: "pull_requests_avatar" }]);
    await page.keyboard.press("Enter");
    await expect
      .poll(() => sent(page, "pull_requests_allow"))
      .toEqual([{ command: "pull_requests_allow" }]);
    await expect(page.locator(".pr")).toHaveCount(13);
  });

  test("the first time, with no avatar yet, shows the login's first letter", async ({ page }) => {
    await openPulls(page, "&allowed=0&avatar=none");
    await expect(page.locator(".account .login")).toHaveText("larsx2");
    await expect(page.locator(".account .face.letter")).toHaveText("L");
    await expect(page.locator(".account img")).toHaveCount(0);
  });

  test("the first time, when gh cannot say who is signed in, it says why", async ({ page }) => {
    await openPulls(page, "&allowed=0&problem=offline&nologin=1");
    await expect(page.locator(".account")).toHaveCount(0);
    await expect(page.locator(".empty .ep")).toHaveText([
      "Raiden could not reach GitHub.",
      "Uses gh, never your token.",
    ]);
    expect(await sent(page, "pull_requests_avatar")).toEqual([]);
  });

  test("the first time, Not now leaves without allowing", async ({ page }) => {
    await openPulls(page, "&allowed=0");
    await page.locator("#notNow").click();
    await expect(page.locator("#chip")).toBeHidden();
    expect(await sent(page, "pull_requests_allow")).toEqual([]);
  });

  test("signed out of gh, Enter copies gh auth login", async ({ page }) => {
    await openPulls(page, "&problem=signedOut&fetched=none");
    await expect(page.locator(".empty .et")).toHaveText("Sign in to GitHub with gh.");
    await page.keyboard.press("Enter");
    await expect
      .poll(() => sent(page, "copy_text"))
      .toEqual([{ command: "copy_text", args: { text: "gh auth login" } }]);
    await expect(page.locator(".empty .et")).toHaveText("Sign in to GitHub with gh.");
  });

  test("after signing in to gh, opening it again shows the list", async ({ page }) => {
    await openPulls(page, "&problem=signedOut&fetched=none&commands=1");
    await expect(page.locator(".empty .et")).toHaveText("Sign in to GitHub with gh.");
    await page.evaluate(() => window.signInToGh?.());
    await page.keyboard.press("Escape");
    await expect(page.locator("#chip")).toBeHidden();
    await page.keyboard.type("pull");
    await expect(page.locator(".row.selected .title")).toHaveText("Pull Requests");
    await page.keyboard.press("Enter");
    await expect(page.locator(".pr")).toHaveCount(13);
    await expect(page.locator(".empty")).toHaveCount(0);
    await expect
      .poll(() => sent(page, "pull_requests_refresh"))
      .toContainEqual({ command: "pull_requests_refresh", args: { force: true } });
  });

  test("on the sign-in screen, ⌘R tries again and shows the list once signed in", async ({
    page,
  }) => {
    await openPulls(page, "&problem=signedOut&fetched=none");
    await expect(page.locator(".empty .et")).toHaveText("Sign in to GitHub with gh.");
    await page.evaluate(() => window.signInToGh?.());
    await page.keyboard.press("Meta+r");
    await expect(page.locator(".pr")).toHaveCount(13);
    await expect
      .poll(() => sent(page, "pull_requests_refresh"))
      .toContainEqual({ command: "pull_requests_refresh", args: { force: true } });
  });

  test("when the saved list cannot be read, it says why, and ⌘R reads it again", async ({
    page,
  }) => {
    await openPulls(page, "&readfails=1");
    await expect(page.locator(".full .none")).toHaveText(
      "Raiden could not read the saved pull requests.⌘R tries again.",
    );
    await page.keyboard.press("Meta+r");
    await expect(page.locator(".pr")).toHaveCount(13);
    await expect(page.locator(".full")).toHaveCount(0);
  });

  test("offline, the saved list shows with when it is from", async ({ page }) => {
    await openPulls(page, "&problem=offline");
    await expect(page.locator(".pr")).toHaveCount(13);
    await expect(page.locator("#ctx")).toHaveText("Offline. Showing the list from 14:59.");
  });

  test("at GitHub's limit, the saved list shows with when it tries again", async ({ page }) => {
    await openPulls(page, "&problem=limited");
    await expect(page.locator(".pr")).toHaveCount(13);
    await expect(page.locator("#ctx")).toHaveText("GitHub limit, retrying at 15:05.");
  });
});

test.describe("other views from it", () => {
  const fromSearch = async (page: Page) => {
    await page.clock.setFixedTime(new Date(SCENE_TIME));
    await page.goto("/scene.html?fixture=apps&commands=1");
    await page.waitForLoadState("load");
    await page.locator(".row.selected").waitFor();
    await page.keyboard.type("pull");
    await expect(page.locator(".row.selected .title")).toHaveText("Pull Requests");
    await page.keyboard.press("Enter");
    await expect(page.locator(".pr")).toHaveCount(13);
  };

  test("⌘, opens Settings in its place, and Esc goes back to the search, as from Clipboard history", async ({
    page,
  }) => {
    await fromSearch(page);
    await page.keyboard.press("Meta+Comma");
    await expect(page.locator("#chip")).toHaveText("Settings");
    await expect(page.locator(".pr")).toHaveCount(0);
    await page.keyboard.press("Escape");
    await expect(page.locator("#chip")).toBeHidden();
    await expect(page.locator("#q")).toHaveValue("pull");
    await expect(page.locator(".row.selected .title")).toHaveText("Pull Requests");
  });

  test("the Clipboard history hotkey opens it in its place, and Esc goes back to the search", async ({
    page,
  }) => {
    await fromSearch(page);
    await page.evaluate(() => window.openClipboard?.(true));
    await expect(page.locator("#chip")).toHaveText("Clipboard History");
    await expect(page.locator(".pr")).toHaveCount(0);
    expect(await sent(page, "hide")).toEqual([]);
    await page.keyboard.press("Escape");
    await expect(page.locator("#chip")).toBeHidden();
    await expect(page.locator("#q")).toHaveValue("pull");
  });
});

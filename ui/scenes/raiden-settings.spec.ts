import { expect, test, type Page } from "@playwright/test";
import { SPOTLIGHT_REFUSAL } from "./scenes";

const sent = (page: Page, command: string) =>
  page.evaluate((name) => window.sent.filter((call) => call.command === name), command);

const open = async (page: Page, params = "") => {
  await page.goto(`/scene.html?fixture=apps&commands=1${params}`);
  await page.waitForLoadState("networkidle");
  await page.locator(".row.selected").waitFor();
};

const openSettings = async (page: Page, params = "") => {
  await open(page, params);
  await page.keyboard.press("Meta+Comma");
  await expect(page.locator(".set").first()).toBeVisible();
};

const row = (page: Page, id: string) => page.locator(`.set[data-row="${id}"]`);

test.describe("getting there", () => {
  test("typing sett finds Settings under Raiden, and Enter opens it", async ({ page }) => {
    await open(page);
    await page.keyboard.type("sett");
    await expect(page.locator(".row.selected .title")).toHaveText("Settings");
    await expect(page.locator(".row.selected .sub")).toHaveText("Raiden");
    await expect(page.locator(".row.selected .acc")).toHaveText("⌘ ,");
    await expect(page.locator("#ctx")).toHaveText("Commands");
    await expect(page.locator("#primary")).toContainText("Open Settings");
    await expect(page.locator(".row", { hasText: "Open Settings File" })).toBeVisible();
    await page.keyboard.press("Enter");
    await expect(page.locator("#chip")).toHaveText("Settings");
    await expect(page.locator("#q")).toHaveAttribute("placeholder", "Search settings");
    await expect(page.locator("#q")).toHaveValue("");
    await expect
      .poll(() => sent(page, "settings_view"))
      .toEqual([{ command: "settings_view", args: { query: "sett", row: 0 } }]);
  });

  test("Open Settings File opens raiden.toml from search", async ({ page }) => {
    await open(page);
    await page.keyboard.type("settings file");
    await expect(page.locator(".row.selected .title")).toHaveText("Open Settings File");
    await expect(page.locator(".row.selected .sub")).toHaveText("raiden.toml");
    await page.keyboard.press("Enter");
    await expect
      .poll(() => sent(page, "open_settings_file"))
      .toEqual([{ command: "open_settings_file", args: { query: "settings file", row: 0 } }]);
  });

  test("⌘, opens Settings from search, and Esc goes back to the search", async ({ page }) => {
    await open(page);
    await page.keyboard.type("saf");
    await page.keyboard.press("Meta+Comma");
    await expect(page.locator("#chip")).toHaveText("Settings");
    await expect(page.locator(".list:not(.prefs)")).toBeHidden();
    await page.keyboard.type("hot");
    await page.keyboard.press("Escape");
    await expect(page.locator("#q")).toHaveValue("");
    await expect(page.locator("#chip")).toHaveText("Settings");
    await page.keyboard.press("Escape");
    await expect(page.locator("#chip")).toBeHidden();
    await expect(page.locator("#q")).toHaveValue("saf");
    await expect(page.locator(".row.selected")).toHaveText("Safari");
  });

  test("Esc back to the search keeps the row that was selected", async ({ page }) => {
    await open(page);
    await page.keyboard.press("ArrowDown");
    await page.keyboard.press("ArrowDown");
    const picked = await page.locator(".row.selected .title").textContent();
    await page.keyboard.press("Meta+Comma");
    await page.keyboard.press("Escape");
    await expect(page.locator(".row.selected .title")).toHaveText(picked ?? "");
    expect(await sent(page, "hide")).toEqual([]);
  });

  test("⌘, opens Settings from Ask AI, and Esc goes back to Ask AI", async ({ page }) => {
    await open(page);
    await page.keyboard.press("Tab");
    await expect(page.locator("#chip")).toHaveText("Ask AI");
    await page.keyboard.press("Meta+Comma");
    await expect(page.locator("#chip")).toHaveText("Settings");
    await expect(page.locator("#ai")).toBeHidden();
    await page.keyboard.press("Escape");
    await expect(page.locator("#chip")).toHaveText("Ask AI");
    await expect(page.locator("#ai")).toBeVisible();
  });

  test("Ask AI's shortcuts do nothing while Settings covers it", async ({ page }) => {
    await page.goto("/scene.html?fixture=answer&moment=done");
    await page.locator('#ai[data-phase="done"]').waitFor();
    await page.keyboard.press("Meta+Comma");
    for (const keys of ["Meta+n", "Meta+y", "Meta+Shift+c", "Meta+Period"]) {
      await page.keyboard.press(keys);
    }
    await expect(page.locator("#actions")).toBeHidden();
    await expect(page.locator("#chip")).toHaveText("Settings");
    expect(await sent(page, "copy_text")).toEqual([]);
    await page.keyboard.press("Escape");
    await expect(page.locator('#ai[data-phase="done"]')).toBeVisible();
  });

  test("an engine chosen in Settings is the one Ask AI asks next", async ({ page }) => {
    await open(page);
    await page.keyboard.press("Tab");
    await page.keyboard.press("Meta+Comma");
    await page.keyboard.type("answers");
    await page.keyboard.press("Enter");
    await expect(row(page, "ai").locator("button.on")).toHaveText("ChatGPT");
    await page.keyboard.press("Escape");
    await page.keyboard.press("Escape");
    await expect(page.locator("#chip")).toHaveText("Ask AI");
    await expect(page.locator("#ctx")).toContainText("ChatGPT");
    await page.keyboard.type("hello");
    await page.keyboard.press("Enter");
    await expect
      .poll(async () => (await sent(page, "ask")).map((call) => call.args?.["id"]))
      .toEqual(["chatgpt"]);
  });

  test("the back button leaves Settings", async ({ page }) => {
    await openSettings(page);
    await page.locator("#back").click();
    await expect(page.locator("#chip")).toBeHidden();
    await expect(page.locator("#q")).toHaveAttribute("placeholder", "Search apps and commands");
  });
});

test.describe("the rows", () => {
  test("are grouped, with their values", async ({ page }) => {
    await openSettings(page);
    await expect(page.locator(".prefs .sec, .set .name")).toHaveText([
      "General",
      "Hotkey",
      "Clipboard history",
      "Open at login",
      "Terminal",
      "Vim keys",
      "Ask AI",
      "Answers come from",
      "Effort",
      "Fix grammar",
      "File",
      "Settings file",
    ]);
    await expect(row(page, "hotkey").locator("kbd")).toHaveText(["⌥", "Space"]);
    await expect(row(page, "clipboard").locator("kbd")).toHaveText(["⌃", "⌥", "V"]);
    await expect(row(page, "login").locator(".switch")).toHaveClass(/\bon\b/);
    await expect(row(page, "terminal").locator(".value")).toHaveText("Ghostty");
    await expect(row(page, "terminal").locator(".value img")).toHaveAttribute(
      "src",
      "/scenes/icons/ghostty.png",
    );
    await expect(row(page, "vim").locator(".switch")).not.toHaveClass(/\bon\b/);
    await expect(row(page, "ai").locator("button.on")).toHaveText("Claude");
    await expect(row(page, "effort").locator(".value")).toHaveText("Low");
    await expect(row(page, "grammar").locator(".value")).toHaveText("Claude");
    await expect(row(page, "file").locator(".path")).toHaveText("~/.config/raiden/raiden.toml");
    await expect(page.locator("#ctx")).toHaveText("Saved to raiden.toml as you change things");
  });

  test("the arrows move and stop at the ends, and the footer shows what Enter does", async ({
    page,
  }) => {
    await openSettings(page);
    const footer = page.locator("#primary");
    await expect(page.locator(".set.selected .name")).toHaveText("Hotkey");
    await expect(footer).toContainText("Change hotkey");
    await page.keyboard.press("ArrowDown");
    await expect(footer).toContainText("Change Clipboard history hotkey");
    await page.keyboard.press("ArrowDown");
    await expect(footer).toContainText("Turn off Open at login");
    await page.keyboard.press("ArrowDown");
    await expect(footer).toContainText("Choose terminal");
    await page.keyboard.press("ArrowDown");
    await expect(footer).toContainText("Turn on Vim keys");
    await page.keyboard.press("ArrowDown");
    await expect(footer).toContainText("Use ChatGPT");
    await page.keyboard.press("ArrowDown");
    await expect(footer).toContainText("Change effort to Medium");
    await page.keyboard.press("ArrowDown");
    await expect(page.locator(".set.selected .name")).toHaveText("Fix grammar");
    await expect(footer).toBeHidden();
    await page.keyboard.press("ArrowDown");
    await expect(footer).toContainText("Open settings file");
    await page.keyboard.press("ArrowDown");
    await expect(page.locator(".set.selected .name")).toHaveText("Settings file");
    for (const _ of [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11]) {
      await page.keyboard.press("ArrowUp");
    }
    await expect(page.locator(".set.selected .name")).toHaveText("Hotkey");
  });

  test("typing filters the settings", async ({ page }) => {
    await openSettings(page);
    await page.keyboard.type("ai");
    await expect(page.locator(".prefs .sec, .set .name")).toHaveText([
      "Ask AI",
      "Answers come from",
      "Effort",
      "Fix grammar",
    ]);
    await expect(page.locator(".set.selected .name")).toHaveText("Answers come from");
    await page.keyboard.type("zz");
    await expect(page.locator(".prefs .none")).toHaveText(
      "No settings match “aizz”Press Esc to clear the search.",
    );
  });

  test("clicking a row keeps typing in the search field", async ({ page }) => {
    await openSettings(page);
    await row(page, "login").click();
    await expect(row(page, "login").locator(".switch")).not.toHaveClass(/\bon\b/);
    await expect(page.locator("#q")).toBeFocused();
    await page.keyboard.press("Escape");
    await expect(page.locator("#q")).toBeFocused();
  });

  test("Enter on Open at login turns it off and on", async ({ page }) => {
    await openSettings(page);
    await page.keyboard.press("ArrowDown");
    await page.keyboard.press("ArrowDown");
    await page.keyboard.press("Enter");
    await expect(row(page, "login").locator(".switch")).not.toHaveClass(/\bon\b/);
    await expect(page.locator("#primary")).toContainText("Turn on Open at login");
    await page.keyboard.press("Enter");
    await expect(row(page, "login").locator(".switch")).toHaveClass(/\bon\b/);
    expect(await sent(page, "set_open_at_login")).toEqual([
      { command: "set_open_at_login", args: { on: false } },
      { command: "set_open_at_login", args: { on: true } },
    ]);
  });

  test("⌘K lists the selected row's action first, and Enter runs it", async ({ page }) => {
    await openSettings(page);
    for (let step = 0; step < 5; step++) {
      await page.keyboard.press("ArrowDown");
    }
    await page.keyboard.press("Meta+k");
    await expect(page.locator("#actions .head")).toHaveText("Settings");
    await expect(page.locator(".act")).toHaveText([
      "Use ChatGPT↵",
      "Change hotkey",
      "Change Clipboard history hotkey",
      "Turn off Open at login",
      "Choose terminal",
      "Turn on Vim keys",
      "Change effort to Medium",
      "Open settings file⌘O",
      "Expand Window⌘⇧E",
    ]);
    await page.keyboard.press("Enter");
    await expect(row(page, "ai").locator("button.on")).toHaveText("ChatGPT");
    expect(await sent(page, "record_hotkey")).toEqual([]);
  });

  test("⌘O in the ⌘K menu opens the file, and ⌘, there keeps the search working", async ({
    page,
  }) => {
    await openSettings(page);
    await page.keyboard.press("ArrowUp");
    await page.keyboard.press("Meta+k");
    await page.keyboard.press("Meta+o");
    await expect(page.locator("#actions")).toBeHidden();
    await expect
      .poll(() => sent(page, "open_settings_file"))
      .toEqual([{ command: "open_settings_file" }]);
    await page.keyboard.press("Meta+k");
    await page.keyboard.press("Meta+Comma");
    await expect(page.locator("#actions")).toBeHidden();
    await page.keyboard.type("login");
    await expect(page.locator(".set .name")).toHaveText(["Open at login"]);
  });

  test("choosing ChatGPT puts its logo on the Tab hint of the main search", async ({ page }) => {
    await openSettings(page);
    await page.keyboard.press("Escape");
    const claude = (await page.locator("#hint .ico.brand").getAttribute("style")) ?? "";
    await page.keyboard.press("Meta+Comma");
    await page.keyboard.type("answers");
    await page.keyboard.press("Enter");
    await expect
      .poll(() => sent(page, "choose_ai"))
      .toEqual([{ command: "choose_ai", args: { id: "chatgpt" } }]);
    await page.keyboard.press("Escape");
    await page.keyboard.press("Escape");
    await expect(page.locator("#hint")).toHaveText("Tabasks ChatGPT");
    await expect(page.locator("#hint .ico.brand")).not.toHaveAttribute("style", claude);
    await expect(page.locator("#q")).toHaveAttribute("aria-describedby", "hint");
  });

  test("a sign-in that ends after the panel hid still switches engine", async ({ page }) => {
    await openSettings(page, "&chatgptsignedout=1&signinwaits=1");
    await page.keyboard.type("answers");
    await page.keyboard.press("Enter");
    await expect(page.locator("#ctx")).toHaveText("Finish signing in to Codex");
    await page.evaluate(() => window.dispatchEvent(new Event("blur")));
    await page.evaluate(() => window.finishSignIn?.());
    await expect
      .poll(() => sent(page, "choose_ai"))
      .toEqual([{ command: "choose_ai", args: { id: "chatgpt" } }]);
  });

  test("a sign-in does not undo an engine chosen elsewhere meanwhile", async ({ page }) => {
    await openSettings(page, "&chatgptsignedout=1&signinwaits=1");
    await page.keyboard.type("answers");
    await page.keyboard.press("Enter");
    await expect(page.locator("#ctx")).toHaveText("Finish signing in to Codex");
    await page.evaluate(() => {
      window.editSettings({ ai: "chatgpt" });
      window.finishSignIn?.();
    });
    await expect.poll(() => sent(page, "ai_accounts")).not.toHaveLength(0);
    await page.waitForTimeout(100);
    expect(await sent(page, "choose_ai")).toEqual([]);
  });

  test("going back to Ask AI keeps the answer where it was", async ({ page }) => {
    await page.goto("/scene.html?fixture=answer&moment=thread");
    await page.locator(".ai-turn").waitFor();
    await page.locator('#ai[data-phase="done"]').waitFor();
    const scrolled = () => page.locator("#ai").evaluate((view) => view.scrollTop);
    await expect.poll(scrolled).toBeGreaterThan(0);
    const before = await scrolled();
    await page.keyboard.press("Meta+Comma");
    await expect(page.locator("#ai")).toBeHidden();
    await page.keyboard.press("Escape");
    await expect(page.locator("#ai")).toBeVisible();
    expect(await scrolled()).toBe(before);
  });

  test("holding Enter acts once", async ({ page }) => {
    await openSettings(page);
    await page.keyboard.type("answers");
    await page.keyboard.down("Enter");
    for (let at = 0; at < 3; at++) {
      await page.evaluate(() =>
        document.dispatchEvent(
          new KeyboardEvent("keydown", { key: "Enter", repeat: true, bubbles: true }),
        ),
      );
    }
    await page.keyboard.up("Enter");
    await expect(row(page, "ai").locator("button.on")).toHaveText("ChatGPT");
    expect(await sent(page, "choose_ai")).toEqual([
      { command: "choose_ai", args: { id: "chatgpt" } },
    ]);
  });

  test("an answer that goes on behind Settings is followed to its end on the way back", async ({
    page,
  }) => {
    await page.goto("/scene.html?fixture=answer&moment=streaming");
    await page.locator('#ai[data-phase="streaming"]').waitFor();
    const id = await page.evaluate(
      () => window.sent.filter((call) => call.command === "ask").at(-1)?.args?.["answer"],
    );
    await page.keyboard.press("Meta+Comma");
    await page.evaluate((answer) => {
      window.emitAnswer({
        id: String(answer),
        kind: "delta",
        text: "\n\nA long part.".repeat(60),
        model: "haiku",
        hidden: {},
      });
    }, id);
    await page.waitForTimeout(100);
    await page.keyboard.press("Escape");
    await expect(page.locator("#ai")).toBeVisible();
    await expect
      .poll(() =>
        page
          .locator("#ai")
          .evaluate((view) => view.scrollHeight - view.scrollTop - view.clientHeight),
      )
      .toBeLessThan(2);
  });

  test("⌘O opens the settings file from any row", async ({ page }) => {
    await openSettings(page);
    await page.keyboard.press("Meta+o");
    await expect
      .poll(() => sent(page, "open_settings_file"))
      .toEqual([{ command: "open_settings_file" }]);
  });

  test("an edit made by hand shows the next time Settings opens", async ({ page }) => {
    await openSettings(page);
    await page.evaluate(() => window.editSettings({ hotkey: "⌃⌥Space", ai: "chatgpt" }));
    await page.keyboard.press("Escape");
    await page.keyboard.press("Meta+Comma");
    await expect(row(page, "hotkey").locator("kbd")).toHaveText(["⌃", "⌥", "Space"]);
    await expect(row(page, "ai").locator("button.on")).toHaveText("ChatGPT");
  });
});

test.describe("the engine", () => {
  test("Enter on Answers come from switches to the other one and saves it", async ({ page }) => {
    await openSettings(page);
    await page.keyboard.type("answers");
    await page.keyboard.press("Enter");
    await expect(row(page, "ai").locator("button.on")).toHaveText("ChatGPT");
    await expect(page.locator("#primary")).toContainText("Use Claude");
    expect(await sent(page, "choose_ai")).toEqual([
      { command: "choose_ai", args: { id: "chatgpt" } },
    ]);
    await page.keyboard.press("Enter");
    await expect(row(page, "ai").locator("button.on")).toHaveText("Claude");
  });

  test("a signed-out engine offers its sign-in, then switches to it", async ({ page }) => {
    await openSettings(page, "&chatgptsignedout=1&signinwaits=1");
    await page.keyboard.type("answers");
    await expect(page.locator("#primary")).toContainText("Sign in to ChatGPT");
    await expect(row(page, "ai").locator(".hint.warn")).toHaveText(
      "ChatGPT: Codex is signed out on this Mac",
    );
    await expect(page.locator("#ctx")).toHaveText(
      "Opens Codex's own sign-in. Raiden never sees your password or token",
    );
    await page.keyboard.press("Enter");
    await expect(page.locator("#ctx")).toHaveText("Finish signing in to Codex");
    expect(await sent(page, "choose_ai")).toEqual([]);
    await page.evaluate(() => window.finishSignIn?.());
    await expect(row(page, "ai").locator("button.on")).toHaveText("ChatGPT");
    expect(await sent(page, "ai_sign_in")).toEqual([
      { command: "ai_sign_in", args: { id: "chatgpt" } },
    ]);
    expect(await sent(page, "choose_ai")).toEqual([
      { command: "choose_ai", args: { id: "chatgpt" } },
    ]);
  });

  test("a chosen engine that is signed out says so, and Enter still switches", async ({ page }) => {
    await openSettings(page, "&ai=chatgpt&chatgptsignedout=1");
    await page.keyboard.type("answers");
    await expect(row(page, "ai").locator("button.on")).toHaveText("ChatGPT");
    await expect(row(page, "ai").locator(".hint.warn")).toHaveText(
      "ChatGPT: Codex is signed out on this Mac",
    );
    await expect(page.locator("#primary")).toContainText("Use Claude");
    await page.keyboard.press("Enter");
    await expect(row(page, "ai").locator("button.on")).toHaveText("Claude");
  });
});

test.describe("the effort", () => {
  test("Enter cycles Low, Medium, High and back, saving each", async ({ page }) => {
    await openSettings(page);
    await page.keyboard.type("effort");
    await expect(page.locator(".set.selected .name")).toHaveText("Effort");
    await expect(row(page, "effort").locator(".hint")).toHaveText(
      "Higher effort gives better answers but a slower start",
    );
    await expect(row(page, "grammar").locator(".hint")).toHaveText(
      "Always Claude Code at low effort for speed, web search off",
    );
    const value = row(page, "effort").locator(".value");
    const footer = page.locator("#primary");
    await expect(value).toHaveText("Low");
    await expect(footer).toContainText("Change effort to Medium");
    await page.keyboard.press("Enter");
    await expect(value).toHaveText("Medium");
    await expect(footer).toContainText("Change effort to High");
    await page.keyboard.press("Enter");
    await expect(value).toHaveText("High");
    await expect(footer).toContainText("Change effort to Low");
    await page.keyboard.press("Enter");
    await expect(value).toHaveText("Low");
    expect(await sent(page, "choose_effort")).toEqual([
      { command: "choose_effort", args: { id: "medium" } },
      { command: "choose_effort", args: { id: "high" } },
      { command: "choose_effort", args: { id: "low" } },
    ]);
  });

  test("a second Enter while the first is saving does not skip a level", async ({ page }) => {
    await openSettings(page, "&effortwaits=1");
    await page.keyboard.type("effort");
    await page.keyboard.press("Enter");
    await page.keyboard.press("Enter");
    await expect(row(page, "effort").locator(".value")).toHaveText("Medium");
    expect(await sent(page, "choose_effort")).toEqual([
      { command: "choose_effort", args: { id: "medium" } },
    ]);
  });

  test("clicking the row cycles it too", async ({ page }) => {
    await openSettings(page, "&effort=high");
    await expect(row(page, "effort").locator(".value")).toHaveText("High");
    await row(page, "effort").click();
    await expect(row(page, "effort").locator(".value")).toHaveText("Low");
    expect(await sent(page, "choose_effort")).toEqual([
      { command: "choose_effort", args: { id: "low" } },
    ]);
  });

  test("a file that cannot be written keeps the effort and says why", async ({ page }) => {
    await openSettings(page, "&choosefails=1");
    await page.keyboard.type("effort");
    await page.keyboard.press("Enter");
    await expect(row(page, "effort").locator(".hint.warn")).toHaveText(
      "raiden.toml is not valid TOML, so Raiden left it as it is",
    );
    await expect(row(page, "effort").locator(".value")).toHaveText("Low");
  });

  test("an effort edited by hand shows the next time Settings opens", async ({ page }) => {
    await openSettings(page);
    await page.evaluate(() => window.editSettings({ effort: "medium" }));
    await page.keyboard.press("Escape");
    await page.keyboard.press("Meta+Comma");
    await expect(row(page, "effort").locator(".value")).toHaveText("Medium");
  });
});

test.describe("recording the hotkey", () => {
  test.beforeEach(async ({ page }) => {
    await openSettings(page);
    await page.keyboard.press("Enter");
    await expect(row(page, "hotkey").locator(".recording")).toHaveText("Press the new keys…");
  });

  test("the next keys with a modifier are saved and shown", async ({ page }) => {
    await expect(page.locator("#ctx")).toHaveText(
      "The old hotkey keeps working until the new one is set",
    );
    await expect(page.locator("#cancel")).toContainText("Cancel");
    await expect(page.locator("#openActions")).toBeHidden();
    await page.keyboard.press("Shift");
    await page.keyboard.press("Control+Alt+k");
    await expect(row(page, "hotkey").locator("kbd")).toHaveText(["⌃", "⌥", "K"]);
    expect(await sent(page, "set_hotkey")).toEqual([
      { command: "set_hotkey", args: { hotkey: "⌃⌥K", which: "hotkey" } },
    ]);
    expect(await sent(page, "record_hotkey")).toEqual([
      { command: "record_hotkey", args: { on: true } },
    ]);
    expect(await sent(page, "hide")).toEqual([]);
  });

  test("⌘Space is refused while Spotlight holds it, and listening goes on", async ({ page }) => {
    await page.keyboard.press("Meta+Space");
    await expect(row(page, "hotkey").locator(".hint.warn")).toHaveText(
      `${SPOTLIGHT_REFUSAL} Try another.`,
    );
    await expect(row(page, "hotkey").locator(".recording")).toBeVisible();
    await page.keyboard.press("Meta+Shift+k");
    await expect(row(page, "hotkey").locator("kbd")).toHaveText(["⇧", "⌘", "K"]);
    await expect(row(page, "hotkey").locator(".hint")).toHaveText("Opens Raiden from any app");
  });

  test("keys without Control, Option or Command are refused", async ({ page }) => {
    await page.keyboard.press("k");
    await expect(row(page, "hotkey").locator(".hint.warn")).toContainText(
      "needs Control, Option or Command",
    );
    await expect(row(page, "hotkey").locator(".recording")).toBeVisible();
  });

  test("Esc cancels and keeps the hotkey, and the next Esc leaves Settings", async ({ page }) => {
    await page.keyboard.press("Meta+Space");
    await page.keyboard.press("Escape");
    await expect(row(page, "hotkey").locator("kbd")).toHaveText(["⌥", "Space"]);
    await expect(row(page, "hotkey").locator(".hint")).toHaveText("Opens Raiden from any app");
    expect(await sent(page, "record_hotkey")).toEqual([
      { command: "record_hotkey", args: { on: true } },
      { command: "record_hotkey", args: { on: false } },
    ]);
    await expect(page.locator("#chip")).toHaveText("Settings");
    await page.keyboard.press("Escape");
    await expect(page.locator("#chip")).toBeHidden();
  });

  test("a hotkey Raiden already holds is heard from the core and recorded", async ({ page }) => {
    await page.evaluate(() => window.pressHotkey?.("⌥Space"));
    await expect(row(page, "hotkey").locator(".recording")).toHaveCount(0);
    expect(await sent(page, "set_hotkey")).toEqual([
      { command: "set_hotkey", args: { hotkey: "⌥Space", which: "hotkey" } },
    ]);
  });

  test("⌘K and ⌘, are recorded, not acted on", async ({ page }) => {
    await page.keyboard.press("Meta+k");
    await expect(page.locator("#actions")).toBeHidden();
    await expect(row(page, "hotkey").locator("kbd")).toHaveText(["⌘", "K"]);
  });

  test("hiding the panel stops listening", async ({ page }) => {
    await page.evaluate(() => window.dispatchEvent(new Event("blur")));
    await expect(page.locator("#chip")).toBeHidden();
    expect(await sent(page, "record_hotkey")).toEqual([
      { command: "record_hotkey", args: { on: true } },
      { command: "record_hotkey", args: { on: false } },
    ]);
  });
});

test.describe("recording the Clipboard history hotkey", () => {
  test.beforeEach(async ({ page }) => {
    await openSettings(page);
    await page.keyboard.press("ArrowDown");
    await expect(page.locator("#primary")).toContainText("Change Clipboard history hotkey");
    await page.keyboard.press("Enter");
    await expect(row(page, "clipboard").locator(".recording")).toHaveText("Press the new keys…");
    await expect(row(page, "hotkey").locator(".recording")).toHaveCount(0);
  });

  test("the next keys are saved as the Clipboard history hotkey", async ({ page }) => {
    await page.keyboard.press("Control+Meta+v");
    await expect(row(page, "clipboard").locator("kbd")).toHaveText(["⌃", "⌘", "V"]);
    await expect(row(page, "hotkey").locator("kbd")).toHaveText(["⌥", "Space"]);
    expect(await sent(page, "set_hotkey")).toEqual([
      { command: "set_hotkey", args: { hotkey: "⌃⌘V", which: "clipboard" } },
    ]);
  });

  test("the hotkey that opens Raiden is refused with the reason", async ({ page }) => {
    await page.evaluate(() => window.pressHotkey?.("⌥Space"));
    await expect(row(page, "clipboard").locator(".hint.warn")).toHaveText(
      "⌥Space already opens Raiden. Try another.",
    );
    await expect(row(page, "clipboard").locator(".recording")).toBeVisible();
  });
});

test.describe("the terminal", () => {
  const openMenu = async (page: Page, params = "") => {
    await openSettings(page, params);
    await page.keyboard.type("terminal");
    await expect(page.locator(".set.selected .name")).toHaveText("Terminal");
    await expect(row(page, "terminal").locator(".value")).toHaveText("Ghostty");
    await expect(page.locator("#primary")).toContainText("Choose terminal");
    await page.keyboard.press("Enter");
    await expect(page.locator("#actions .head")).toHaveText("Terminal");
  };

  test("Enter lists the installed terminals with ⌘1 to ⌘5, then the missing ones", async ({
    page,
  }) => {
    await openMenu(page);
    await expect(page.locator("#actions .act")).toHaveText([
      "Ghostty⌘1",
      "iTerm2⌘2",
      "Rex⌘3",
      "Tern⌘4",
      "Terminal⌘5",
      "Kitty",
      "Alacritty",
      "Warp",
    ]);
    await expect(page.locator("#actions .mh")).toHaveText(["Not installed"]);
    await expect(page.locator("#actions .act.off")).toHaveCount(3);
    await expect(page.locator("#actions .act .check")).toHaveCount(1);
    await expect(page.locator("#actions .act", { has: page.locator(".check") })).toContainText(
      "Ghostty",
    );
    await expect(page.locator("#actions .act.sel")).toContainText("Ghostty");
  });

  test("⌘3 chooses Rex, saves it and closes the menu", async ({ page }) => {
    await openMenu(page);
    await page.keyboard.press("Meta+3");
    await expect(page.locator("#actions")).toBeHidden();
    await expect
      .poll(() => sent(page, "choose_terminal"))
      .toEqual([{ command: "choose_terminal", args: { id: "rex" } }]);
    await expect(row(page, "terminal").locator(".value")).toHaveText("Rex");
    await page.keyboard.press("Enter");
    await expect(page.locator("#actions .act", { has: page.locator(".check") })).toContainText(
      "Rex",
    );
  });

  test("the arrows stop at both ends, and Enter on a missing terminal does nothing", async ({
    page,
  }) => {
    await openMenu(page);
    await page.keyboard.press("ArrowUp");
    await expect(page.locator("#actions .act.sel")).toContainText("Ghostty");
    for (let step = 0; step < 10; step++) {
      await page.keyboard.press("ArrowDown");
    }
    await expect(page.locator("#actions .act.sel")).toHaveText("Warp");
    await page.keyboard.press("Enter");
    await page.keyboard.press("Meta+6");
    await expect(page.locator("#actions")).toBeVisible();
    await page.locator("#actions .act", { hasText: "Kitty" }).click({ force: true });
    await page.keyboard.press("Meta+Shift+t");
    await expect(page.locator("#actions")).toBeVisible();
    expect(await sent(page, "choose_terminal")).toEqual([]);
    await expect(page.locator("#actions .act.sel")).toHaveText("Kitty");
    await expect(page.locator("#actions .act.sel")).toHaveAttribute("aria-disabled", "true");
    await page.keyboard.press("ArrowUp");
    await expect(page.locator("#actions .act.sel")).toContainText("Terminal");
    await page.keyboard.press("Enter");
    await expect
      .poll(() => sent(page, "choose_terminal"))
      .toEqual([{ command: "choose_terminal", args: { id: "terminal" } }]);
  });

  test("typing filters the terminals, and Esc closes the menu", async ({ page }) => {
    await openMenu(page);
    await page.keyboard.type("ter");
    await expect(page.locator("#actions .act .act-t")).toHaveText(["iTerm2", "Tern", "Terminal"]);
    await page.keyboard.press("Escape");
    await page.keyboard.press("Escape");
    await expect(page.locator("#actions")).toBeHidden();
    await expect(page.locator("#chip")).toHaveText("Settings");
    expect(await sent(page, "choose_terminal")).toEqual([]);
  });

  test("a click chooses an installed terminal", async ({ page }) => {
    await openMenu(page);
    await page.locator("#actions .act", { hasText: "Tern" }).first().click();
    await expect
      .poll(() => sent(page, "choose_terminal"))
      .toEqual([{ command: "choose_terminal", args: { id: "tern" } }]);
  });
});

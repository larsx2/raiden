import { expect, test, type Page } from "@playwright/test";
import { SCENE_TIME } from "./scenes";

const FIRST_CLIP = "git worktree add .cache/launcher -b launcher origin/main";
const LAST_CLIP = "NSWindowCollectionBehavior.fullScreenAuxiliary";

const openHistory = async (page: Page, vim: string) => {
  await page.clock.setFixedTime(new Date(SCENE_TIME));
  await page.goto(`/scene.html?fixture=clipboard${vim}`);
  await page.waitForLoadState("networkidle");
  await expect(page.locator(".clip").first()).toBeVisible();
};

test.describe("⌃D and ⌃U", () => {
  test("page through Clipboard history with Vim keys on, stopping at the ends", async ({
    page,
  }) => {
    await openHistory(page, "&vim=1");
    await page.keyboard.press("Control+d");
    await expect(page.locator(".clip.selected .name")).toHaveText(LAST_CLIP);
    await page.keyboard.press("Control+d");
    await expect(page.locator(".clip.selected .name")).toHaveText(LAST_CLIP);
    await page.keyboard.press("Control+u");
    await expect(page.locator(".clip.selected .name")).toHaveText(FIRST_CLIP);
    await expect(page.locator("#q")).toHaveValue("");
  });

  test("leave Clipboard history alone with Vim keys off", async ({ page }) => {
    await openHistory(page, "");
    await page.keyboard.press("Control+d");
    await expect(page.locator(".clip.selected .name")).toHaveText(FIRST_CLIP);
  });
});

const answerScroll = (page: Page) =>
  page.evaluate(() => {
    const answer = document.querySelector<HTMLElement>("#ai");
    return answer
      ? { top: answer.scrollTop, height: answer.clientHeight, end: answer.scrollHeight }
      : null;
  });

const openLongAnswer = async (page: Page, vim: string) => {
  await page.goto(`/scene.html?fixture=answer&moment=done&long=6${vim}`);
  await page.waitForLoadState("networkidle");
  await page.locator(`#ai[data-phase="done"]`).waitFor();
  await page.evaluate(() => {
    const answer = document.querySelector<HTMLElement>("#ai");
    if (answer) {
      answer.scrollTop = 0;
    }
  });
  await expect.poll(async () => (await answerScroll(page))?.top).toBe(0);
};

test.describe("scrolling the answer from the keyboard", () => {
  test("Page Down and Page Up move it by 80% of its visible height, Vim keys or not", async ({
    page,
  }) => {
    await openLongAnswer(page, "");
    const before = await answerScroll(page);
    expect(before && before.end > before.height * 3).toBe(true);
    await page.keyboard.press("PageDown");
    const step = (before?.height ?? 0) * 0.8;
    await expect
      .poll(async () => Math.abs(((await answerScroll(page))?.top ?? 0) - step))
      .toBeLessThanOrEqual(2);
    await page.keyboard.press("PageDown");
    await expect
      .poll(async () => Math.abs(((await answerScroll(page))?.top ?? 0) - step * 2))
      .toBeLessThanOrEqual(2);
    await page.keyboard.press("PageUp");
    await page.keyboard.press("PageUp");
    await expect.poll(async () => (await answerScroll(page))?.top).toBe(0);
  });

  test("⌃D and ⌃U move it by half its visible height with Vim keys on", async ({ page }) => {
    await openLongAnswer(page, "&vim=1");
    const before = await answerScroll(page);
    await page.keyboard.press("Control+d");
    await expect
      .poll(async () =>
        Math.abs(((await answerScroll(page))?.top ?? 0) - (before?.height ?? 0) / 2),
      )
      .toBeLessThanOrEqual(2);
    await page.keyboard.press("Control+u");
    await expect.poll(async () => (await answerScroll(page))?.top).toBe(0);
    await expect(page.locator("#q")).toHaveValue("");
  });
});

const sent = (page: Page, command: string) =>
  page.evaluate((name) => window.sent.filter((call) => call.command === name), command);

test.describe("the Vim keys row in Settings", () => {
  test("typing vim finds it, and Enter turns it on and off at once", async ({ page }) => {
    await page.goto("/scene.html?fixture=settings");
    await page.waitForLoadState("networkidle");
    await page.locator(".set.selected").waitFor();
    await page.keyboard.type("vim");
    await expect(page.locator(".set .name")).toHaveText(["Vim keys"]);
    await expect(page.locator("#primary")).toContainText("Turn on Vim keys");
    const toggle = page.locator('.set[data-row="vim"] .switch');
    await expect(toggle).toHaveAttribute("aria-checked", "false");
    await page.keyboard.press("Enter");
    await expect(toggle).toHaveAttribute("aria-checked", "true");
    await expect(page.locator("#primary")).toContainText("Turn off Vim keys");
    await page.keyboard.press("Enter");
    await expect(toggle).toHaveAttribute("aria-checked", "false");
    expect(await sent(page, "set_vim_keys")).toEqual([
      { command: "set_vim_keys", args: { on: true } },
      { command: "set_vim_keys", args: { on: false } },
    ]);
  });
});

const commandsSent = (page: Page) =>
  page.evaluate(() =>
    window.sent
      .map((call) => call.command)
      .filter((command) => !["icon", "frame", "keystroke"].includes(command)),
  );

const badge = (page: Page) => page.locator("footer .nm");

const openSearch = async (page: Page, more = "") => {
  await page.goto(`/scene.html?fixture=apps${more}`);
  await page.waitForLoadState("networkidle");
  await page.locator(".row.selected").waitFor();
};

const toNormal = async (page: Page) => {
  await page.keyboard.press("Escape");
  await expect(badge(page)).toHaveText("NORMAL");
};

const typedAhead = (text: string) => [...text].map((key) => ({ key, text: key }));

test.describe("the mode badge", () => {
  test("shows INSERT while Vim keys are on, NORMAL after Esc, and nothing when off", async ({
    page,
  }) => {
    await openSearch(page);
    await expect(badge(page)).toHaveCount(0);
    await openSearch(page, "&vim=1");
    await expect(badge(page)).toHaveText("INSERT");
    await expect(badge(page)).toHaveClass(/\bins\b/);
    await page.keyboard.type("s");
    await toNormal(page);
    await expect(badge(page)).not.toHaveClass(/\bins\b/);
    await expect(page.locator("#q")).toHaveAttribute("readonly", "");
    await expect(page.locator("#q")).toHaveClass(/\bdim\b/);
    await expect(page.locator('[role="status"].said')).toHaveText(
      "Normal mode. j and k move, i types.",
    );
  });
});

test.describe("Normal in the main search", () => {
  test("Esc, j and Enter open the second row, and what was typed stays", async ({ page }) => {
    await openSearch(page, "&vim=1");
    await page.keyboard.type("s");
    const second = await page.locator(".row .title").nth(1).textContent();
    await toNormal(page);
    await page.keyboard.press("j");
    await expect(page.locator(".row.selected .title")).toHaveText(second ?? "");
    await page.keyboard.press("Enter");
    await expect.poll(async () => (await commandsSent(page)).includes("open")).toBe(true);
    await expect(page.locator("#q")).toHaveValue("s");
  });

  test("j stops at the last row, k at the first, and G and gg jump to them", async ({ page }) => {
    await openSearch(page, "&vim=1");
    await page.keyboard.type("s");
    const titles = await page.locator(".row .title").allTextContents();
    await toNormal(page);
    await page.keyboard.press("Shift+g");
    await expect(page.locator(".row.selected .title")).toHaveText(titles.at(-1) ?? "");
    await page.keyboard.press("j");
    await expect(page.locator(".row.selected .title")).toHaveText(titles.at(-1) ?? "");
    await page.keyboard.press("g");
    await page.keyboard.press("g");
    await expect(page.locator(".row.selected .title")).toHaveText(titles[0] ?? "");
    await page.keyboard.press("k");
    await expect(page.locator(".row.selected .title")).toHaveText(titles[0] ?? "");
  });

  test("i types again at the end, and / replaces what was typed", async ({ page }) => {
    await openSearch(page, "&vim=1");
    await page.keyboard.type("sa");
    await toNormal(page);
    await page.keyboard.press("i");
    await expect(badge(page)).toHaveText("INSERT");
    await page.keyboard.type("f");
    await expect(page.locator("#q")).toHaveValue("saf");
    await toNormal(page);
    await page.keyboard.press("/");
    await page.keyboard.type("m");
    await expect(page.locator("#q")).toHaveValue("m");
    await expect(page.locator('[role="status"].said')).toHaveText("Insert mode.");
  });

  test("holding i switches to typing once and types nothing", async ({ page }) => {
    await openSearch(page, "&vim=1");
    await page.keyboard.type("s");
    await toNormal(page);
    await page.keyboard.down("i");
    await page.keyboard.down("i");
    await page.keyboard.down("i");
    await page.keyboard.up("i");
    await expect(badge(page)).toHaveText("INSERT");
    await expect(page.locator("#q")).toHaveValue("s");
    await page.keyboard.type("i");
    await expect(page.locator("#q")).toHaveValue("si");
  });

  test("g then another letter does nothing, and holding g is one g", async ({ page }) => {
    await openSearch(page, "&vim=1");
    await page.keyboard.type("s");
    await toNormal(page);
    await page.keyboard.press("j");
    const second = await page.locator(".row.selected .title").textContent();
    await page.keyboard.press("g");
    await page.keyboard.press("k");
    await expect(page.locator(".row.selected .title")).toHaveText(second ?? "");
    await page.keyboard.down("g");
    await page.keyboard.down("g");
    await page.keyboard.up("g");
    await page.keyboard.press("j");
    await expect(page.locator(".row.selected .title")).toHaveText(second ?? "");
  });

  test("clicking the field returns to typing", async ({ page }) => {
    await openSearch(page, "&vim=1");
    await page.keyboard.type("s");
    await toNormal(page);
    await page.locator("#q").click();
    await expect(badge(page)).toHaveText("INSERT");
    await page.keyboard.type("a");
    await expect(page.locator("#q")).not.toHaveValue("s");
  });

  test("⌘K works in Normal, its search takes every letter, and closing it returns to Normal", async ({
    page,
  }) => {
    await openSearch(page, "&vim=1");
    await page.keyboard.type("s");
    await toNormal(page);
    await page.keyboard.press("Meta+k");
    await expect(page.locator("#actions")).toBeVisible();
    await page.keyboard.type("jk");
    await expect(page.locator("#aq")).toHaveValue("jk");
    await page.keyboard.press("Escape");
    await page.keyboard.press("Escape");
    await expect(page.locator("#actions")).toBeHidden();
    await expect(badge(page)).toHaveText("NORMAL");
    await expect(page.locator("#q")).toHaveValue("s");
  });

  test("a search with no match clears on Esc, as without Vim keys", async ({ page }) => {
    await openSearch(page, "&vim=1");
    await page.keyboard.type("zzzz");
    await page.keyboard.press("Escape");
    await expect(page.locator("#q")).toHaveValue("");
    await expect(badge(page)).toHaveText("INSERT");
  });
});

test.describe("typing right after the hotkey", () => {
  test("typed-ahead j and k land as text", async ({ page }) => {
    await openSearch(page, "&vim=1");
    await page.evaluate(
      (batches) => {
        window.typedAhead = batches;
        window.dispatchEvent(new Event("focus"));
      },
      [typedAhead("jk")],
    );
    await expect(page.locator("#q")).toHaveValue("jk");
    await expect(badge(page)).toHaveText("INSERT");
  });

  test("a typed-ahead Esc never turns the next letters into commands", async ({ page }) => {
    await openSearch(page, "&vim=1");
    await page.evaluate(
      (batches) => {
        window.typedAhead = batches;
        window.dispatchEvent(new Event("focus"));
      },
      [[...typedAhead("saf"), { key: "Escape", text: "" }]],
    );
    await expect(page.locator("#q")).toHaveValue("");
    await page.keyboard.type("jk");
    await expect(page.locator("#q")).toHaveValue("jk");
    await expect(badge(page)).toHaveText("INSERT");
  });

  test("an Esc handed back as a real key right after opening never switches mode", async ({
    page,
  }) => {
    await openSearch(page, "&vim=1&early=60000");
    await page.evaluate(() => window.dispatchEvent(new Event("focus")));
    await page.keyboard.type("saf");
    await page.keyboard.press("Escape");
    await expect(page.locator("#q")).toHaveValue("");
    await page.keyboard.type("jk");
    await expect(page.locator("#q")).toHaveValue("jk");
    await expect(badge(page)).toHaveText("INSERT");
  });

  test("once the moment after opening has passed, Esc switches to Normal", async ({ page }) => {
    await openSearch(page, "&vim=1&early=100");
    await page.evaluate(() => window.dispatchEvent(new Event("focus")));
    await page.keyboard.type("s");
    await page.waitForTimeout(150);
    await toNormal(page);
  });

  test("letters typed after ⌃⌥V land in Clipboard history's search", async ({ page }) => {
    await openSearch(page, "&vim=1&clips=1");
    await page.keyboard.type("s");
    await toNormal(page);
    await page.evaluate(
      (batches) => {
        window.typedAhead = batches;
        window.openClipboard?.(true);
      },
      [typedAhead("jk")],
    );
    await expect(page.locator("#chip")).toHaveText("Clipboard History");
    await expect(page.locator("#q")).toHaveValue("jk");
    await expect(badge(page)).toHaveText("INSERT");
  });

  test("hiding puts the panel back in Insert", async ({ page }) => {
    await openSearch(page, "&vim=1");
    await page.keyboard.type("s");
    await toNormal(page);
    await page.evaluate(() => window.dispatchEvent(new Event("blur")));
    await page.evaluate(() => window.dispatchEvent(new Event("focus")));
    await expect(badge(page)).toHaveText("INSERT");
    await page.keyboard.type("j");
    await expect(page.locator("#q")).toHaveValue(/j$/);
  });
});

const QUIET_KEYS = [..."bcdefhlmnopqrstuvwxyz0123456789-=[];',.`\\", "Space"];

const pressQuietKeys = async (page: Page, previews = false) => {
  for (const key of QUIET_KEYS.filter((quiet) => !previews || quiet !== "Space")) {
    await page.keyboard.press(key);
    await page.keyboard.press(`Shift+${key}`);
  }
};

const SCREENS: Array<{ name: string; open: (page: Page) => Promise<void>; previews?: boolean }> = [
  {
    name: "the main search",
    open: async (page) => {
      await openSearch(page, "&vim=1");
      await page.keyboard.type("s");
    },
    previews: true,
  },
  {
    name: "Clipboard history",
    open: async (page) => openHistory(page, "&vim=1"),
  },
  {
    name: "Settings",
    open: async (page) => {
      await page.goto("/scene.html?fixture=settings&vim=1");
      await page.waitForLoadState("networkidle");
      await page.locator(".set.selected").waitFor();
    },
  },
  {
    name: "Pull Requests",
    open: async (page) => {
      await page.clock.setFixedTime(new Date(SCENE_TIME));
      await page.goto("/scene.html?fixture=pulls&vim=1");
      await page.waitForLoadState("networkidle");
      await page.locator(".pr.selected").waitFor();
    },
  },
  {
    name: "Ask AI",
    open: async (page) => {
      await page.goto("/scene.html?fixture=answer&moment=done&vim=1");
      await page.waitForLoadState("networkidle");
      await page.locator(`#ai[data-phase="done"]`).waitFor();
    },
  },
];

test.describe("Normal takes no text", () => {
  for (const screen of SCREENS) {
    test(`on ${screen.name}, every other key leaves the field as it was and sends nothing`, async ({
      page,
    }) => {
      await screen.open(page);
      await toNormal(page);
      const field = await page.locator("#q").inputValue();
      const before = await commandsSent(page);
      await pressQuietKeys(page, screen.previews);
      await expect(page.locator("#q")).toHaveValue(field);
      expect(await commandsSent(page)).toEqual(before);
      await expect(badge(page)).toHaveText("NORMAL");
    });
  }
});

test("Space in Normal previews the selected app with Quick Look and closes it again", async ({
  page,
}) => {
  await SCREENS[0]?.open(page);
  await toNormal(page);
  const field = await page.locator("#q").inputValue();
  await page.keyboard.press("Space");
  await page.keyboard.press("Space");
  await expect(page.locator("#q")).toHaveValue(field);
  const looked = await page.evaluate(() =>
    window.sent.filter((call) => call.command === "quick_look").map((call) => call.args),
  );
  expect(looked).toHaveLength(1);
  expect(looked[0]?.["follow"]).toBe(false);
  expect(String(looked[0]?.["path"])).toMatch(/^\/.*\.app$/);
  await expect
    .poll(() =>
      page.evaluate(() => window.sent.filter((call) => call.command === "close_quick_look").length),
    )
    .toBe(1);
});

test("Normal mode stays Normal while Quick Look takes the keyboard and gives it back", async ({
  page,
}) => {
  await SCREENS[0]?.open(page);
  await toNormal(page);
  await page.keyboard.press("Space");
  await expect
    .poll(() =>
      page.evaluate(() => window.sent.filter((call) => call.command === "quick_look").length),
    )
    .toBe(1);
  await page.evaluate(() => {
    window.dispatchEvent(new Event("blur"));
    window.dispatchEvent(new Event("focus"));
  });
  await page.waitForTimeout(400);
  await expect(badge(page)).toHaveText("NORMAL");
});

for (const key of ["a", "i"]) {
  test(`after j moves onto a file, ${key} returns to typing and Space types a space`, async ({
    page,
  }) => {
    await page.goto("/scene.html?fixture=files&vim=1");
    await page.waitForLoadState("networkidle");
    await page.locator(".row.selected").waitFor();
    await page.keyboard.type("notes");
    await expect(page.locator(".row")).toHaveCount(5);
    await toNormal(page);
    await page.keyboard.press("j");
    await expect(page.locator(".row.selected .title")).toHaveText("notes.md");
    await page.keyboard.press(key);
    await expect(badge(page)).toHaveText("INSERT");
    await page.keyboard.press("Space");
    await expect(page.locator("#q")).toHaveValue(/ /);
    const looked = await page.evaluate(
      () => window.sent.filter((call) => call.command === "quick_look").length,
    );
    expect(looked).toBe(0);
  });
}

const pressesToHide = async (page: Page) => {
  for (let presses = 1; presses <= 8; presses++) {
    await page.keyboard.press("Escape");
    if ((await commandsSent(page)).includes("hide")) {
      return presses;
    }
  }
  return Infinity;
};

const TYPED: Array<{ name: string; url: string; ready: string; typed: string }> = [
  { name: "the main search", url: "fixture=apps", ready: ".row.selected", typed: "s" },
  { name: "Clipboard history", url: "fixture=clipboard", ready: ".clip", typed: "x" },
  { name: "Settings", url: "fixture=settings", ready: ".set.selected", typed: "e" },
  { name: "Pull Requests", url: "fixture=pulls", ready: ".pr.selected", typed: "e" },
  {
    name: "Ask AI",
    url: "fixture=answer&moment=done",
    ready: '#ai[data-phase="done"]',
    typed: "x",
  },
];

test.describe("Esc hides in one press more with Vim keys on", () => {
  for (const screen of TYPED) {
    test(`from ${screen.name} with something typed`, async ({ page }) => {
      const count = async (vim: string) => {
        await page.clock.setFixedTime(new Date(SCENE_TIME));
        await page.goto(`/scene.html?${screen.url}${vim}`);
        await page.waitForLoadState("networkidle");
        await page.locator(screen.ready).first().waitFor();
        await page.keyboard.type(screen.typed);
        return pressesToHide(page);
      };
      const off = await count("");
      const on = await count("&vim=1");
      expect(off).toBeLessThan(8);
      expect(on).toBe(off + 1);
    });
  }
});

test.describe("Normal in lists", () => {
  test("G and gg reach the oldest and newest clip, and j stops at the end", async ({ page }) => {
    await openHistory(page, "&vim=1");
    await toNormal(page);
    await page.keyboard.press("Shift+G");
    await expect(page.locator(".clip.selected .name")).toHaveText(LAST_CLIP);
    await expect(page.locator(".clip.selected")).toBeInViewport();
    await page.keyboard.press("j");
    await expect(page.locator(".clip.selected .name")).toHaveText(LAST_CLIP);
    await page.keyboard.press("g");
    await page.keyboard.press("g");
    await expect(page.locator(".clip.selected .name")).toHaveText(FIRST_CLIP);
    await page.keyboard.press("k");
    await expect(page.locator(".clip.selected .name")).toHaveText(FIRST_CLIP);
  });

  test("j and k move through Settings, and Enter on Vim keys turns them off and leaves Normal", async ({
    page,
  }) => {
    await page.goto("/scene.html?fixture=settings&vim=1");
    await page.waitForLoadState("networkidle");
    await page.locator(".set.selected").waitFor();
    await toNormal(page);
    await page.keyboard.press("j");
    await page.keyboard.press("j");
    await page.keyboard.press("j");
    await expect(page.locator(".set.selected .name")).toHaveText("Terminal");
    await page.keyboard.press("j");
    await expect(page.locator(".set.selected .name")).toHaveText("Vim keys");
    await page.keyboard.press("Enter");
    await expect(badge(page)).toHaveCount(0);
    await page.keyboard.type("j");
    await expect(page.locator("#q")).toHaveValue("j");
  });

  test("gt and gT switch Pull Requests tabs and stop at the first and last", async ({ page }) => {
    await page.clock.setFixedTime(new Date(SCENE_TIME));
    await page.goto("/scene.html?fixture=pulls&vim=1");
    await page.waitForLoadState("networkidle");
    await page.locator(".pr.selected").waitFor();
    const tab = page.locator(".sc.on");
    await toNormal(page);
    await page.keyboard.press("g");
    await page.keyboard.press("Shift+T");
    await expect(tab).toContainText("All");
    await page.keyboard.press("g");
    await page.keyboard.press("t");
    await expect(tab).toContainText("Failing");
    for (const _ of [1, 2, 3]) {
      await page.keyboard.press("g");
      await page.keyboard.press("t");
    }
    await expect(tab).toContainText("Drafts");
    await page.keyboard.press("g");
    await page.keyboard.press("Shift+T");
    await expect(tab).toContainText("Ready");
    await page.keyboard.press("j");
    await expect(page.locator(".pr").nth(1)).toHaveClass(/\bselected\b/);
  });
});

test.describe("Normal in Ask AI", () => {
  test("j and k scroll the answer, gg goes to the top and G to the bottom", async ({ page }) => {
    await openLongAnswer(page, "&vim=1");
    await toNormal(page);
    await page.keyboard.press("j");
    await expect.poll(async () => (await answerScroll(page))?.top ?? 0).toBeGreaterThanOrEqual(60);
    await page.keyboard.press("k");
    await expect.poll(async () => (await answerScroll(page))?.top).toBe(0);
    await page.keyboard.press("Shift+G");
    await expect
      .poll(async () => {
        const now = await answerScroll(page);
        return now ? now.end - now.height - now.top : -1;
      })
      .toBeLessThanOrEqual(1);
    await page.keyboard.press("g");
    await page.keyboard.press("g");
    await expect.poll(async () => (await answerScroll(page))?.top).toBe(0);
  });

  test("Esc stops a streaming answer and the mode stays as it was", async ({ page }) => {
    await page.goto("/scene.html?fixture=answer&moment=streaming&vim=1");
    await page.waitForLoadState("networkidle");
    await page.locator(`#ai[data-phase="streaming"]`).waitFor();
    await page.keyboard.press("Escape");
    await expect.poll(async () => (await commandsSent(page)).includes("ai_stop")).toBe(true);
    await expect(badge(page)).toHaveText("INSERT");
  });

  test("going back from an answer starts the main search in Insert", async ({ page }) => {
    await page.goto("/scene.html?fixture=answer&moment=done&vim=1");
    await page.waitForLoadState("networkidle");
    await page.locator(`#ai[data-phase="done"]`).waitFor();
    await toNormal(page);
    await page.keyboard.press("Escape");
    await expect(page.locator("#chip")).toBeHidden();
    await expect(badge(page)).toHaveText("INSERT");
  });
});

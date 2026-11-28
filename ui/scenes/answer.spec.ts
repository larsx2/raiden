import { expect, test, type Page } from "@playwright/test";
import recorded from "./answer.json" with { type: "json" };

const sent = (page: Page, command: string) =>
  page.evaluate((name) => window.sent.filter((call) => call.command === name), command);

const fullAnswer = recorded.events.deltas.join("");
const plainAnswer = [
  "Run `just land` from your worktree. It rebases your commits on main and pushes them, and the push hook runs `just ci` on exactly what is pushed.",
  "git fetch origin main\njust land",
  "If another change landed first, it:",
  "- rebases and tries again by itself\n- stops after the same reason three times, so read its message first",
  "See just's manual (https://just.systems/man/en/) for more.",
].join("\n\n");
const plainFollowUp = recorded.followUp.deltas
  .join("")
  .replace(/\n```python\n/, "\n")
  .replace(/\n```$/, "");
const windowTag = { kind: "window", label: "AGENTS.md in Zed", text: "AGENTS.md in Zed" };

const openScene = async (page: Page, query: string) => {
  await page.goto(`/scene.html?fixture=answer${query}`);
  await page.waitForLoadState("load");
};

test("Tab turns what is typed into a question, with the front window attached", async ({
  page,
}) => {
  await openScene(page, "");
  await page.locator(".row.selected").waitFor();
  await page.keyboard.type("why does it fail");
  await expect(page.locator("#hint")).toHaveText("Tabasks Claude");
  await expect(page.locator("#hint .ico.brand")).toBeVisible();
  await page.keyboard.press("Tab");
  await expect
    .poll(() => sent(page, "ask"))
    .toEqual([
      {
        command: "ask",
        args: {
          id: "claude",
          question: "why does it fail",
          context: [windowTag],
          conversation: expect.any(String),
          answer: expect.any(String),
        },
      },
    ]);
  await expect(page.locator("#q")).toBeFocused();
  await expect(page.locator("#q")).toHaveValue("");
  await expect(page.locator(".ai-q")).toHaveText("why does it fail");
  await expect(page.locator("#ai")).toHaveAttribute("data-phase", "done");
  await expect(page.locator(".cx")).toHaveText(["AGENTS.md in Zed", "1 API key hidden"]);
  await expect(page.locator("#ctx")).toHaveText("Claude Haiku, on your Claude subscription");
  await expect
    .poll(() => sent(page, "save_turn"))
    .toEqual([
      {
        command: "save_turn",
        args: {
          conversation: expect.any(String),
          question: "why does it fail",
          model: "haiku",
          answer: fullAnswer,
        },
      },
    ]);
});

test("Tab with nothing typed opens Ask AI waiting for a question, and Enter asks it", async ({
  page,
}) => {
  await openScene(page, "");
  await page.locator(".row.selected").waitFor();
  await expect(page.locator("#hint")).toHaveText("Tabasks Claude");
  await expect(page.locator("#hint .ico.brand")).toBeVisible();
  await page.keyboard.press("Tab");
  await expect(page.locator("#ai")).toHaveAttribute("data-phase", "waiting");
  await expect(page.locator("#q")).toBeFocused();
  await expect(page.locator("#q")).toHaveAttribute("placeholder", "Ask a follow-up");
  await expect(page.locator("#hint")).toHaveCount(0);
  await expect(page.locator(".cx")).toHaveText(["AGENTS.md in Zed"]);
  await expect(page.locator(".ai-empty")).toContainText(
    "Ask anything about what you are working on.",
  );
  await expect(page.locator(".ai-q")).toHaveCount(0);
  await expect(page.locator(".ai-a")).toHaveCount(0);
  await expect(page.locator(".caret")).toHaveCount(0);
  await expect(page.locator("header.loading")).toHaveCount(0);
  await expect(page.locator("#primary")).toHaveCount(0);
  await page.keyboard.press("Enter");
  await page.keyboard.press("Meta+k");
  await expect(page.locator(".act")).toHaveText([
    "New conversation⌘N",
    "Conversation history⌘Y",
    "Use ChatGPT",
    "Expand Window⌘⇧E",
  ]);
  await page.keyboard.press("Escape");
  await expect(page.locator("#actions")).toHaveCount(0);
  expect(await sent(page, "ask")).toEqual([]);
  await page.keyboard.type("why does it fail");
  await expect(page.locator("#primary")).toContainText("Ask");
  await page.keyboard.press("Enter");
  await expect
    .poll(() => sent(page, "ask"))
    .toEqual([
      {
        command: "ask",
        args: {
          id: "claude",
          question: "why does it fail",
          context: [windowTag],
          conversation: expect.any(String),
          answer: expect.any(String),
        },
      },
    ]);
  await expect(page.locator(".ai-q")).toHaveText("why does it fail");
  await expect(page.locator("#ai")).toHaveAttribute("data-phase", "done");
});

test("a question asked before the front window is read waits for it and goes with it", async ({
  page,
}) => {
  await openScene(page, "&frontwaits=1");
  await page.locator(".row.selected").waitFor();
  await page.keyboard.press("Tab");
  await expect(page.locator("#ai")).toHaveAttribute("data-phase", "waiting");
  await page.keyboard.type("why does it fail");
  await page.keyboard.press("Enter");
  await expect(page.locator(".ai-q")).toHaveText("why does it fail");
  await expect(page.locator("#ai")).toHaveAttribute("data-phase", "streaming");
  expect(await sent(page, "ask")).toEqual([]);
  await page.evaluate(() => window.finishFront?.());
  await expect
    .poll(() => sent(page, "ask"))
    .toEqual([
      {
        command: "ask",
        args: {
          id: "claude",
          question: "why does it fail",
          context: [windowTag],
          conversation: expect.any(String),
          answer: expect.any(String),
        },
      },
    ]);
  await expect(page.locator("#ai")).toHaveAttribute("data-phase", "done");
});

test("Esc or Backspace leaves Ask AI before a question, sending nothing", async ({ page }) => {
  await openScene(page, "");
  await page.locator(".row.selected").waitFor();
  await page.keyboard.press("Tab");
  await expect(page.locator(".cx")).toHaveText(["AGENTS.md in Zed"]);
  await page.keyboard.press("Escape");
  await expect(page.locator("#ai")).toHaveCount(0);
  await expect(page.locator("#q")).toHaveAttribute("placeholder", "Search apps and commands");
  await expect(page.locator("#hint")).toHaveText("Tabasks Claude");
  await expect(page.locator("#hint .ico.brand")).toBeVisible();
  expect(await sent(page, "hide")).toEqual([]);
  await page.keyboard.press("Tab");
  await expect(page.locator(".cx")).toHaveText(["AGENTS.md in Zed"]);
  await page.keyboard.press("Backspace");
  await expect(page.locator(".cx")).toHaveCount(0);
  await expect(page.locator(".ai-empty")).toContainText(
    "Nothing from your screen goes with your question.",
  );
  await page.keyboard.press("Backspace");
  await expect(page.locator("#ai")).toHaveCount(0);
  expect(await sent(page, "ask")).toEqual([]);
  expect(await sent(page, "ai_stop")).toEqual([]);
});

test("the Ask AI row under the results asks what was typed", async ({ page }) => {
  await openScene(page, "");
  await page.locator(".row.selected").waitFor();
  await page.keyboard.type("saf");
  await expect(page.locator(".sec")).toHaveText(["Results", "Use “saf” with"]);
  await page.keyboard.press("ArrowDown");
  await expect(page.locator("#primary")).toContainText("Ask AI");
  await page.keyboard.press("Enter");
  await expect
    .poll(() => sent(page, "ask"))
    .toEqual([
      {
        command: "ask",
        args: {
          id: "claude",
          question: "saf",
          context: [windowTag],
          conversation: expect.any(String),
          answer: expect.any(String),
        },
      },
    ]);
});

test("⌘. stops a streaming answer and keeps what arrived", async ({ page }) => {
  await openScene(page, "&moment=streaming");
  await expect(page.locator("#ai")).toHaveAttribute("data-phase", "streaming");
  await expect(page.locator("#primary")).toContainText("Stop");
  const [asked] = await sent(page, "ask");
  const id = String(asked?.args?.["answer"]);
  await page.keyboard.press("Meta+.");
  await expect.poll(() => sent(page, "ai_stop")).toEqual([{ command: "ai_stop", args: { id } }]);
  await expect(page.locator(".muted")).toHaveText("Stopped.");
  await expect(page.locator(".caret")).toHaveCount(0);
  await expect(page.locator(".ai-a p").first()).toContainText("It rebases your commits on main");
  await page.evaluate(
    (stoppedId) =>
      window.emitAnswer({ id: stoppedId, kind: "delta", text: " late", model: "", hidden: {} }),
    id,
  );
  await expect(page.locator(".ai-a")).not.toContainText("late");
});

test("Esc stops the answer, then goes back to the search, then hides", async ({ page }) => {
  await openScene(page, "&moment=streaming");
  await expect(page.locator("#ai")).toHaveAttribute("data-phase", "streaming");
  await page.keyboard.press("Escape");
  await expect.poll(() => sent(page, "ai_stop")).toHaveLength(1);
  await expect(page.locator("#ai")).toHaveAttribute("data-phase", "stopped");
  await page.keyboard.press("Escape");
  await expect(page.locator("#ai")).toHaveCount(0);
  await expect(page.locator("#q")).toHaveAttribute("placeholder", "Search apps and commands");
  expect(await sent(page, "hide")).toEqual([]);
  await page.keyboard.press("Escape");
  await expect.poll(() => sent(page, "hide")).toEqual([{ command: "hide" }]);
});

test("typing a follow-up while the answer streams is not held up", async ({ page }) => {
  await openScene(page, "&moment=streaming&hold=1");
  await expect(page.locator("#ai")).toHaveAttribute("data-phase", "streaming");
  await page.keyboard.type("and then?");
  await expect(page.locator("#q")).toHaveValue("and then?");
  await expect(page.locator("#primary")).toContainText("Ask");
});

test("a follow-up goes on the same conversation and shows under the first answer", async ({
  page,
}) => {
  await openScene(page, "&moment=done&selection=SELECT%201");
  await expect(page.locator("#ai")).toHaveAttribute("data-phase", "done");
  const first = await sent(page, "ask");
  expect(first[0]?.args?.["context"]).toEqual([
    windowTag,
    { kind: "selection", label: "Selection from Zed", text: "SELECT 1" },
  ]);
  await expect(page.locator(".cx:not(.shield)")).toHaveText([
    "AGENTS.md in Zed",
    "Selection from Zed",
  ]);
  await expect(page.locator(".cx-x")).toHaveCount(0);
  await page.keyboard.type(recorded.followUp.question);
  await page.keyboard.press("Enter");
  await expect.poll(() => sent(page, "ask")).toHaveLength(2);
  const asked = await sent(page, "ask");
  expect(asked[1]?.args).toEqual({
    id: "claude",
    conversation: asked[0]?.args?.["conversation"],
    question: recorded.followUp.question,
    context: first[0]?.args?.["context"],
    answer: expect.any(String),
  });
  expect(asked[1]?.args?.["answer"]).not.toBe(asked[0]?.args?.["answer"]);
  await expect(page.locator(".ai-q")).toHaveText([recorded.question, recorded.followUp.question]);
  await expect(page.locator(".ai-turn .ai-a")).toContainText("tries again by itself");
  await expect(page.locator(".ai-a").last()).toContainText("installs it in your worktree");
  await expect(page.locator("#ai")).toHaveAttribute("data-phase", "done");
  await expect.poll(() => sent(page, "save_turn")).toHaveLength(2);
  const kept = await sent(page, "save_turn");
  expect(kept.map((call) => call.args?.["conversation"])).toEqual([
    asked[0]?.args?.["conversation"],
    asked[0]?.args?.["conversation"],
  ]);
  expect(await sent(page, "ai_end")).toEqual([]);
  await expect(page.locator("#primary")).toContainText("Paste into Zed");
  await page.keyboard.press("Meta+Shift+c");
  await expect
    .poll(() => sent(page, "copy_text"))
    .toEqual([{ command: "copy_text", args: { text: plainFollowUp } }]);
});

test("leaving Ask AI ends the conversation, and coming back starts a new one", async ({ page }) => {
  await openScene(page, "&moment=done");
  await expect(page.locator("#ai")).toHaveAttribute("data-phase", "done");
  const conversation = (await sent(page, "ask"))[0]?.args?.["conversation"];
  await page.keyboard.press("Escape");
  await expect(page.locator("#ai")).toHaveCount(0);
  await expect
    .poll(() => sent(page, "ai_end"))
    .toEqual([{ command: "ai_end", args: { conversation } }]);
  await page.keyboard.type("and then?");
  await page.keyboard.press("Tab");
  await expect.poll(() => sent(page, "ask")).toHaveLength(2);
  const asked = await sent(page, "ask");
  expect(asked[1]?.args?.["conversation"]).not.toBe(conversation);
  expect(asked[1]?.args?.["context"]).toEqual([windowTag]);
  await expect(page.locator(".ai-q")).toHaveText(["and then?"]);
  await expect(page.locator(".ai-turn")).toHaveCount(0);
});

test("⌘N starts a new conversation from the thread", async ({ page }) => {
  await openScene(page, "&moment=done");
  await expect(page.locator("#ai")).toHaveAttribute("data-phase", "done");
  const conversation = (await sent(page, "ask"))[0]?.args?.["conversation"];
  await page.keyboard.press("Meta+n");
  await expect(page.locator("#ai")).toHaveAttribute("data-phase", "waiting");
  await expect(page.locator(".ai-q")).toHaveCount(0);
  await expect(page.locator(".cx")).toHaveText(["AGENTS.md in Zed"]);
  await expect
    .poll(() => sent(page, "ai_end"))
    .toEqual([{ command: "ai_end", args: { conversation } }]);
  await page.keyboard.type("why does it fail");
  await page.keyboard.press("Enter");
  await expect.poll(() => sent(page, "ask")).toHaveLength(2);
  const asked = await sent(page, "ask");
  expect(asked[1]?.args?.["conversation"]).not.toBe(conversation);
  expect(asked[1]?.args?.["context"]).toEqual([windowTag]);
});

test("⌘K Conversation history lists past conversations and opens one to go on with", async ({
  page,
}) => {
  await openScene(page, "&history=1");
  await page.locator(".row.selected").waitFor();
  await page.keyboard.press("Tab");
  await expect(page.locator("#ai")).toHaveAttribute("data-phase", "waiting");
  await page.keyboard.press("Meta+k");
  await page.keyboard.type("history");
  await expect(page.locator(".act")).toHaveText(["Conversation history⌘Y"]);
  await page.keyboard.press("Enter");
  await expect(page.locator("#actions .head")).toHaveText("Conversation history");
  await expect(page.locator("#aq")).toBeFocused();
  await expect(page.locator(".act .act-t")).toHaveText([
    "Summarize #22",
    "What does just sync do?",
  ]);
  await expect(page.locator(".act .sub").first()).toHaveText("Yesterday");
  await page.keyboard.type("sync");
  await expect(page.locator(".act .act-t")).toHaveText(["What does just sync do?"]);
  await page.keyboard.press("Escape");
  await page.keyboard.type("summ");
  await page.keyboard.press("Enter");
  await expect(page.locator("#actions")).toHaveCount(0);
  await expect(page.locator("#ai")).toHaveAttribute("data-phase", "done");
  await expect(page.locator(".ai-q")).toHaveText(["Summarize #22", "Which files does it touch?"]);
  await expect(page.locator(".ai-turn .ai-a")).toHaveText("About Summarize #22");
  await expect(page.locator(".ai-a").last()).toHaveText("About Which files does it touch?");
  await expect(page.locator("#q")).toBeFocused();
  expect(await sent(page, "ask")).toEqual([]);
  await page.keyboard.type("and the tests?");
  await page.keyboard.press("Enter");
  await expect
    .poll(() => sent(page, "ask"))
    .toEqual([
      {
        command: "ask",
        args: {
          id: "claude",
          conversation: "history-0",
          question: "and the tests?",
          context: [],
          answer: expect.any(String),
        },
      },
    ]);
  await expect(page.locator(".ai-q")).toHaveText([
    "Summarize #22",
    "Which files does it touch?",
    "and the tests?",
  ]);
  await expect(page.locator("#ai")).toHaveAttribute("data-phase", "done");
  await page.keyboard.press("Meta+y");
  await expect(page.locator(".act .act-t")).toHaveText(["What does just sync do?"]);
});

test("a Recent answers row opens its whole conversation", async ({ page }) => {
  await openScene(page, "&history=1");
  await page.locator(".row.selected").waitFor();
  await page.keyboard.type("files touch");
  await expect(page.locator(".row.selected")).toContainText("Summarize #22");
  await page.keyboard.press("Enter");
  await expect(page.locator("#ai")).toHaveAttribute("data-phase", "done");
  await expect(page.locator(".ai-q")).toHaveText(["Summarize #22", "Which files does it touch?"]);
});

test("Esc in Conversation history closes the menu and keeps the thread", async ({ page }) => {
  await openScene(page, "&moment=done&history=1");
  await expect(page.locator("#ai")).toHaveAttribute("data-phase", "done");
  await page.keyboard.press("Meta+y");
  await expect(page.locator("#actions .head")).toHaveText("Conversation history");
  await page.keyboard.press("Escape");
  await expect(page.locator("#actions")).toHaveCount(0);
  await expect(page.locator(".ai-q")).toHaveText([recorded.question]);
  expect(await sent(page, "ai_end")).toEqual([]);
});

test("a tag removed with its button before the question is not sent", async ({ page }) => {
  await openScene(page, "&selection=SELECT%201");
  await page.locator(".row.selected").waitFor();
  await page.keyboard.press("Tab");
  await expect(page.locator(".cx")).toHaveText(["AGENTS.md in Zed", "Selection from Zed"]);
  await page.locator(".cx-x").last().click();
  await expect(page.locator(".cx")).toHaveText(["AGENTS.md in Zed"]);
  await expect(page.locator("#q")).toBeFocused();
  await page.keyboard.type("why does it fail");
  await page.keyboard.press("Enter");
  await expect.poll(() => sent(page, "ask")).toHaveLength(1);
  expect((await sent(page, "ask"))[0]?.args?.["context"]).toEqual([windowTag]);
});

test("a follow-up after a stopped answer keeps it in the thread and saves it", async ({ page }) => {
  await openScene(page, "&moment=streaming");
  await expect(page.locator("#ai")).toHaveAttribute("data-phase", "streaming");
  await page.keyboard.press("Meta+.");
  await expect(page.locator("#ai")).toHaveAttribute("data-phase", "stopped");
  await page.keyboard.type("and then?");
  await page.keyboard.press("Enter");
  await expect.poll(() => sent(page, "save_turn")).toHaveLength(1);
  const [kept] = await sent(page, "save_turn");
  const [asked] = await sent(page, "ask");
  expect(kept?.args?.["conversation"]).toBe(asked?.args?.["conversation"]);
  expect(kept?.args?.["question"]).toBe(recorded.question);
  expect(String(kept?.args?.["answer"])).toContain("It rebases");
  await expect(page.locator(".ai-turn .muted")).toHaveText("Stopped.");
});

test("Backspace on an empty field keeps the conversation once a question is sent; Esc leaves", async ({
  page,
}) => {
  await openScene(page, "&moment=done");
  await expect(page.locator("#ai")).toHaveAttribute("data-phase", "done");
  await page.keyboard.type("a");
  await page.keyboard.press("Backspace");
  await page.keyboard.press("Backspace");
  await expect(page.locator("#ai")).toBeVisible();
  await expect(page.locator(".ai-q")).toHaveText([recorded.question]);
  await page.keyboard.press("Escape");
  await expect(page.locator("#ai")).toHaveCount(0);
});

test("a signed-out Claude Code asks you to sign in, and Enter signs in and asks", async ({
  page,
}) => {
  await openScene(page, "&signedout=1");
  await page.locator(".row.selected").waitFor();
  await page.keyboard.type("why");
  await page.keyboard.press("Tab");
  await expect(page.locator(".ai-empty")).toContainText("Sign in to Claude Code to ask");
  await expect(page.locator("#primary")).toContainText("Sign in");
  expect(await sent(page, "ask")).toEqual([]);
  await page.keyboard.press("Enter");
  await expect
    .poll(() => sent(page, "ai_sign_in"))
    .toEqual([{ command: "ai_sign_in", args: { id: "claude" } }]);
  await expect
    .poll(() => sent(page, "ask"))
    .toEqual([
      {
        command: "ask",
        args: {
          id: "claude",
          question: "why",
          context: [windowTag],
          conversation: expect.any(String),
          answer: expect.any(String),
        },
      },
    ]);
  await expect(page.locator("#ai")).toHaveAttribute("data-phase", "done");
});

test("Enter after an answer pastes it into the app you came from", async ({ page }) => {
  await openScene(page, "&moment=done");
  await expect(page.locator("#primary")).toContainText("Paste into Zed");
  await page.keyboard.press("Enter");
  await expect
    .poll(() => sent(page, "paste_back"))
    .toEqual([{ command: "paste_back", args: { text: plainAnswer } }]);
  await page.evaluate(() => window.dispatchEvent(new Event("blur")));
  await expect(page.locator("#ai")).toHaveCount(0);
  await expect(page.locator("#q")).toHaveValue("");
});

test("⌘⇧C copies the answer and the actions menu offers it", async ({ page }) => {
  await openScene(page, "&moment=done");
  await expect(page.locator("#ai")).toHaveAttribute("data-phase", "done");
  await page.keyboard.press("Meta+Shift+c");
  await expect
    .poll(() => sent(page, "copy_text"))
    .toEqual([{ command: "copy_text", args: { text: plainAnswer } }]);
  await page.keyboard.press("Meta+k");
  await expect(page.locator(".act")).toHaveText([
    "Paste into Zed↵",
    "Copy answer⌘⇧C",
    "New conversation⌘N",
    "Conversation history⌘Y",
    "Use ChatGPT",
    "Expand Window⌘⇧E",
  ]);
});

test("the whole answer that comes with done replaces the pieces streamed before it", async ({
  page,
}) => {
  await openScene(page, "&moment=streaming");
  await expect(page.locator("#ai")).toHaveAttribute("data-phase", "streaming");
  const [asked] = await sent(page, "ask");
  await page.evaluate(
    (id) => window.emitAnswer({ id, kind: "done", text: "All of it.", model: "", hidden: {} }),
    String(asked?.args?.["answer"]),
  );
  await expect(page.locator("#ai")).toHaveAttribute("data-phase", "done");
  await expect(page.locator(".ai-a")).toHaveText("All of it.");
  await expect
    .poll(() => sent(page, "save_turn"))
    .toEqual([
      {
        command: "save_turn",
        args: {
          conversation: expect.any(String),
          question: recorded.question,
          model: "haiku",
          answer: "All of it.",
        },
      },
    ]);
});

test("an answer is found again by two of its words after the panel reopens", async ({ page }) => {
  await openScene(page, "&moment=done");
  await expect(page.locator("#ai")).toHaveAttribute("data-phase", "done");
  await expect.poll(() => sent(page, "save_turn")).toHaveLength(1);
  await page.evaluate(() => window.dispatchEvent(new Event("blur")));
  await page.evaluate(() => window.dispatchEvent(new Event("focus")));
  await page.keyboard.type("land change");
  await expect(page.locator(".row").first()).toContainText(recorded.question);
  await expect(page.locator(".row.selected")).toContainText(recorded.question);
  await page.keyboard.press("Enter");
  await expect(page.locator("#ai")).toHaveAttribute("data-phase", "done");
  await expect(page.locator(".ai-a")).toContainText("tries again by itself");
});

const askedId = async (page: Page, at = 0) => {
  const asked = await sent(page, "ask");
  return String(asked[at]?.args?.["answer"]);
};

test("going back from a streaming answer stops it", async ({ page }) => {
  await openScene(page, "&moment=streaming");
  await expect(page.locator("#ai")).toHaveAttribute("data-phase", "streaming");
  const id = await askedId(page);
  await page.locator("#back").click();
  await expect(page.locator("#ai")).toHaveCount(0);
  await expect.poll(() => sent(page, "ai_stop")).toEqual([{ command: "ai_stop", args: { id } }]);
});

test("hiding the panel while an answer streams stops it", async ({ page }) => {
  await openScene(page, "&moment=streaming");
  await expect(page.locator("#ai")).toHaveAttribute("data-phase", "streaming");
  const id = await askedId(page);
  await page.evaluate(() => window.dispatchEvent(new Event("blur")));
  await expect.poll(() => sent(page, "ai_stop")).toEqual([{ command: "ai_stop", args: { id } }]);
});

test("a new question while one streams stops the first", async ({ page }) => {
  await openScene(page, "&moment=streaming");
  await expect(page.locator("#ai")).toHaveAttribute("data-phase", "streaming");
  const id = await askedId(page);
  await page.keyboard.type("and then?");
  await page.keyboard.press("Enter");
  await expect.poll(() => sent(page, "ask")).toHaveLength(2);
  expect(await sent(page, "ai_stop")).toEqual([{ command: "ai_stop", args: { id } }]);
  const asked = await sent(page, "ask");
  expect(asked[1]?.args?.["conversation"]).toBe(asked[0]?.args?.["conversation"]);
  await expect(page.locator(".ai-q")).toHaveText([recorded.question, "and then?"]);
  await expect(page.locator(".ai-turn .muted")).toHaveText("Stopped.");
});

test("removing a tag while the answer streams asks again without it", async ({ page }) => {
  await openScene(page, "&moment=streaming&selection=SELECT%201");
  await expect(page.locator("#ai")).toHaveAttribute("data-phase", "streaming");
  const id = await askedId(page);
  await page.keyboard.press("Backspace");
  await expect.poll(() => sent(page, "ask")).toHaveLength(2);
  expect(await sent(page, "ai_stop")).toEqual([{ command: "ai_stop", args: { id } }]);
  const asked = await sent(page, "ask");
  expect(asked[1]?.args).toEqual({
    id: "claude",
    question: recorded.question,
    context: [windowTag],
    conversation: expect.any(String),
    answer: expect.any(String),
  });
  expect(asked[1]?.args?.["conversation"]).not.toBe(asked[0]?.args?.["conversation"]);
  await expect
    .poll(() => sent(page, "ai_end"))
    .toEqual([{ command: "ai_end", args: { conversation: asked[0]?.args?.["conversation"] } }]);
  await expect(page.locator(".cx:not(.shield)")).toHaveText(["AGENTS.md in Zed"]);
  await expect(page.locator(".ai-q")).toHaveText(recorded.question);
  await expect(page.locator("#ai")).toHaveAttribute("data-phase", "streaming");
});

test("a paste back that fails keeps the answer up and says why", async ({ page }) => {
  const reason = "Paste back skipped: you switched to Safari.";
  await openScene(page, `&moment=done&pastefails=${encodeURIComponent(reason)}`);
  await expect(page.locator("#ai")).toHaveAttribute("data-phase", "done");
  await page.keyboard.press("Enter");
  await page.evaluate(() => window.dispatchEvent(new Event("blur")));
  await page.evaluate(() => window.dispatchEvent(new Event("focus")));
  await expect(page.locator(".ai-a .muted")).toHaveText(reason);
  await expect(page.locator(".ai-a")).toContainText("tries again by itself");
  expect(await sent(page, "paste_back")).toHaveLength(1);
});

test("typing stays cheap while an answer streams in fast", { tag: "@speed" }, async ({ page }) => {
  await openScene(page, "&moment=streaming");
  await expect(page.locator("#ai")).toHaveAttribute("data-phase", "streaming");
  const id = await askedId(page);
  await page.evaluate((streaming) => {
    let sentPieces = 0;
    const pump = () => {
      for (let burst = 0; burst < 10; burst++) {
        const text = sentPieces % 10 === 9 ? "Next part.\n\n" : "some more `words` ";
        window.emitAnswer({ id: streaming, kind: "delta", text, model: "haiku", hidden: {} });
        sentPieces++;
      }
      Object.assign(window, { streamedPieces: sentPieces });
      if (sentPieces < 6000) {
        setTimeout(pump, 2);
      }
    };
    pump();
  }, id);
  const before = (await sent(page, "keystroke")).length;
  await page.evaluate(() => {
    const seen: number[] = [];
    window.addEventListener("input", (event) => seen.push(performance.now() - event.timeStamp));
    Object.assign(window, { seen });
  });
  const typed = "does typing keep up while it streams";
  await page.keyboard.type(typed, { delay: 5 });
  await expect(page.locator("#q")).toHaveValue(typed);
  const handled = (await sent(page, "keystroke"))
    .slice(before)
    .map((call) => Number(call.args?.["ms"]));
  const waited = await page.evaluate(() => (window as unknown as { seen: number[] }).seen);
  const slowest = (times: number[]) =>
    times.toSorted((a, b) => a - b)[Math.ceil(times.length * 0.95) - 1] ?? Infinity;
  expect(handled).toHaveLength(typed.length);
  expect(waited).toHaveLength(typed.length);
  expect(slowest(handled)).toBeLessThan(8);
  expect(slowest(waited)).toBeLessThan(8);
  const streamed = await page.evaluate(
    () => (window as unknown as { streamedPieces: number }).streamedPieces,
  );
  expect(streamed).toBeGreaterThan(100);
  expect(streamed).toBeLessThan(6000);
  await expect(page.locator(".ai-a")).toContainText("Next part.");
});

test("a sign-in that finishes after the panel was left asks nothing", async ({ page }) => {
  await openScene(page, "&signedout=1&signinwaits=1");
  await page.locator(".row.selected").waitFor();
  await page.keyboard.type("why");
  await page.keyboard.press("Tab");
  await expect(page.locator("#primary")).toContainText("Sign in");
  await page.keyboard.press("Enter");
  await expect.poll(() => sent(page, "ai_sign_in")).toHaveLength(1);
  await page.evaluate(() => window.dispatchEvent(new Event("blur")));
  await expect(page.locator("#ai")).toHaveCount(0);
  await page.evaluate(() => window.finishSignIn?.());
  await expect.poll(() => sent(page, "ai_accounts")).toHaveLength(2);
  await page.waitForTimeout(100);
  expect(await sent(page, "ask")).toEqual([]);
  expect(await sent(page, "save_turn")).toEqual([]);
  await expect(page.locator("#ai")).toHaveCount(0);
});

test("an answer left before it is done is not kept", async ({ page }) => {
  await openScene(page, "&moment=streaming");
  await expect(page.locator("#ai")).toHaveAttribute("data-phase", "streaming");
  const id = await askedId(page);
  await page.locator("#back").click();
  await page.evaluate(
    (left) => window.emitAnswer({ id: left, kind: "done", text: "Late.", model: "", hidden: {} }),
    id,
  );
  await page.waitForTimeout(100);
  expect(await sent(page, "save_turn")).toEqual([]);
});

test("a stopped answer is not kept", async ({ page }) => {
  await openScene(page, "&moment=streaming");
  await expect(page.locator("#ai")).toHaveAttribute("data-phase", "streaming");
  const id = await askedId(page);
  await page.keyboard.press("Meta+.");
  await expect(page.locator("#ai")).toHaveAttribute("data-phase", "stopped");
  await page.evaluate(
    (left) => window.emitAnswer({ id: left, kind: "done", text: "Late.", model: "", hidden: {} }),
    id,
  );
  await page.waitForTimeout(100);
  expect(await sent(page, "save_turn")).toEqual([]);
});

test("the loading line never makes the window scroll sideways", async ({ page }) => {
  await openScene(page, "&moment=streaming&hold=1");
  await expect(page.locator("header.loading")).toHaveCount(1);
  const widest = await page.evaluate(async () => {
    const overflow: number[] = [];
    const started = performance.now();
    while (performance.now() - started < 1000) {
      await new Promise(requestAnimationFrame);
      for (const element of [
        document.documentElement,
        document.body,
        document.querySelector("#app"),
      ]) {
        if (element) {
          overflow.push(element.scrollWidth - element.clientWidth);
        }
      }
    }
    return Math.max(...overflow);
  });
  expect(widest).toBeLessThanOrEqual(0);
});

test("scrolling up while an answer streams stays put, and the bottom is followed again after", async ({
  page,
}) => {
  await openScene(page, "&moment=streaming");
  await expect(page.locator("#ai")).toHaveAttribute("data-phase", "streaming");
  const id = await askedId(page);
  const more = (text: string) =>
    page.evaluate(
      ([answer, piece]) =>
        window.emitAnswer({
          id: String(answer),
          kind: "delta",
          text: String(piece),
          model: "haiku",
          hidden: {},
        }),
      [id, text],
    );
  await more("\n\nA long part.".repeat(40));
  await expect(page.locator(".ai-a p")).toHaveCount(41);
  const bottom = () =>
    page.locator("#ai").evaluate((view) => view.scrollHeight - view.scrollTop - view.clientHeight);
  await expect.poll(bottom).toBeLessThan(2);
  await page.locator("#ai").evaluate((view) => {
    view.scrollTop = 0;
    view.dispatchEvent(new Event("scroll"));
  });
  await more("\n\nStill going.");
  await page.waitForTimeout(100);
  expect(await page.locator("#ai").evaluate((view) => view.scrollTop)).toBe(0);
  await page.locator("#ai").evaluate((view) => {
    view.scrollTop = view.scrollHeight;
    view.dispatchEvent(new Event("scroll"));
  });
  await more("\n\nThe end.".repeat(10));
  await expect.poll(bottom).toBeLessThan(2);
});

test("while the tool searches the web, the answer says what it is searching for", async ({
  page,
}) => {
  await openScene(page, "&moment=streaming&hold=1");
  await expect(page.locator("#ai")).toHaveAttribute("data-phase", "streaming");
  const [asked] = await sent(page, "ask");
  const id = String(asked?.args?.["answer"]);
  const emit = (kind: string, text: string) =>
    page.evaluate(
      ([answer, eventKind, words]) =>
        window.emitAnswer({
          id: answer,
          kind: eventKind as "delta",
          text: words,
          model: "",
          hidden: {},
        }),
      [id, kind, text],
    );
  await emit("searching", "");
  await expect(page.locator(".searching")).toHaveText("Searching the web");
  await expect(page.locator(".ai-a .caret")).toHaveCount(0);
  await emit("searching", "usd to mxn");
  await expect(page.locator(".searching")).toHaveText("Searching the web for “usd to mxn”");
  await emit("delta", " ");
  await expect(page.locator(".searching")).toHaveCount(1);
  await emit("delta", "About 18 pesos.");
  await expect(page.locator(".searching")).toHaveCount(0);
  await expect(page.locator(".ai-a")).toContainText("About 18 pesos.");
});

test("a new question from search never shows the last answer's search line", async ({ page }) => {
  await openScene(page, "&moment=streaming&hold=1&frontwaits=1");
  await page.evaluate(() => window.finishFront?.());
  await expect.poll(() => sent(page, "ask")).toHaveLength(1);
  const [asked] = await sent(page, "ask");
  const id = String(asked?.args?.["answer"]);
  await page.evaluate(
    (answer) =>
      window.emitAnswer({
        id: answer,
        kind: "searching",
        text: "usd to mxn",
        model: "",
        hidden: {},
      }),
    id,
  );
  await expect(page.locator(".searching")).toHaveCount(1);
  await page.keyboard.press("Escape");
  await page.keyboard.press("Escape");
  await expect(page.locator("#ai")).toHaveCount(0);
  await page.keyboard.type("what next");
  await page.keyboard.press("Tab");
  await expect(page.locator(".ai-q")).toHaveText("what next");
  await expect(page.locator("#ai")).toHaveAttribute("data-phase", "streaming");
  await expect(page.locator(".searching")).toHaveCount(0);
  await page.evaluate(() => window.finishFront?.());
  await expect.poll(() => sent(page, "ask")).toHaveLength(2);
});

const chooseInMenu = async (page: Page, title: string) => {
  await page.keyboard.press("Meta+k");
  await page.keyboard.type(title);
  await expect(page.locator(".act")).toHaveText([title]);
  await page.keyboard.press("Enter");
};

test("⌘K names the other AI, and choosing it saves the choice and starts a new conversation on it", async ({
  page,
}) => {
  await openScene(page, "&moment=done");
  await expect(page.locator("#ai")).toHaveAttribute("data-phase", "done");
  await expect(page.locator("#ctx")).toHaveText("Claude Haiku, on your Claude subscription");
  const conversation = (await sent(page, "ask"))[0]?.args?.["conversation"];
  const claudeLogo = (await page.locator("#chip .ico.brand").getAttribute("style")) ?? "";
  expect(claudeLogo).not.toBe("");
  await page.keyboard.type("and then?");
  await page.keyboard.press("Meta+k");
  await expect(page.locator(".act").filter({ hasText: /^Use / })).toHaveText("Use ChatGPT");
  await page.keyboard.press("Escape");
  await chooseInMenu(page, "Use ChatGPT");
  await expect
    .poll(() => sent(page, "choose_ai"))
    .toEqual([{ command: "choose_ai", args: { id: "chatgpt" } }]);
  await expect(page.locator("#ai")).toHaveAttribute("data-phase", "waiting");
  await expect(page.locator(".ai-q")).toHaveCount(0);
  await expect(page.locator("#q")).toHaveValue("and then?");
  await expect(page.locator("#ctx")).toHaveText("ChatGPT, on your ChatGPT subscription");
  await expect(page.locator("#chip .ico.brand")).not.toHaveAttribute("style", claudeLogo);
  await expect(page.locator("#chip .ico.brand")).toHaveAttribute(
    "style",
    (await page.locator("#ctx .ico.brand").getAttribute("style")) ?? "",
  );
  await expect
    .poll(() => sent(page, "ai_end"))
    .toEqual([{ command: "ai_end", args: { conversation } }]);
  await page.keyboard.press("Enter");
  await expect.poll(() => sent(page, "ask")).toHaveLength(2);
  const asked = (await sent(page, "ask"))[1]?.args;
  expect(asked?.["id"]).toBe("chatgpt");
  expect(asked?.["question"]).toBe("and then?");
  expect(asked?.["conversation"]).not.toBe(conversation);
  await expect(page.locator("#ai")).toHaveAttribute("data-phase", "done");
  await expect(page.locator("#ctx")).toHaveText("GPT-6-Luna, on your ChatGPT subscription");
  await page.keyboard.press("Meta+k");
  await expect(page.locator(".act").filter({ hasText: /^Use / })).toHaveText("Use Claude");
});

test("Ask AI answers through the AI the settings file names", async ({ page }) => {
  await openScene(page, "&ai=chatgpt");
  await page.locator(".row.selected").waitFor();
  await expect(page.locator("#hint")).toHaveText("Tabasks ChatGPT");
  await page.keyboard.type("why does it fail");
  await page.keyboard.press("Tab");
  await expect.poll(() => sent(page, "ask")).toHaveLength(1);
  expect((await sent(page, "ask"))[0]?.args?.["id"]).toBe("chatgpt");
  await expect(page.locator("#ctx")).toHaveText("GPT-6-Luna, on your ChatGPT subscription");
  await chooseInMenu(page, "Use Claude");
  await expect
    .poll(() => sent(page, "choose_ai"))
    .toEqual([{ command: "choose_ai", args: { id: "claude" } }]);
  await expect(page.locator("#ctx")).toHaveText("Claude, on your Claude subscription");
});

test("choosing a signed-out ChatGPT offers Codex's sign-in, then waits for a question", async ({
  page,
}) => {
  await openScene(page, "&moment=done&chatgptsignedout=1");
  await expect(page.locator("#ai")).toHaveAttribute("data-phase", "done");
  await chooseInMenu(page, "Use ChatGPT");
  await expect(page.locator(".ai-empty")).toContainText("Sign in to Codex to ask");
  await expect(page.locator("#primary")).toContainText("Sign in");
  await page.keyboard.press("Enter");
  await expect
    .poll(() => sent(page, "ai_sign_in"))
    .toEqual([{ command: "ai_sign_in", args: { id: "chatgpt" } }]);
  await expect(page.locator("#ai")).toHaveAttribute("data-phase", "waiting");
  expect(await sent(page, "ask")).toHaveLength(1);
});

test("a choice that cannot be saved says why and keeps the conversation", async ({ page }) => {
  await openScene(page, "&choosefails=1");
  await page.locator(".row.selected").waitFor();
  await page.keyboard.press("Tab");
  await expect(page.locator("#ai")).toHaveAttribute("data-phase", "waiting");
  await chooseInMenu(page, "Use ChatGPT");
  await expect(page.locator(".ai-empty")).toContainText(
    "raiden.toml is not valid TOML, so Raiden left it as it is",
  );
  await expect(page.locator("#ctx")).toHaveText("Claude, on your Claude subscription");
  expect(await sent(page, "choose_ai")).toEqual([
    { command: "choose_ai", args: { id: "chatgpt" } },
  ]);
  expect(await sent(page, "front_context")).toHaveLength(1);
});

test("a past ChatGPT conversation goes on through ChatGPT while Claude is the setting", async ({
  page,
}) => {
  await openScene(page, "&history=1&historymodel=gpt-6-luna");
  await page.locator(".row.selected").waitFor();
  await page.keyboard.press("Tab");
  await expect(page.locator("#ctx")).toHaveText("Claude, on your Claude subscription");
  await page.keyboard.press("Meta+y");
  await expect(page.locator("#actions .head")).toHaveText("Conversation history");
  await page.keyboard.type("summ");
  await page.keyboard.press("Enter");
  await expect(page.locator("#ai")).toHaveAttribute("data-phase", "done");
  await expect(page.locator("#ctx")).toHaveText("GPT-6-Luna, on your ChatGPT subscription");
  await page.keyboard.type("and the tests?");
  await page.keyboard.press("Enter");
  await expect.poll(() => sent(page, "ask")).toHaveLength(1);
  expect((await sent(page, "ask"))[0]?.args?.["id"]).toBe("chatgpt");
  await page.keyboard.press("Meta+k");
  await expect(page.locator(".act").filter({ hasText: /^Use / })).toHaveText("Use Claude");
});

test("choosing the other AI while an answer streams stops it and starts over", async ({ page }) => {
  await openScene(page, "&moment=streaming");
  await expect(page.locator("#ai")).toHaveAttribute("data-phase", "streaming");
  const answer = (await sent(page, "ask"))[0]?.args?.["answer"];
  await chooseInMenu(page, "Use ChatGPT");
  await expect
    .poll(() => sent(page, "ai_stop"))
    .toEqual([{ command: "ai_stop", args: { id: answer } }]);
  await expect(page.locator("#ai")).toHaveAttribute("data-phase", "waiting");
  await expect(page.locator(".ai-a")).toHaveCount(0);
  await expect(page.locator("#ctx")).toHaveText("ChatGPT, on your ChatGPT subscription");
});

test("an answer shows its links, lists, bold and code as they read, not as marks", async ({
  page,
}) => {
  await openScene(page, "&moment=done");
  await expect(page.locator("#ai")).toHaveAttribute("data-phase", "done");
  const answer = page.locator(".ai-a");
  await expect(answer.locator("strong")).toHaveText("exactly");
  await expect(answer.locator("em")).toHaveText("same");
  await expect(answer.locator("p code")).toHaveText(["just land", "just ci"]);
  await expect(answer.locator("pre")).toHaveText("git fetch origin main\njust land");
  await expect(answer.locator("ul > li")).toHaveText([
    "rebases and tries again by itself",
    "stops after the same reason three times, so read its message first",
  ]);
  await expect(answer.locator("a")).toHaveText("just's manual");
  await expect(answer.locator("a")).toHaveAttribute("title", "https://just.systems/man/en/");
  await expect(answer.locator("a")).not.toHaveAttribute("href");
  await expect(answer).not.toContainText("**");
  await expect(answer).not.toContainText("](");
});

test("clicking a link in an answer asks Raiden to open it, and keeps the field focused", async ({
  page,
}) => {
  await openScene(page, "&moment=done");
  await expect(page.locator("#ai")).toHaveAttribute("data-phase", "done");
  await page.locator(".ai-a a").click();
  await expect
    .poll(() => sent(page, "open_link"))
    .toEqual([{ command: "open_link", args: { url: "https://just.systems/man/en/" } }]);
  await expect(page.locator("#q")).toBeFocused();
  expect(page.url()).toContain("/scene.html");
});

test("a link still arriving shows as plain text until it closes, and only web links open", async ({
  page,
}) => {
  await openScene(page, "&moment=streaming");
  await expect(page.locator("#ai")).toHaveAttribute("data-phase", "streaming");
  const id = await askedId(page);
  const emit = (text: string) =>
    page.evaluate(
      ([answer, delta]) =>
        window.emitAnswer({ id: answer, kind: "delta", text: delta, model: "", hidden: {} }),
      [id, text] as const,
    );
  const answer = page.locator(".ai-a");
  await emit(" **1 USD** at [Ria](https://www.ria");
  await expect(answer).toContainText("at [Ria](https://www.ria");
  await expect(answer.locator("a")).toHaveCount(0);
  await expect(answer.locator("strong")).toHaveText("1 USD");
  await emit("moneytransfer.com/). <b>Not bold</b> [run](javascript:alert(1))");
  await expect(answer.locator("a")).toHaveText("Ria");
  await expect(answer).toContainText("<b>Not bold</b> run");
  await expect(answer.locator("b")).toHaveCount(0);
  await answer.locator("a").click();
  await expect
    .poll(() => sent(page, "open_link"))
    .toEqual([{ command: "open_link", args: { url: "https://www.riamoneytransfer.com/" } }]);
});

test("a code block shows its colors by the language on its fence, and copies as plain text", async ({
  page,
}) => {
  await openScene(page, "&moment=thread");
  await expect(page.locator("#ai")).toHaveAttribute("data-phase", "done");
  const code = page.locator(".ai-a").last().locator("pre code");
  await expect(code.locator(".tk-keyword")).toHaveText(["def"]);
  await expect(code.locator(".tk-function")).toHaveText(["install_hooks"]);
  await expect(code.locator(".tk-comment")).toHaveText(["# git runs the hooks kept in the repo"]);
  await expect(code.locator(".tk-string")).toHaveText([
    '"scripts"',
    '"hooks"',
    '"git"',
    '"config"',
    '"core.hooksPath"',
  ]);
  await expect(code.locator(".tk-number")).toHaveText(["None", "30"]);
  await expect(code.locator(".tk-type")).toHaveText(["str"]);
  const colors = await code
    .locator("span[class]")
    .evaluateAll((spans) => new Set(spans.map((span) => getComputedStyle(span).color)).size);
  expect(colors).toBe(6);
  await page.keyboard.press("Meta+Shift+c");
  await expect
    .poll(() => sent(page, "copy_text"))
    .toEqual([{ command: "copy_text", args: { text: plainFollowUp } }]);
});

const LONG_CODE = Array.from(
  { length: 50 },
  (_, at) => `export function total${at}(items: Item[], rate = 0.${at % 9}): number {
  // sums the prices of part ${at}, then adds tax
  const sum = items.reduce((acc, item) => acc + item.price * item.count, 0);
  const label = \`Total ${at}: \${sum.toFixed(2)}\`;
  return Math.round(sum * (1 + rate) * 100) / 100;
}
`,
).join("");

test(
  "300 lines of code streaming in keep each update within a frame",
  { tag: "@speed" },
  async ({ page }) => {
    await page.addInitScript(() => {
      const frame = window.requestAnimationFrame.bind(window);
      const costs: number[] = [];
      const tasks: number[] = [];
      Object.assign(window, { costs, tasks });
      const Channel = window.MessageChannel;
      window.MessageChannel = class extends Channel {
        constructor() {
          super();
          const port = this.port1;
          Object.defineProperty(port, "onmessage", {
            set(handler: (event: MessageEvent) => void) {
              port.addEventListener("message", (event) => {
                const started = performance.now();
                handler(event);
                void document.body.offsetHeight;
                tasks.push(performance.now() - started);
              });
              port.start();
            },
          });
        }
      };
      window.requestAnimationFrame = (callback) =>
        frame((time) => {
          const started = performance.now();
          callback(time);
          void document.body.offsetHeight;
          costs.push(performance.now() - started);
        });
    });
    await openScene(page, "&moment=streaming");
    await expect(page.locator("#ai")).toHaveAttribute("data-phase", "streaming");
    const id = await askedId(page);
    const pieces = [
      "\n\n```ts\n",
      ...(LONG_CODE.match(/\s*\w{1,6}|\s*[^\w\s]+|\s+/g) ?? []),
      "```\n",
    ];
    expect(LONG_CODE.split("\n")).toHaveLength(301);
    await page.evaluate(
      async ({ streaming, told }) => {
        const costs = (window as unknown as { costs: number[] }).costs;
        costs.length = 0;
        const emit = (text: string) =>
          window.emitAnswer({ id: streaming, kind: "delta", text, model: "haiku", hidden: {} });
        for (let at = 0; at < told.length; at += 32) {
          told.slice(at, at + 32).forEach(emit);
          await new Promise((next) => setTimeout(next, 16));
        }
        await new Promise((next) => setTimeout(next, 50));
      },
      { streaming: id, told: pieces },
    );
    const costs = await page.evaluate(() => (window as unknown as { costs: number[] }).costs);
    const sorted = costs.toSorted((a, b) => a - b);
    const p95 = sorted[Math.ceil(sorted.length * 0.95) - 1] ?? Infinity;
    const slowest = sorted.at(-1) ?? Infinity;
    test.info().annotations.push({
      type: "speed",
      description: `code streaming: ${pieces.length} pieces, ${costs.length} updates, p95 ${p95.toFixed(1)} ms, slowest ${slowest.toFixed(1)} ms`,
    });
    await expect(page.locator(".ai-a pre code .tk-comment")).toHaveCount(50);
    await expect(page.locator(".ai-a pre code > span")).toHaveCount(8);
    const whole = await page.evaluate(async (streaming) => {
      const tasks = (window as unknown as { tasks: number[] }).tasks;
      tasks.length = 0;
      window.emitAnswer({ id: streaming, kind: "done", text: "", model: "haiku", hidden: {} });
      while (document.querySelectorAll(".ai-a pre code > span").length !== 1) {
        await new Promise((next) => setTimeout(next, 5));
      }
      return tasks.length > 0 ? Math.max(...tasks) : Infinity;
    }, id);
    test.info().annotations.push({
      type: "speed",
      description: `the block colored whole after the answer ends: ${whole.toFixed(1)} ms`,
    });
    expect(p95).toBeLessThan(8);
    expect(slowest).toBeLessThan(16.7);
    expect(whole).toBeLessThan(16.7);
  },
);

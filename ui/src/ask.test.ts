import { describe, expect, it } from "vitest";
import {
  ageLabel,
  findable,
  hiddenLabel,
  madeBy,
  matchesAnswer,
  modelLabel,
  sent,
  tagsFor,
  titleOf,
} from "./ask";

const zed = { app: "Zed", path: "/Applications/Zed.app", window: "AGENTS.md", selection: null };

describe("context tags", () => {
  it("names the front window and the selection after the app", () => {
    const tags = tagsFor({ ...zed, selection: "fn main() {}" });
    expect(tags.map((tag) => tag.label)).toEqual(["AGENTS.md in Zed", "Selection from Zed"]);
    expect(sent(tags)).toEqual([
      { kind: "window", label: "AGENTS.md in Zed", text: "AGENTS.md in Zed" },
      { kind: "selection", label: "Selection from Zed", text: "fn main() {}" },
    ]);
  });

  it("drops the app's name and what follows it from a window's title", () => {
    const chrome = {
      ...zed,
      app: "Google Chrome",
      window: "Lunges - YouTube - Google Chrome - Alex (me@example.com)",
    };
    const [tag] = tagsFor(chrome);
    expect(tag?.label).toBe("Lunges - YouTube in Google Chrome");
    expect(tag?.text).toBe(`${chrome.window} in Google Chrome`);
    expect(tagsFor({ ...zed, window: "Zed" })[0]?.label).toBe("Zed");
    expect(tagsFor({ ...zed, window: "notes — Zed" })[0]?.label).toBe("notes in Zed");
    expect(tagsFor({ ...zed, window: "notes - Zedd fans" })[0]?.label).toBe(
      "notes - Zedd fans in Zed",
    );
    expect(tagsFor({ ...zed, window: "Zed - notes" })[0]?.label).toBe("Zed - notes in Zed");
  });

  it("names the window once when its title is the app's name", () => {
    const tags = tagsFor({ ...zed, app: "WhatsApp", window: "WhatsApp" });
    expect(tags.map((tag) => tag.label)).toEqual(["WhatsApp"]);
  });

  it("has no tags without a front app or anything to show", () => {
    expect(tagsFor(null)).toEqual([]);
    expect(tagsFor({ ...zed, window: null })).toEqual([]);
  });
});

describe("hidden secrets", () => {
  it("counts what was hidden in plain words", () => {
    expect(hiddenLabel({})).toBeNull();
    expect(hiddenLabel({ openai_api_key: 1 })).toBe("1 API key hidden");
    expect(hiddenLabel({ openai_api_key: 1, stripe_secret_key: 1 })).toBe("2 API keys hidden");
    expect(hiddenLabel({ postgres_password: 1 })).toBe("1 connection string hidden");
    expect(hiddenLabel({ github_token: 2, openai_api_key: 1 })).toBe("3 secrets hidden");
  });
});

describe("model names", () => {
  it("reads model ids the way the footer shows them", () => {
    expect(modelLabel("claude-haiku-4-5-20251001", "Claude")).toBe("Claude Haiku 4.5");
    expect(modelLabel("haiku", "Claude")).toBe("Claude Haiku");
    expect(modelLabel("Claude Sonnet 5.5", "Claude")).toBe("Claude Sonnet 5.5");
    expect(modelLabel("gpt-6-luna", "ChatGPT")).toBe("GPT-6-Luna");
    expect(modelLabel("gpt-5.1-codex", "ChatGPT")).toBe("GPT-5.1-Codex");
    expect(modelLabel("", "ChatGPT")).toBe("ChatGPT");
  });

  it("tells which subscription a saved conversation ran on from its model", () => {
    expect(madeBy("haiku")).toBe("claude");
    expect(madeBy("claude-haiku-4-5-20251001")).toBe("claude");
    expect(madeBy("gpt-6-luna")).toBe("chatgpt");
    expect(madeBy("")).toBeNull();
    expect(madeBy("llama")).toBeNull();
  });
});

describe("recent answers", () => {
  const land = findable({
    id: "c",
    at: 0,
    model: "",
    turns: [{ question: "How do I land a change?", answer: "" }],
  });

  it("match when every word typed starts a word of the question", () => {
    expect(matchesAnswer(land, "land change")).toBe(true);
    expect(matchesAnswer(land, "chan")).toBe(true);
    expect(matchesAnswer(land, "land merge")).toBe(false);
    expect(matchesAnswer(land, "  ")).toBe(false);
  });

  it("match on any question of a conversation, titled by the first", () => {
    const conversation = {
      id: "c",
      at: 0,
      model: "",
      turns: [
        { question: "How do I land a change?", answer: "" },
        { question: "And when the hook fails?", answer: "" },
      ],
    };
    expect(matchesAnswer(findable(conversation), "land hook")).toBe(true);
    expect(titleOf(conversation)).toBe("How do I land a change?");
  });

  it("say how long ago they were asked", () => {
    const now = 1_000_000;
    expect(ageLabel(now - 10, now)).toBe("Just now");
    expect(ageLabel(now - 5 * 60, now)).toBe("5 min ago");
    expect(ageLabel(now - 2 * 3600, now)).toBe("2 h ago");
    expect(ageLabel(now - 26 * 3600, now)).toBe("Yesterday");
  });
});

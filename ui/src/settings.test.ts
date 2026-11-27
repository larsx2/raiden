import { describe, expect, it } from "vitest";
import type { Account } from "./ask";
import {
  SETTING_ROWS,
  effortName,
  engineStep,
  filterRows,
  groupsOf,
  hotkeyKeys,
  hotkeyText,
  keyName,
  nextEffort,
  tryAnother,
  type Pressed,
} from "./settings";

const pressed = (key: string, code: string, held: Partial<Pressed> = {}): Pressed => ({
  key,
  code,
  ctrlKey: false,
  altKey: false,
  shiftKey: false,
  metaKey: false,
  ...held,
});

const account = (id: Account["id"], signedIn: boolean): Account => ({
  id,
  name: id === "claude" ? "Claude" : "ChatGPT",
  through: id === "claude" ? "Claude Code" : "Codex",
  signedIn,
});

describe("settings rows", () => {
  it("are grouped General, Ask AI and File", () => {
    expect(
      groupsOf(SETTING_ROWS).map(([group, rows]) => [group, rows.map((row) => row.id)]),
    ).toEqual([
      ["General", ["hotkey", "clipboard", "login", "terminal", "vim"]],
      ["Ask AI", ["ai", "effort", "grammar"]],
      ["Conversions", ["currency"]],
      ["File", ["file"]],
    ]);
  });

  it("are found by the start of any word in them", () => {
    const ids = (query: string) => filterRows(SETTING_ROWS, query).map((row) => row.id);
    expect(ids("")).toEqual([
      "hotkey",
      "clipboard",
      "login",
      "terminal",
      "vim",
      "ai",
      "effort",
      "grammar",
      "currency",
      "file",
    ]);
    expect(ids("fixer")).toEqual(["currency"]);
    expect(ids("clip")).toEqual(["clipboard"]);
    expect(ids("hotkey")).toEqual(["hotkey", "clipboard"]);
    expect(ids("ai")).toEqual(["ai", "effort", "grammar"]);
    expect(ids("effort")).toEqual(["effort", "grammar"]);
    expect(ids("reasoning")).toEqual(["effort"]);
    expect(ids("chatgpt")).toEqual(["ai"]);
    expect(ids("log")).toEqual(["login"]);
    expect(ids("raiden.toml")).toEqual(["file"]);
    expect(ids("open log")).toEqual(["login"]);
    expect(ids("zz")).toEqual([]);
    for (const word of ["vim", "vi", "modal", "normal", "insert", "hjkl", "navigation"]) {
      expect(ids(word)).toEqual(["vim"]);
    }
    expect(ids("keyboard")).toEqual(["hotkey", "clipboard", "vim"]);
    for (const word of ["terminal", "ghostty", "iterm", "shell", "warp"]) {
      expect(ids(word)).toEqual(["terminal"]);
    }
  });
});

describe("effort", () => {
  it("cycles Low, Medium, High and back to Low", () => {
    expect(nextEffort("low")).toBe("medium");
    expect(nextEffort("medium")).toBe("high");
    expect(nextEffort("high")).toBe("low");
    expect(["low", "medium", "high"].map((effort) => effortName(effort as "low"))).toEqual([
      "Low",
      "Medium",
      "High",
    ]);
  });
});

describe("recording a hotkey", () => {
  it("names keys by where they sit, so Option does not change them", () => {
    expect(hotkeyText(pressed("˚", "KeyK", { ctrlKey: true, altKey: true }))).toBe("⌃⌥K");
    expect(hotkeyText(pressed(" ", "Space", { metaKey: true }))).toBe("⌘Space");
    expect(hotkeyText(pressed("!", "Digit1", { shiftKey: true, metaKey: true }))).toBe("⇧⌘1");
    expect(hotkeyText(pressed("F5", "F5", { altKey: true }))).toBe("⌥F5");
    expect(hotkeyText(pressed("k", "KeyK"))).toBe("K");
  });

  it("waits while only modifiers are down", () => {
    for (const key of ["Shift", "Control", "Alt", "Meta"]) {
      expect(hotkeyText(pressed(key, `${key}Left`, { metaKey: true }))).toBeNull();
    }
  });

  it("falls back to the key when there is no code", () => {
    expect(keyName("", "k")).toBe("K");
    expect(keyName("Comma", ",")).toBe("Comma");
  });

  it("shows a hotkey as one key cap per modifier and one for the key", () => {
    expect(hotkeyKeys("⌥Space")).toEqual(["⌥", "Space"]);
    expect(hotkeyKeys("⌃⌥⇧⌘K")).toEqual(["⌃", "⌥", "⇧", "⌘", "K"]);
    expect(hotkeyKeys("⌥")).toEqual(["⌥"]);
  });

  it("asks for another hotkey after the reason", () => {
    expect(tryAnother("⌘Space can open only Raiden, not Clipboard history")).toBe(
      "⌘Space can open only Raiden, not Clipboard history. Try another.",
    );
    expect(tryAnother("Taken.")).toBe("Taken. Try another.");
  });
});

describe("the engine row", () => {
  it("switches to the other engine when it is signed in", () => {
    expect(engineStep(account("chatgpt", true))).toEqual({
      kind: "use",
      id: "chatgpt",
      title: "Use ChatGPT",
    });
  });

  it("signs in to the other engine first when it is signed out", () => {
    expect(engineStep(account("claude", false))).toEqual({
      kind: "signIn",
      id: "claude",
      title: "Sign in to Claude",
    });
  });
});

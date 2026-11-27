import { describe, expect, it, vi } from "vitest";
import { QUICK_LOOK_KEYS, appActions, filterActions, matchesInOrder } from "./actions";

const handlers = () => ({ open: vi.fn(), showInFinder: vi.fn(), copyPath: vi.fn() });

describe("app actions", () => {
  it("opens, shows in Finder and copies the path, with their shortcuts", () => {
    const h = handlers();
    const actions = appActions({ path: "/Applications/Safari.app" }, h);
    expect(actions.map((action) => [action.title, action.keys, action.hides])).toEqual([
      ["Open", "↵", true],
      ["Show in Finder", "⌘ ↵", true],
      ["Copy path", "⌘ ⇧ C", false],
    ]);
    for (const action of actions) {
      action.run();
    }
    expect(h.open).toHaveBeenCalledOnce();
    expect(h.showInFinder).toHaveBeenCalledWith("/Applications/Safari.app");
    expect(h.copyPath).toHaveBeenCalledWith("/Applications/Safari.app");
  });

  it("adds Quick Look on ⌘Y after Show in Finder when it can preview, keeping the menu open", () => {
    const quickLook = vi.fn();
    const actions = appActions({ path: "/Users/me/notes.md" }, { ...handlers(), quickLook });
    expect(actions.map((action) => [action.title, action.keys, action.hides])).toEqual([
      ["Open", "↵", true],
      ["Show in Finder", "⌘ ↵", true],
      ["Quick Look", QUICK_LOOK_KEYS, false],
      ["Copy path", "⌘ ⇧ C", false],
    ]);
    expect(QUICK_LOOK_KEYS).toBe("⌘ Y");
    actions[2]?.run();
    expect(quickLook).toHaveBeenCalledWith("/Users/me/notes.md");
  });

  it("keeps the actions whose names hold the typed letters in order", () => {
    const actions = appActions({ path: "/Applications/Safari.app" }, handlers());
    expect(filterActions(actions, "cop").map((action) => action.title)).toEqual(["Copy path"]);
    expect(filterActions(actions, " ").map((action) => action.title)).toHaveLength(3);
    expect(filterActions(actions, "x")).toEqual([]);
  });

  it("ignores case and surrounding spaces", () => {
    expect(matchesInOrder(" SiF ", "Show in Finder")).toBe(true);
    expect(matchesInOrder("fs", "Show in Finder")).toBe(false);
  });

  it("matches on what an action is about when it says", () => {
    const past = {
      title: "Summarize #22",
      keys: "",
      hides: false,
      run: () => undefined,
      about: "Summarize #22 Which files does it touch?",
    };
    expect(filterActions([past], "files")).toEqual([past]);
  });
});

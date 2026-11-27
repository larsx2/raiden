import { describe, expect, it } from "vitest";
import { filterActions } from "./actions";
import { NOT_INSTALLED, terminalActions, type TerminalId, type TerminalRow } from "./terminals";

const ROWS: TerminalRow[] = [
  { id: "ghostty", name: "Ghostty", app: "/Applications/Ghostty.app" },
  { id: "iterm", name: "iTerm2", app: "/Applications/iTerm.app" },
  { id: "rex", name: "Rex", app: "/Applications/Rex.app" },
  { id: "tern", name: "Tern", app: "/Applications/Tern.app" },
  { id: "terminal", name: "Terminal", app: "/System/Applications/Utilities/Terminal.app" },
  { id: "kitty", name: "Kitty", app: null },
  { id: "alacritty", name: "Alacritty", app: null },
  { id: "warp", name: "Warp", app: null },
];

describe("the terminal menu", () => {
  it("numbers the installed terminals ⌘1 to ⌘5 and checks the one in use", () => {
    const actions = terminalActions(ROWS, "rex", () => undefined);
    expect(actions.map((action) => [action.title, action.keys, action.checked === true])).toEqual([
      ["Ghostty", "⌘ 1", false],
      ["iTerm2", "⌘ 2", false],
      ["Rex", "⌘ 3", true],
      ["Tern", "⌘ 4", false],
      ["Terminal", "⌘ 5", false],
      ["Kitty", "", false],
      ["Alacritty", "", false],
      ["Warp", "", false],
    ]);
    expect(actions.slice(0, 5).every((action) => !action.off && action.section === undefined)).toBe(
      true,
    );
    expect(actions.slice(0, 5).map((action) => action.app)).toEqual(
      ROWS.slice(0, 5).map((row) => row.app),
    );
  });

  it("puts what is missing under Not installed, with a bundled icon where there is one", () => {
    const missing = terminalActions(ROWS, "ghostty", () => undefined).slice(5);
    expect(missing.every((action) => action.off && action.section === NOT_INSTALLED)).toBe(true);
    expect(missing.every((action) => action.image?.endsWith(".png") === true)).toBe(true);
    const plain = terminalActions(
      ROWS.map((row) => (row.id === "rex" ? { ...row, app: null } : row)),
      "ghostty",
      () => undefined,
    ).find((action) => action.title === "Rex");
    expect(plain).toMatchObject({ off: true, image: "", section: NOT_INSTALLED });
  });

  it("chooses only installed terminals", () => {
    const chosen: TerminalId[] = [];
    for (const action of terminalActions(ROWS, "ghostty", (id) => chosen.push(id))) {
      action.run();
    }
    expect(chosen).toEqual(["ghostty", "iterm", "rex", "tern", "terminal"]);
  });

  it("filters by name like every other menu", () => {
    const actions = terminalActions(ROWS, "ghostty", () => undefined);
    expect(filterActions(actions, "ter").map((action) => action.title)).toEqual([
      "iTerm2",
      "Tern",
      "Terminal",
    ]);
  });
});

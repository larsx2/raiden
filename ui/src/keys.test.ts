import { describe, expect, it, vi } from "vitest";
import { handleKey, shortcutOf, stepsBack } from "./keys";

const actions = () => ({
  hide: vi.fn(),
  page: vi.fn(),
  move: vi.fn(),
  open: vi.fn(),
  cancel: vi.fn(),
  clear: vi.fn(),
  toggleMenu: vi.fn(),
  closeMenu: vi.fn(),
  shortcut: vi.fn(() => true),
  ask: vi.fn(),
  stop: vi.fn(),
  back: vi.fn(),
  removeTag: vi.fn(),
  normal: vi.fn(),
  insert: vi.fn(),
  wait: vi.fn(),
  line: vi.fn(),
  edge: vi.fn(),
  tab: vi.fn(),
  preview: vi.fn(() => true),
  closePreview: vi.fn(),
});
const list = { menu: false, pressing: false, search: "", held: "" };
const menu = { menu: true, pressing: false, search: "", held: "" };
const settings = { ...list, settings: true };

describe("keys in Settings", () => {
  it("Escape clears the search first, then goes back instead of hiding", () => {
    const a = actions();
    handleKey({ key: "Escape" }, { ...settings, search: "hot" }, a);
    handleKey({ key: "Escape" }, settings, a);
    expect(a.clear).toHaveBeenCalledOnce();
    expect(a.back).toHaveBeenCalledOnce();
    expect(a.hide).not.toHaveBeenCalled();
  });

  it("Backspace on an empty search goes back and steps back once", () => {
    const a = actions();
    expect(handleKey({ key: "Backspace" }, settings, a)).toBe(true);
    expect(a.back).toHaveBeenCalledOnce();
    expect(a.removeTag).not.toHaveBeenCalled();
    expect(stepsBack({ key: "Backspace" }, settings)).toBe(true);
    expect(handleKey({ key: "Backspace" }, { ...settings, search: "x" }, a)).toBe(false);
  });

  it("Tab does not start Ask AI, and ⌘, reaches the shortcuts", () => {
    const a = actions();
    handleKey({ key: "Tab" }, settings, a);
    expect(a.ask).not.toHaveBeenCalled();
    handleKey({ key: ",", metaKey: true }, settings, a);
    expect(a.shortcut).toHaveBeenCalledWith("⌘ ,");
  });
});

describe("keys in Pull Requests", () => {
  const pulls = { ...list, pulls: true };

  it("Escape clears the typed text first, then steps back instead of hiding", () => {
    const a = actions();
    handleKey({ key: "Escape" }, { ...pulls, search: "retry" }, a);
    handleKey({ key: "Escape" }, pulls, a);
    expect(a.clear).toHaveBeenCalledOnce();
    expect(a.back).toHaveBeenCalledOnce();
    expect(a.hide).not.toHaveBeenCalled();
  });

  it("Backspace on an empty filter steps back once per press, and edits typed text", () => {
    const a = actions();
    expect(handleKey({ key: "Backspace" }, pulls, a)).toBe(true);
    expect(handleKey({ key: "Backspace", repeat: true }, { ...pulls, held: "Backspace" }, a)).toBe(
      true,
    );
    expect(handleKey({ key: "Backspace" }, { ...pulls, search: "x" }, a)).toBe(false);
    expect(a.back).toHaveBeenCalledOnce();
    expect(stepsBack({ key: "Backspace" }, pulls)).toBe(true);
  });

  it("Tab and Shift Tab move between the tabs, typed or not, never from the menu, and never start Ask AI", () => {
    const a = actions();
    expect(handleKey({ key: "Tab" }, pulls, a)).toBe(true);
    expect(handleKey({ key: "Tab", shiftKey: true }, { ...pulls, search: "beacon" }, a)).toBe(true);
    expect(handleKey({ key: "Tab" }, { ...pulls, menu: true }, a)).toBe(true);
    expect(a.tab.mock.calls).toEqual([[1], [-1]]);
    expect(a.ask).not.toHaveBeenCalled();
  });

  it("⌘ keys reach the view's shortcuts", () => {
    const a = actions();
    handleKey({ key: "2", metaKey: true }, pulls, a);
    handleKey({ key: "R", metaKey: true, shiftKey: true }, pulls, a);
    handleKey({ key: "Backspace", metaKey: true }, pulls, a);
    expect(a.shortcut.mock.calls).toEqual([["⌘ 2"], ["⌘ ⇧ R"], ["⌘ ⌫"]]);
  });
});

describe("panel keys", () => {
  it("hides the panel on Escape", () => {
    const a = actions();
    expect(handleKey({ key: "Escape" }, list, a)).toBe(true);
    expect(a.hide).toHaveBeenCalledOnce();
  });

  it("pages the list with Page Down and Page Up", () => {
    const a = actions();
    handleKey({ key: "PageDown" }, list, a);
    handleKey({ key: "PageUp" }, list, a);
    expect(a.page.mock.calls).toEqual([[1], [-1]]);
  });

  it("moves the selection with the arrows and opens it with Enter", () => {
    const a = actions();
    handleKey({ key: "ArrowDown" }, list, a);
    handleKey({ key: "ArrowUp" }, list, a);
    handleKey({ key: "Enter" }, list, a);
    expect(a.move.mock.calls).toEqual([[1], [-1]]);
    expect(a.open).toHaveBeenCalledOnce();
  });

  it("moves down with ⌃J and ⌃N and up with ⌃K and ⌃P, in the list and in the menu", () => {
    for (const view of [list, menu]) {
      const a = actions();
      for (const key of ["j", "n", "k", "p"]) {
        expect(handleKey({ key, ctrlKey: true }, view, a)).toBe(true);
      }
      expect(a.move.mock.calls).toEqual([[1], [1], [-1], [-1]]);
      expect(a.shortcut).not.toHaveBeenCalled();
    }
  });

  it("leaves ⌘K and ⌃⌥K to their own meanings", () => {
    const a = actions();
    handleKey({ key: "k", metaKey: true }, list, a);
    handleKey({ key: "k", ctrlKey: true, altKey: true }, list, a);
    expect(a.move).not.toHaveBeenCalled();
    expect(a.toggleMenu).toHaveBeenCalledOnce();
  });

  it("leaves Enter alone while an input method is composing", () => {
    const a = actions();
    expect(handleKey({ key: "Enter", isComposing: true }, list, a)).toBe(false);
    expect(a.open).not.toHaveBeenCalled();
  });

  it("leaves typing alone", () => {
    const a = actions();
    expect(handleKey({ key: "a" }, list, a)).toBe(false);
    expect(handleKey({ key: "a", shiftKey: true }, menu, a)).toBe(false);
    expect(a.hide).not.toHaveBeenCalled();
  });
});

describe("actions menu keys", () => {
  it("toggles the menu with ⌘K", () => {
    const a = actions();
    expect(handleKey({ key: "k", metaKey: true }, list, a)).toBe(true);
    expect(handleKey({ key: "K", metaKey: true }, menu, a)).toBe(true);
    expect(a.toggleMenu).toHaveBeenCalledTimes(2);
  });
});

describe("going back", () => {
  it("clears the search on Escape before anything else", () => {
    const a = actions();
    handleKey({ key: "Escape" }, { ...menu, search: "x" }, a);
    handleKey({ key: "Escape" }, { ...list, search: "saf" }, a);
    expect(a.clear).toHaveBeenCalledTimes(2);
    expect(a.closeMenu).not.toHaveBeenCalled();
    expect(a.hide).not.toHaveBeenCalled();
  });

  it("closes the menu on Escape once its search is empty, instead of hiding", () => {
    const a = actions();
    handleKey({ key: "Escape" }, menu, a);
    expect(a.closeMenu).toHaveBeenCalledOnce();
    expect(a.hide).not.toHaveBeenCalled();
  });

  it("closes the menu on Backspace in its empty search", () => {
    const a = actions();
    expect(handleKey({ key: "Backspace" }, menu, a)).toBe(true);
    expect(a.closeMenu).toHaveBeenCalledOnce();
  });

  it("leaves Backspace to the field while there is text or no menu", () => {
    const a = actions();
    expect(handleKey({ key: "Backspace" }, { ...menu, search: "x" }, a)).toBe(false);
    expect(handleKey({ key: "Backspace" }, list, a)).toBe(false);
    expect(handleKey({ key: "Backspace", repeat: true }, list, a)).toBe(false);
    expect(a.closeMenu).not.toHaveBeenCalled();
    expect(a.hide).not.toHaveBeenCalled();
  });

  it("takes no step back on a held key's repeats", () => {
    const a = actions();
    expect(handleKey({ key: "Escape", repeat: true }, { ...menu, search: "x" }, a)).toBe(true);
    expect(handleKey({ key: "Escape", repeat: true }, menu, a)).toBe(true);
    expect(handleKey({ key: "Escape", repeat: true }, list, a)).toBe(true);
    expect(handleKey({ key: "Backspace", repeat: true }, menu, a)).toBe(true);
    expect(a.clear).not.toHaveBeenCalled();
    expect(a.closeMenu).not.toHaveBeenCalled();
    expect(a.hide).not.toHaveBeenCalled();
  });

  it("keeps a held Backspace that left the menu from erasing the search behind it", () => {
    const a = actions();
    const behind = { ...list, search: "saf", held: "Backspace" };
    expect(handleKey({ key: "Backspace", repeat: true }, behind, a)).toBe(true);
    expect(handleKey({ key: "Backspace" }, behind, a)).toBe(false);
  });

  it("names the presses that take a step back", () => {
    expect(stepsBack({ key: "Escape" }, list)).toBe(true);
    expect(stepsBack({ key: "Backspace" }, menu)).toBe(true);
    expect(stepsBack({ key: "Backspace" }, { ...menu, search: "x" })).toBe(false);
    expect(stepsBack({ key: "Backspace" }, list)).toBe(false);
  });

  it("moves through the actions and runs one with Enter, leaving paging to the field", () => {
    const a = actions();
    handleKey({ key: "ArrowDown" }, menu, a);
    handleKey({ key: "Enter" }, menu, a);
    expect(handleKey({ key: "PageDown" }, menu, a)).toBe(false);
    expect(a.move.mock.calls).toEqual([[1]]);
    expect(a.open).toHaveBeenCalledOnce();
    expect(a.page).not.toHaveBeenCalled();
  });

  it("runs an action's shortcut from the list and from the menu", () => {
    const a = actions();
    expect(handleKey({ key: "C", metaKey: true, shiftKey: true }, list, a)).toBe(true);
    expect(handleKey({ key: "Enter", metaKey: true }, list, a)).toBe(true);
    expect(handleKey({ key: "Enter", metaKey: true }, menu, a)).toBe(true);
    expect(handleKey({ key: "c", metaKey: true, shiftKey: true }, menu, a)).toBe(true);
    expect(a.shortcut.mock.calls).toEqual([["⌘ ⇧ C"], ["⌘ ↵"], ["⌘ ↵"], ["⌘ ⇧ C"]]);
    expect(a.open).not.toHaveBeenCalled();
  });

  it("never takes ⌘↵ for Enter, even when no action holds it", () => {
    const a = { ...actions(), shortcut: vi.fn(() => false) };
    expect(handleKey({ key: "Enter", metaKey: true }, menu, a)).toBe(false);
    expect(handleKey({ key: "a", metaKey: true }, list, a)).toBe(false);
    expect(a.open).not.toHaveBeenCalled();
  });

  it("cancels a flashing action on Escape before going back", () => {
    const a = actions();
    handleKey({ key: "Escape" }, { ...menu, pressing: true, search: "cop" }, a);
    expect(a.cancel).toHaveBeenCalledOnce();
    expect(a.clear).not.toHaveBeenCalled();
    expect(a.closeMenu).not.toHaveBeenCalled();
    expect(a.hide).not.toHaveBeenCalled();
  });

  it("names shortcuts the way the menu shows them", () => {
    expect(shortcutOf({ key: "c", metaKey: true, shiftKey: true })).toBe("⌘ ⇧ C");
    expect(shortcutOf({ key: "Enter", ctrlKey: true, altKey: true })).toBe("⌃ ⌥ ↵");
    expect(shortcutOf({ key: "Backspace", metaKey: true })).toBe("⌘ ⌫");
  });
});

describe("answer keys", () => {
  const answer = { ...list, answer: true };
  const streaming = { ...answer, streaming: true };

  it("asks AI with Tab from the search, typed or not, and keeps Tab from leaving the field", () => {
    const a = actions();
    expect(handleKey({ key: "Tab" }, { ...list, search: "why" }, a)).toBe(true);
    expect(handleKey({ key: "Tab" }, list, a)).toBe(true);
    expect(handleKey({ key: "Tab" }, { ...answer, search: "more" }, a)).toBe(true);
    expect(handleKey({ key: "Tab" }, answer, a)).toBe(true);
    expect(handleKey({ key: "Tab" }, { ...menu, search: "x" }, a)).toBe(true);
    expect(a.ask).toHaveBeenCalledTimes(2);
  });

  it("stops a streaming answer on Escape before clearing or leaving", () => {
    const a = actions();
    handleKey({ key: "Escape" }, { ...streaming, search: "x" }, a);
    expect(a.stop).toHaveBeenCalledOnce();
    expect(a.clear).not.toHaveBeenCalled();
    handleKey({ key: "Escape" }, { ...answer, search: "x" }, a);
    handleKey({ key: "Escape" }, answer, a);
    expect(a.clear).toHaveBeenCalledOnce();
    expect(a.back).toHaveBeenCalledOnce();
    expect(a.hide).not.toHaveBeenCalled();
  });

  it("closes the menu on Escape even while streaming", () => {
    const a = actions();
    handleKey({ key: "Escape" }, { ...streaming, menu: true }, a);
    expect(a.closeMenu).toHaveBeenCalledOnce();
    expect(a.stop).not.toHaveBeenCalled();
  });

  it("removes a tag with Backspace in the empty field, once per press", () => {
    const a = actions();
    expect(handleKey({ key: "Backspace" }, answer, a)).toBe(true);
    expect(handleKey({ key: "Backspace", repeat: true }, answer, a)).toBe(true);
    expect(handleKey({ key: "Backspace" }, { ...answer, search: "x" }, a)).toBe(false);
    expect(a.removeTag).toHaveBeenCalledOnce();
    expect(stepsBack({ key: "Backspace" }, answer)).toBe(true);
  });
});

describe("⌃D and ⌃U", () => {
  it("page through the list with Vim keys on, as Page Down and Page Up do", () => {
    const a = actions();
    expect(handleKey({ key: "d", ctrlKey: true }, { ...list, vim: true }, a)).toBe(true);
    expect(handleKey({ key: "u", ctrlKey: true }, { ...list, vim: true }, a)).toBe(true);
    expect(a.page.mock.calls).toEqual([
      [1, true],
      [-1, true],
    ]);
    expect(a.shortcut).not.toHaveBeenCalled();
  });

  it("are left alone with Vim keys off and inside the actions menu", () => {
    const a = actions();
    handleKey({ key: "d", ctrlKey: true }, list, a);
    handleKey({ key: "u", ctrlKey: true }, { ...menu, vim: true }, a);
    expect(a.page).not.toHaveBeenCalled();
    expect(a.shortcut.mock.calls).toEqual([["⌃ D"], ["⌃ U"]]);
  });
});

describe("Quick Look", () => {
  const browsing = { ...list, search: "release", browsing: true };

  it("Space types a space while typing", () => {
    const a = actions();
    expect(handleKey({ key: " " }, { ...list, search: "release" }, a)).toBe(false);
    expect(a.preview).not.toHaveBeenCalled();
  });

  it("Space previews once ↓ moved the selection into the results", () => {
    const a = actions();
    expect(handleKey({ key: "ArrowDown" }, { ...list, search: "release" }, a)).toBe(true);
    expect(a.move).toHaveBeenCalledWith(1);
    expect(handleKey({ key: " " }, browsing, a)).toBe(true);
    expect(a.preview).toHaveBeenCalledOnce();
  });

  it("Space types a space on a row it cannot preview", () => {
    const a = { ...actions(), preview: vi.fn(() => false) };
    expect(handleKey({ key: " " }, browsing, a)).toBe(false);
  });

  it("⌘Y always previews, typing or not, once per press", () => {
    const a = actions();
    expect(handleKey({ key: "y", metaKey: true }, { ...list, search: "release" }, a)).toBe(true);
    expect(handleKey({ key: "y", metaKey: true }, browsing, a)).toBe(true);
    expect(handleKey({ key: "y", metaKey: true, repeat: true }, browsing, a)).toBe(true);
    expect(a.preview).toHaveBeenCalledTimes(2);
    expect(a.shortcut).not.toHaveBeenCalled();
  });

  it("⌘Y in the actions menu runs the menu's Quick Look", () => {
    const a = actions();
    expect(handleKey({ key: "y", metaKey: true }, menu, a)).toBe(true);
    expect(a.preview).not.toHaveBeenCalled();
    expect(a.shortcut).toHaveBeenCalledWith("⌘ Y");
  });

  it("a letter typed after moving types it, and then Space types a space again", () => {
    const a = actions();
    expect(handleKey({ key: "n" }, browsing, a)).toBe(false);
    expect(handleKey({ key: " " }, { ...list, search: "releasen" }, a)).toBe(false);
    expect(a.preview).not.toHaveBeenCalled();
  });

  it("Space closes an open preview while browsing but types while typing, and Escape only closes it", () => {
    const a = actions();
    const typing = { ...list, search: "release", previewing: true };
    expect(handleKey({ key: " " }, typing, a)).toBe(false);
    expect(a.preview).not.toHaveBeenCalled();
    expect(handleKey({ key: " " }, { ...typing, browsing: true }, a)).toBe(true);
    expect(a.preview).toHaveBeenCalledOnce();
    expect(handleKey({ key: "Escape" }, typing, a)).toBe(true);
    expect(a.closePreview).toHaveBeenCalledOnce();
    expect(a.clear).not.toHaveBeenCalled();
    expect(a.hide).not.toHaveBeenCalled();
  });

  it("Shift+Space previews as Space does, while held Space, ⌥Space and other screens leave Quick Look alone", () => {
    const a = actions();
    expect(handleKey({ key: " ", shiftKey: true }, browsing, a)).toBe(true);
    expect(handleKey({ key: " ", repeat: true }, browsing, a)).toBe(true);
    expect(handleKey({ key: " ", altKey: true }, browsing, a)).toBe(false);
    expect(handleKey({ key: " " }, { ...menu, browsing: true }, a)).toBe(false);
    expect(handleKey({ key: " " }, { ...list, answer: true, previewing: true }, a)).toBe(false);
    expect(handleKey({ key: "y", metaKey: true }, { ...list, settings: true }, a)).toBe(true);
    expect(a.preview).toHaveBeenCalledOnce();
  });
});

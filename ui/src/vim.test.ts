import { describe, expect, it, vi } from "vitest";
import { handleKey, stepsBack, switchesToInsert, vimLetter, type PanelView } from "./keys";
import { tabAfter } from "./pulls";

const actions = () => ({
  hide: vi.fn(),
  page: vi.fn(),
  move: vi.fn(),
  open: vi.fn(),
  cancel: vi.fn(),
  clear: vi.fn(),
  toggleMenu: vi.fn(),
  closeMenu: vi.fn(),
  shortcut: vi.fn(() => false),
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

type Actions = ReturnType<typeof actions>;

const base: PanelView = { menu: false, pressing: false, search: "", held: "", vim: true };
const insert = (view: Partial<PanelView> = {}): PanelView => ({ ...base, movable: true, ...view });
const normal = (view: Partial<PanelView> = {}): PanelView => insert({ normal: true, ...view });

const called = (a: Actions) =>
  Object.entries(a)
    .flatMap(([name, mock]) =>
      mock.mock.calls.map((args, at) => ({
        order: mock.mock.invocationCallOrder[at] ?? 0,
        call: [name, ...(args as unknown[])],
      })),
    )
    .toSorted((one, other) => one.order - other.order)
    .map(({ call }) => call);

const escape = (view: PanelView) => {
  const a = actions();
  expect(handleKey({ key: "Escape" }, view, a)).toBe(true);
  return called(a);
};

describe("Esc with Vim keys on", () => {
  it("hides the empty first screen on the first press, in Insert", () => {
    expect(escape(insert())).toEqual([["hide"]]);
  });

  it("goes to Normal first wherever there is something to move through", () => {
    expect(escape(insert({ search: "saf" }))).toEqual([["normal"]]);
    expect(escape(insert({ clipboard: true }))).toEqual([["normal"]]);
    expect(escape(insert({ clipboard: true, search: "x" }))).toEqual([["normal"]]);
    expect(escape(insert({ settings: true }))).toEqual([["normal"]]);
    expect(escape(insert({ pulls: true }))).toEqual([["normal"]]);
    expect(escape(insert({ answer: true }))).toEqual([["normal"]]);
  });

  it("then clears the search, goes back and hides, in Normal", () => {
    expect(escape(normal({ search: "saf" }))).toEqual([["clear"]]);
    expect(escape(normal())).toEqual([["hide"]]);
    expect(escape(normal({ clipboard: true, search: "x" }))).toEqual([["clear"]]);
    expect(escape(normal({ clipboard: true }))).toEqual([["back"]]);
    expect(escape(normal({ settings: true }))).toEqual([["back"]]);
    expect(escape(normal({ pulls: true }))).toEqual([["back"]]);
    expect(escape(normal({ answer: true }))).toEqual([["back"]]);
  });

  it("takes today's step when there is nothing to move through", () => {
    expect(escape(insert({ search: "zzzz", movable: false }))).toEqual([["clear"]]);
    expect(escape(insert({ clipboard: true, search: "zz", movable: false }))).toEqual([["clear"]]);
    expect(escape(insert({ pulls: true, movable: false }))).toEqual([["back"]]);
    expect(escape(insert({ answer: true, movable: false }))).toEqual([["back"]]);
  });

  it("takes today's step when it was typed ahead, so early letters never become commands", () => {
    expect(escape(insert({ search: "saf", early: true }))).toEqual([["clear"]]);
    expect(escape(insert({ clipboard: true, early: true }))).toEqual([["back"]]);
  });

  it("keeps its first jobs: a flashing row, the actions menu and a streaming answer", () => {
    expect(escape(insert({ pressing: true }))).toEqual([["cancel"]]);
    expect(escape(normal({ menu: true, search: "co" }))).toEqual([["clear"]]);
    expect(escape(normal({ menu: true }))).toEqual([["closeMenu"]]);
    expect(escape(insert({ answer: true, streaming: true }))).toEqual([["stop"]]);
    expect(escape(normal({ answer: true, streaming: true }))).toEqual([["stop"]]);
  });

  it("only forgets a waiting g", () => {
    expect(escape(normal({ waiting: true, search: "saf" }))).toEqual([["wait", false]]);
  });

  it("reads ⌃[ as Esc only with Vim keys on", () => {
    const a = actions();
    expect(handleKey({ key: "[", ctrlKey: true }, insert({ search: "saf" }), a)).toBe(true);
    expect(called(a)).toEqual([["normal"]]);
    expect(stepsBack({ key: "[", ctrlKey: true }, insert())).toBe(true);
    const off = actions();
    handleKey({ key: "[", ctrlKey: true }, { ...base, vim: false }, off);
    expect(called(off)).toEqual([["shortcut", "⌃ ["]]);
  });

  it("moves one step per press when held", () => {
    const a = actions();
    expect(handleKey({ key: "Escape", repeat: true }, normal({ held: "Escape" }), a)).toBe(true);
    expect(called(a)).toEqual([]);
  });

  it("is today's Esc with Vim keys off", () => {
    const off = { ...base, vim: false, movable: true };
    expect(escape({ ...off, search: "saf" })).toEqual([["clear"]]);
    expect(escape({ ...off, clipboard: true })).toEqual([["back"]]);
    expect(escape(off)).toEqual([["hide"]]);
  });
});

const press = (key: string, view: PanelView, more: Record<string, unknown> = {}) => {
  const a = actions();
  const handled = handleKey({ key, ...more }, view, a);
  return { handled, calls: called(a) };
};

describe("Normal", () => {
  it("moves with j and k, and jumps to the ends with gg and G", () => {
    expect(press("j", normal()).calls).toEqual([["line", 1]]);
    expect(press("k", normal()).calls).toEqual([["line", -1]]);
    expect(press("G", normal(), { shiftKey: true }).calls).toEqual([["edge", true]]);
    expect(press("g", normal()).calls).toEqual([["wait", true]]);
    expect(press("g", normal({ waiting: true })).calls).toEqual([
      ["wait", false],
      ["edge", false],
    ]);
  });

  it("switches Pull Requests tabs with gt and gT, and Shift does not forget the g", () => {
    const pulls = normal({ pulls: true, waiting: true });
    expect(press("Shift", pulls, { shiftKey: true })).toEqual({ handled: false, calls: [] });
    expect(press("T", pulls, { shiftKey: true }).calls).toEqual([
      ["wait", false],
      ["tab", -1],
    ]);
    expect(press("t", pulls).calls).toEqual([
      ["wait", false],
      ["tab", 1],
    ]);
    expect(press("t", normal({ waiting: true })).calls).toEqual([["wait", false]]);
  });

  it("forgets the g on any other key: a letter then does nothing, other keys act", () => {
    expect(press("j", normal({ waiting: true }))).toEqual({
      handled: true,
      calls: [["wait", false]],
    });
    expect(press("i", normal({ waiting: true }))).toEqual({
      handled: true,
      calls: [["wait", false]],
    });
    expect(press("ArrowDown", normal({ waiting: true })).calls).toEqual([
      ["wait", false],
      ["move", 1],
    ]);
  });

  it("drops a held g, i, a or /, so holding never becomes gg or types", () => {
    for (const key of ["g", "i", "a", "/"]) {
      expect(press(key, normal(), { repeat: true })).toEqual({ handled: true, calls: [] });
    }
    expect(press("j", normal(), { repeat: true }).calls).toEqual([["line", 1]]);
    expect(switchesToInsert({ key: "i" }, normal())).toBe(true);
    expect(switchesToInsert({ key: "i" }, insert())).toBe(false);
    expect(press("i", insert({ held: "i" }), { repeat: true })).toEqual({
      handled: true,
      calls: [],
    });
    expect(press("i", insert({ held: "i" }), { repeat: true, keyCode: 229 })).toEqual({
      handled: true,
      calls: [],
    });
  });

  it("goes back to typing with i or a at the end, and with / selecting the text", () => {
    expect(press("i", normal()).calls).toEqual([["insert", false, false]]);
    expect(press("a", normal()).calls).toEqual([["insert", false, false]]);
    expect(press("/", normal(), { shiftKey: true }).calls).toEqual([["insert", true, false]]);
    expect(press("i", normal(), { keyCode: 229 }).calls).toEqual([["insert", false, true]]);
  });

  it("takes no text: every other letter, digit and symbol does nothing", () => {
    for (const key of [..."bcdefhlmnopqrsuvwxyz0123456789.,;'[]-=`\\", "Dead"]) {
      expect(press(key, normal()), key).toEqual({ handled: true, calls: [] });
      const shifted = key === "Dead" ? key : key.toUpperCase();
      expect(press(shifted, normal(), { shiftKey: true }), key).toEqual({
        handled: true,
        calls: [],
      });
    }
    expect(press("∆", normal(), { altKey: true, code: "KeyJ" })).toEqual({
      handled: true,
      calls: [],
    });
    expect(press("Backspace", normal({ search: "saf" }))).toEqual({ handled: true, calls: [] });
  });

  it("keeps Enter, Tab, the arrows, Page keys, Backspace on empty and ⌘ shortcuts", () => {
    expect(press("Enter", normal()).calls).toEqual([["open"]]);
    expect(press("Tab", normal()).calls).toEqual([["ask"]]);
    expect(press("ArrowUp", normal()).calls).toEqual([["move", -1]]);
    expect(press("PageDown", normal()).calls).toEqual([["page", 1]]);
    expect(press("Backspace", normal({ clipboard: true })).calls).toEqual([["back"]]);
    expect(press("j", normal(), { ctrlKey: true }).calls).toEqual([["move", 1]]);
    expect(press("c", normal({ clipboard: true }), { metaKey: true }).calls).toEqual([
      ["shortcut", "⌘ C"],
    ]);
  });

  it("returns to typing on ⌘V so the paste lands", () => {
    expect(press("v", normal(), { metaKey: true }).calls).toEqual([
      ["insert", false],
      ["shortcut", "⌘ V"],
    ]);
    expect(press("v", insert(), { metaKey: true }).calls).toEqual([["shortcut", "⌘ V"]]);
  });

  it("leaves the actions menu's search to type every letter", () => {
    const menu = normal({ menu: true });
    expect(press("j", menu)).toEqual({ handled: false, calls: [] });
    expect(press("k", menu, { metaKey: true }).calls).toEqual([["toggleMenu"]]);
  });

  it("steps aside while an input method composes, and for the Esc that ends it", () => {
    expect(press("Escape", insert({ search: "に" }), { isComposing: true })).toEqual({
      handled: false,
      calls: [],
    });
    expect(press("Escape", insert({ search: "に" }), { keyCode: 229 })).toEqual({
      handled: false,
      calls: [],
    });
  });

  it("still reads Normal letters an input source marks as composing, since none can start", () => {
    expect(press("j", normal(), { keyCode: 229, code: "KeyJ" }).calls).toEqual([["line", 1]]);
  });

  it("is never on with Vim keys off, so j, k, g, G, i, a and / type", () => {
    const off = { ...base, vim: false, normal: true };
    for (const key of ["j", "k", "g", "G", "i", "a", "/"]) {
      expect(press(key, off)).toEqual({ handled: false, calls: [] });
    }
  });
});

describe("reading keys whatever the keyboard", () => {
  it("uses the letter produced, so Dvorak's j is j wherever it sits", () => {
    expect(vimLetter({ key: "j", code: "KeyC" })).toBe("j");
  });

  it("uses the position when the key produces no Latin letter", () => {
    expect(vimLetter({ key: "о", code: "KeyJ" })).toBe("j");
    expect(vimLetter({ key: "Г", code: "KeyG", shiftKey: true })).toBe("G");
    expect(press("о", normal(), { code: "KeyJ" }).calls).toEqual([["line", 1]]);
  });

  it("reads G from Shift, so Caps Lock changes nothing", () => {
    expect(vimLetter({ key: "J", code: "KeyJ" })).toBe("j");
    expect(vimLetter({ key: "G", code: "KeyG" })).toBe("g");
    expect(vimLetter({ key: "g", code: "KeyG", shiftKey: true })).toBe("G");
  });

  it("takes / as produced, Shift or not", () => {
    expect(vimLetter({ key: "/", code: "Digit7", shiftKey: true })).toBe("/");
  });
});

describe("Pull Requests tabs", () => {
  it("stop at the first and last tab instead of wrapping", () => {
    expect(tabAfter("all", 1)).toBe("failing");
    expect(tabAfter("drafts", 1)).toBe("drafts");
    expect(tabAfter("all", -1)).toBe("all");
    expect(tabAfter("ready", -1)).toBe("failing");
  });
});

describe("Quick Look in Normal mode", () => {
  it("Space previews without moving first, and types nothing when there is no preview", () => {
    const a = actions();
    expect(handleKey({ key: " " }, normal(), a)).toBe(true);
    expect(a.preview).toHaveBeenCalledOnce();
    const none = { ...actions(), preview: vi.fn(() => false) };
    expect(handleKey({ key: " " }, normal(), none)).toBe(true);
  });

  it("Space types a space in Insert mode until j or k move the selection", () => {
    const a = actions();
    expect(handleKey({ key: " " }, insert({ search: "release" }), a)).toBe(false);
    expect(handleKey({ key: "j" }, normal(), a)).toBe(true);
    expect(a.line).toHaveBeenCalledWith(1);
    expect(a.preview).not.toHaveBeenCalled();
  });
});

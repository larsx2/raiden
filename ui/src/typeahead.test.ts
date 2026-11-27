import { describe, expect, it, vi } from "vitest";
import { applyTyped, catchUp, type Field, type Typed } from "./typeahead";

const field = (value: string, start = value.length, end = start): Field => {
  const state: Field = {
    value,
    selectionStart: start,
    selectionEnd: end,
    setSelectionRange: (from, to) => {
      state.selectionStart = from;
      state.selectionEnd = to;
    },
  };
  return state;
};

const text = (value: string): Typed => ({ key: value, text: value });
const named = (key: string): Typed => ({ key, text: "" });

describe("keys typed while the panel was opening", () => {
  it("replace the last search, which is selected, and pass Enter to the panel's keys", () => {
    const f = field("old", 0, 3);
    const keys: string[] = [];
    const seen: string[] = [];
    const runKey = (key: string) => {
      keys.push(key);
      return key === "Enter";
    };
    for (const typed of [text("s"), text("a"), text("f"), named("Enter")]) {
      applyTyped(typed, f, runKey, () => seen.push(f.value));
    }
    expect(f.value).toBe("saf");
    expect(seen).toEqual(["s", "sa", "saf"]);
    expect(keys).toEqual(["Enter"]);
  });

  it("delete one character, or the selection, on Backspace the panel leaves alone", () => {
    const changed = vi.fn();
    const f = field("sa😀");
    applyTyped(named("Backspace"), f, () => false, changed);
    expect(f.value).toBe("sa");
    const selected = field("safari", 1, 4);
    applyTyped(named("Backspace"), selected, () => false, changed);
    expect(selected.value).toBe("sri");
    expect(changed).toHaveBeenCalledTimes(2);
  });

  it("leave the search alone when the panel uses Backspace itself", () => {
    const f = field("sa");
    applyTyped(named("Backspace"), f, () => true, vi.fn());
    expect(f.value).toBe("sa");
  });

  it("are pulled until Raiden holds no more, in order", async () => {
    const batches: (Typed[] | null)[] = [[text("s"), text("a")], [], [text("f")], null];
    const pulled: string[] = [];
    await catchUp(
      async () => batches.shift() ?? null,
      (typed) => pulled.push(typed.key),
    );
    expect(pulled).toEqual(["s", "a", "f"]);
    expect(batches).toEqual([]);
  });

  it("stop being pulled after Enter or Escape, so later keys never reach a hidden search", async () => {
    for (const stop of ["Enter", "Escape"]) {
      const pull = vi.fn(async () => [text("s"), named(stop)]);
      const pulled: string[] = [];
      await catchUp(pull, (typed) => pulled.push(typed.key));
      expect(pulled).toEqual(["s", stop]);
      expect(pull).toHaveBeenCalledOnce();
    }
  });

  it("stop being pulled when Raiden fails to answer", async () => {
    const pull = vi.fn(async (): Promise<Typed[] | null> => {
      throw new Error("gone");
    });
    await catchUp(pull, vi.fn());
    expect(pull).toHaveBeenCalledOnce();
  });

  it("stop being pulled after a bounded number of tries", async () => {
    const pull = vi.fn(async () => [] as Typed[]);
    await catchUp(pull, vi.fn());
    expect(pull).toHaveBeenCalledTimes(50);
  });
});

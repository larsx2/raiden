import { describe, expect, it } from "vitest";
import { freshEditor, press, settle, tokenOf, type Editor, type Line } from "./editing";

type Typed = { line: Line; insert: boolean; editor: Editor };

const type = (text: string, at: number, keys: string, from = freshEditor()): Typed => {
  let state: Typed = { line: { text, at }, insert: false, editor: from };
  for (const key of keys) {
    const done = press(state.editor, state.line, key);
    if (done === "skip") {
      throw new Error(`${key} was left for the screen`);
    }
    state = done;
  }
  return state;
};

const shown = ({ line }: Typed) => `${line.text.slice(0, line.at)}|${line.text.slice(line.at)}`;

const QUESTION = "how do I rebase onto main?";

describe("moving in the question", () => {
  it("h and l move a character and stop at the ends", () => {
    expect(shown(type("abc", 1, "l"))).toBe("ab|c");
    expect(shown(type("abc", 1, "lll"))).toBe("ab|c");
    expect(shown(type("abc", 1, "hhh"))).toBe("|abc");
  });

  it("w, b and e move by word, and W, B and E by space-separated word", () => {
    expect(shown(type(QUESTION, 0, "w"))).toBe("how |do I rebase onto main?");
    expect(shown(type(QUESTION, 0, "3w"))).toBe("how do I |rebase onto main?");
    expect(shown(type(QUESTION, 0, "e"))).toBe("ho|w do I rebase onto main?");
    expect(shown(type(QUESTION, 22, "w"))).toBe("how do I rebase onto main|?");
    expect(shown(type(QUESTION, 22, "W"))).toBe("how do I rebase onto main|?");
    expect(shown(type(QUESTION, 25, "b"))).toBe("how do I rebase onto |main?");
    expect(shown(type(QUESTION, 25, "B"))).toBe("how do I rebase onto |main?");
    expect(shown(type("a.b c", 0, "E"))).toBe("a.|b c");
  });

  it("0, ^ and $ go to the start, the first word and the last character", () => {
    expect(shown(type("  ask me", 5, "0"))).toBe("|  ask me");
    expect(shown(type("  ask me", 5, "^"))).toBe("  |ask me");
    expect(shown(type("  ask me", 2, "$"))).toBe("  ask m|e");
  });

  it("f, t, F and T find a character, and ; and , repeat the find", () => {
    expect(shown(type(QUESTION, 0, "fo"))).toBe("h|ow do I rebase onto main?");
    expect(shown(type(QUESTION, 0, "fo;"))).toBe("how d|o I rebase onto main?");
    expect(shown(type(QUESTION, 0, "fo;,"))).toBe("h|ow do I rebase onto main?");
    expect(shown(type(QUESTION, 0, "to"))).toBe("|how do I rebase onto main?");
    expect(shown(type(QUESTION, 25, "Fe"))).toBe("how do I rebas|e onto main?");
    expect(shown(type(QUESTION, 25, "Te"))).toBe("how do I rebase| onto main?");
    expect(shown(type(QUESTION, 0, "fz"))).toBe("|how do I rebase onto main?");
    expect(shown(type("a,b,c,d", 0, "t,;"))).toBe("a,|b,c,d");
    expect(shown(type("a,b,c,d", 6, "T,;"))).toBe("a,b,|c,d");
  });
});

describe("changing the question", () => {
  it("x and X delete a character, and a count deletes more", () => {
    expect(shown(type("abcd", 1, "x"))).toBe("a|cd");
    expect(shown(type("abcd", 1, "2x"))).toBe("a|d");
    expect(shown(type("abcd", 3, "x"))).toBe("ab|c");
    expect(shown(type("abcd", 2, "X"))).toBe("a|cd");
  });

  it("d deletes over a motion, a text object or the whole line", () => {
    expect(shown(type(QUESTION, 0, "dw"))).toBe("|do I rebase onto main?");
    expect(shown(type(QUESTION, 0, "d2w"))).toBe("|I rebase onto main?");
    expect(shown(type(QUESTION, 0, "2dw"))).toBe("|I rebase onto main?");
    expect(shown(type(QUESTION, 4, "de"))).toBe("how | I rebase onto main?");
    expect(shown(type(QUESTION, 9, "db"))).toBe("how do |rebase onto main?");
    expect(shown(type(QUESTION, 9, "d$"))).toBe("how do I| ");
    expect(shown(type(QUESTION, 9, "D"))).toBe("how do I| ");
    expect(shown(type(QUESTION, 9, "d0"))).toBe("|rebase onto main?");
    expect(shown(type(QUESTION, 11, "diw"))).toBe("how do I | onto main?");
    expect(shown(type(QUESTION, 11, "daw"))).toBe("how do I |onto main?");
    expect(shown(type(QUESTION, 11, "2daw"))).toBe("how do I |main?");
    expect(shown(type(QUESTION, 11, "d3iw"))).toBe("how do I | main?");
    expect(shown(type(QUESTION, 0, "dfe"))).toBe("|base onto main?");
    expect(shown(type(QUESTION, 0, "dte"))).toBe("|ebase onto main?");
    expect(shown(type(QUESTION, 12, "dd"))).toBe("|");
  });

  it("c changes over a motion and starts typing, and cw stops at the word's end", () => {
    const changed = type(QUESTION, 9, "cw");
    expect(changed.insert).toBe(true);
    expect(shown(changed)).toBe("how do I | onto main?");
    expect(shown(type(QUESTION, 9, "ciw"))).toBe("how do I | onto main?");
    expect(shown(type(QUESTION, 7, "cw"))).toBe("how do | rebase onto main?");
    expect(shown(type(QUESTION, 5, "cw"))).toBe("how d| I rebase onto main?");
    expect(shown(type(QUESTION, 4, "c2w"))).toBe("how | rebase onto main?");
    expect(shown(type(QUESTION, 9, "C"))).toBe("how do I |");
    expect(shown(type(QUESTION, 9, "cc"))).toBe("|");
    expect(shown(type(QUESTION, 9, "S"))).toBe("|");
    expect(shown(type("abc", 1, "s"))).toBe("a|c");
    expect(type("abc", 1, "s").insert).toBe(true);
  });

  it("r replaces characters under the cursor", () => {
    expect(shown(type("abcd", 1, "rx"))).toBe("a|xcd");
    expect(shown(type("abcd", 1, "2rx"))).toBe("ax|xd");
    expect(shown(type("abcd", 3, "2rx"))).toBe("abc|d");
  });

  it("y copies and p and P paste, and a deletion can be pasted", () => {
    expect(shown(type("one two", 0, "yiwP"))).toBe("on|eone two");
    expect(shown(type("one two", 0, "yiw$p"))).toBe("one twoon|e");
    expect(shown(type("one two", 0, "dwp"))).toBe("tone| wo");
    expect(shown(type("ab", 0, "yy$p"))).toBe("aba|b");
    expect(shown(type("ab", 0, "p"))).toBe("|ab");
    expect(shown(type("hello world", 6, "yy"))).toBe("hello |world");
  });

  it("u undoes the last change, one at a time", () => {
    expect(shown(type("abcd", 0, "xxu"))).toBe("|bcd");
    expect(shown(type("abcd", 0, "xxuu"))).toBe("|abcd");
    expect(shown(type("abcd", 0, "u"))).toBe("|abcd");
    expect(shown(type("abcd", 0, "xxx2u"))).toBe("|bcd");
  });

  it("undo takes back what was typed after a change as one step", () => {
    const changed = type(QUESTION, 9, "cw");
    const typed = { text: "how do I merge onto main?", at: 14 };
    const back = settle(changed.editor, typed);
    expect(back.line.at).toBe(13);
    expect(shown(type(typed.text, 13, "u", back.editor))).toBe(`how do I |${QUESTION.slice(9)}`);
  });
});

describe("starting to type", () => {
  it("i, a, I and A start typing before, after, at the first word and at the end", () => {
    expect(shown(type("  abc", 3, "i"))).toBe("  a|bc");
    expect(shown(type("  abc", 3, "a"))).toBe("  ab|c");
    expect(shown(type("  abc", 3, "I"))).toBe("  |abc");
    expect(shown(type("  abc", 3, "A"))).toBe("  abc|");
    expect(type("abc", 0, "i").insert).toBe(true);
  });

  it("going back to Normal puts the cursor on the last character typed", () => {
    expect(settle(freshEditor(), { text: "abc", at: 3 }).line.at).toBe(2);
    expect(settle(freshEditor(), { text: "abc", at: 0 }).line.at).toBe(0);
    expect(settle(freshEditor(), { text: "", at: 0 }).line.at).toBe(0);
  });
});

describe("counts", () => {
  it("stop at 999, however long the count typed", () => {
    expect(shown(type("ab", 0, "99999999999999999999l"))).toBe("a|b");
    expect(
      type("ab", 0, "99999999999999999999p", { ...freshEditor(), register: "x" }).line.text,
    ).toBe(`a${"x".repeat(999)}b`);
  });
});

describe("keys left for the screen", () => {
  it("j, k, g, G and / still scroll the answer and search, unless a command is waiting", () => {
    for (const key of ["j", "k", "g", "G", "/"]) {
      expect(press(freshEditor(), { text: "abc", at: 0 }, key)).toBe("skip");
    }
    const waiting = type("abc", 0, "d");
    expect(press(waiting.editor, waiting.line, "j")).not.toBe("skip");
    const counting = type("abc", 0, "3");
    expect(press(counting.editor, counting.line, "j")).toBe("skip");
  });

  it("the character after f or r is the one typed, whatever the layout", () => {
    expect(tokenOf(["f"], { key: "é", code: "Digit2" })).toBe("é");
    expect(tokenOf([], { key: "х", code: "KeyX" })).toBe("x");
    expect(tokenOf([], { key: "Enter" })).toBeUndefined();
  });

  it("emoji, flags and accented letters count as one character", () => {
    expect(shown(type("a😀b", 1, "x"))).toBe("a|b");
    expect(shown(type("a😀b", 0, "l"))).toBe("a|😀b");
    expect(shown(type("a😀b", 0, "ll"))).toBe("a😀|b");
    expect(shown(type("a 😀😀 b", 0, "ex"))).toBe("a 😀| b");
    expect(shown(type("a 😀😀 b", 0, "cw"))).toBe("| 😀😀 b");
    expect(shown(type("a🇲🇽b", 1, "x"))).toBe("a|b");
    expect(shown(type("a👍🏽b", 0, "lx"))).toBe("a|b");
    expect(shown(type("ae\u0301b", 1, "x"))).toBe("a|b");
    expect(shown(type("ae\u0301b", 1, "rx"))).toBe("a|xb");
    expect(shown(type("ae\u0301b", 0, "ll"))).toBe("ae\u0301|b");
    expect(shown(type("a 😀 b", 0, "ee"))).toBe("a 😀 |b");
    expect(shown(type("cafe\u0301 bar", 0, "w"))).toBe("cafe\u0301 |bar");
    expect(shown(type("cafe\u0301 bar", 0, "ee"))).toBe("cafe\u0301 ba|r");
    expect(shown(type("cafe\u0301 bar", 6, "b"))).toBe("|cafe\u0301 bar");
    expect(shown(type("cafe\u0301 bar", 1, "diw"))).toBe("| bar");
    expect(shown(type("नमस्ते दुनिया", 0, "dw"))).toBe("|दुनिया");
  });
});

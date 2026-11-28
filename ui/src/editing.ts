import { vimLetter, type Keystroke } from "./keys";

export type Line = { text: string; at: number };

type Find = { kind: "f" | "t" | "F" | "T"; char: string };

export type Editor = {
  keys: string[];
  register: string;
  undo: Line[];
  opened?: Line;
  find?: Find;
};

export type Edited = { editor: Editor; line: Line; insert: boolean };

type Range = { from: number; to: number };

const OPERATORS = new Set(["d", "c", "y"]);
const FINDS = new Set(["f", "t", "F", "T"]);
const WANTS_CHAR = new Set([...FINDS, "r"]);
const OBJECTS = new Set(["i", "a"]);
const UNDO_LIMIT = 100;
const COUNT_LIMIT = 999;
const TEXT_LIMIT = 20_000;
const KEYS: Record<string, string> = { ArrowLeft: "h", ArrowRight: "l", Home: "0", End: "$" };
const OPTION_SYMBOLS = new Set(["$", "^", ";", ","]);

export const freshEditor = (): Editor => ({ keys: [], register: "", undo: [] });

const characters = new Intl.Segmenter(undefined, { granularity: "grapheme" });

const characterAt = (text: string, at: number) => characters.segment(text).containing(at);

export function after(text: string, at: number): number {
  if (at >= text.length) {
    return text.length;
  }
  const found = characterAt(text, at);
  return found ? found.index + found.segment.length : at + 1;
}

export function before(text: string, at: number): number {
  if (at <= 0) {
    return 0;
  }
  return characterAt(text, at - 1)?.index ?? at - 1;
}

const lastChar = (text: string) => before(text, text.length);

const charStart = (text: string, at: number) =>
  at >= text.length ? at : (characterAt(text, at)?.index ?? at);

const rest = (text: string, at: number) => Math.min(Math.max(0, at), lastChar(text));

export const blockOf = (line: Line): [number, number] => [line.at, after(line.text, line.at)];

const JOINS = /^[\p{M}\u200d]$/u;
const isHigh = (code: number) => code >= 0xd800 && code <= 0xdbff;
const isLow = (code: number) => code >= 0xdc00 && code <= 0xdfff;

const pointAt = (text: string, at: number) => String.fromCodePoint(text.codePointAt(at) ?? 0);

const joined = (text: string, at: number) =>
  (isLow(text.charCodeAt(at)) && isHigh(text.charCodeAt(at - 1))) || JOINS.test(pointAt(text, at));

function kindAt(text: string, at: number, big: boolean): number {
  if (at < 0 || at >= text.length) {
    return 0;
  }
  let start = at;
  while (start > 0 && joined(text, start)) {
    start--;
  }
  const char = pointAt(text, start);
  if (/\s/.test(char)) {
    return 0;
  }
  return big || /[\p{L}\p{N}\p{M}_]/u.test(char) ? 1 : 2;
}

function wordStart(text: string, at: number, big: boolean): number {
  let next = at;
  const start = kindAt(text, next, big);
  if (start !== 0) {
    while (next < text.length && kindAt(text, next, big) === start) {
      next++;
    }
  }
  while (next < text.length && kindAt(text, next, big) === 0) {
    next++;
  }
  return next;
}

function wordEnd(text: string, at: number, big: boolean): number {
  let next = after(text, at);
  while (next < text.length && kindAt(text, next, big) === 0) {
    next++;
  }
  if (next >= text.length) {
    return Math.max(at, text.length - 1);
  }
  const found = kindAt(text, next, big);
  while (next + 1 < text.length && kindAt(text, next + 1, big) === found) {
    next++;
  }
  return next;
}

function wordBack(text: string, at: number, big: boolean): number {
  let next = at - 1;
  while (next > 0 && kindAt(text, next, big) === 0) {
    next--;
  }
  if (next <= 0) {
    return 0;
  }
  const found = kindAt(text, next, big);
  while (next > 0 && kindAt(text, next - 1, big) === found) {
    next--;
  }
  return next;
}

const firstWord = (text: string) => Math.max(0, text.search(/\S/));

function findChar(
  text: string,
  at: number,
  find: Find,
  count: number,
  again = false,
): number | undefined {
  const forward = find.kind === "f" || find.kind === "t";
  const close = again && (find.kind === "t" || find.kind === "T");
  let next = at;
  for (let time = 0; time < count; time++) {
    const skip = close && time === 0;
    const from = forward
      ? after(text, skip ? after(text, next) : next)
      : (skip ? before(text, next) : next) - 1;
    const found = forward
      ? text.indexOf(find.char, from)
      : from < 0
        ? -1
        : text.lastIndexOf(find.char, from);
    if (found < 0) {
      return undefined;
    }
    next = found;
  }
  if (find.kind === "t") {
    return Math.max(at, before(text, next));
  }
  if (find.kind === "T") {
    return Math.min(at, after(text, next));
  }
  return next;
}

const reversed = (find: Find): Find => ({
  ...find,
  kind: ({ f: "F", F: "f", t: "T", T: "t" } as const)[find.kind],
});

type Motion = { to: number; inclusive: boolean; find?: Find };

function moveBy(
  motion: string,
  char: string | undefined,
  text: string,
  at: number,
  count: number,
  last: Find | undefined,
): Motion | undefined {
  const repeat = (step: (from: number) => number) => {
    let next = at;
    for (let time = 0; time < count; time++) {
      next = step(next);
    }
    return next;
  };
  switch (motion) {
    case "h":
      return { to: repeat((from) => before(text, from)), inclusive: false };
    case "l":
      return { to: repeat((from) => after(text, from)), inclusive: false };
    case "w":
    case "W":
      return {
        to: repeat((from) => wordStart(text, from, motion === "W")),
        inclusive: false,
      };
    case "b":
    case "B":
      return {
        to: repeat((from) => wordBack(text, from, motion === "B")),
        inclusive: false,
      };
    case "e":
    case "E":
      return {
        to: repeat((from) => charStart(text, wordEnd(text, from, motion === "E"))),
        inclusive: true,
      };
    case "0":
      return { to: 0, inclusive: false };
    case "^":
      return { to: firstWord(text), inclusive: false };
    case "$":
      return { to: lastChar(text), inclusive: true };
    case ";":
    case ",": {
      if (!last) {
        return undefined;
      }
      const find = motion === ";" ? last : reversed(last);
      const to = findChar(text, at, find, count, true);
      return to === undefined
        ? undefined
        : { to, inclusive: find.kind === "f" || find.kind === "t" };
    }
    default: {
      if (!FINDS.has(motion) || char === undefined) {
        return undefined;
      }
      const find = { kind: motion as Find["kind"], char };
      const to = findChar(text, at, find, count);
      return to === undefined
        ? undefined
        : { to, inclusive: motion === "f" || motion === "t", find };
    }
  }
}

function wordObject(text: string, at: number, around: boolean, big: boolean): Range {
  if (text === "") {
    return { from: 0, to: 0 };
  }
  const here = kindAt(text, at, big);
  let from = at;
  let to = at + 1;
  while (from > 0 && kindAt(text, from - 1, big) === here) {
    from--;
  }
  while (to < text.length && kindAt(text, to, big) === here) {
    to++;
  }
  if (!around) {
    return { from, to };
  }
  if (here === 0) {
    return { from, to: to < text.length ? wordEnd(text, to - 1, big) + 1 : to };
  }
  let spaced = to;
  while (spaced < text.length && kindAt(text, spaced, big) === 0) {
    spaced++;
  }
  if (spaced > to) {
    return { from, to: spaced };
  }
  while (from > 0 && kindAt(text, from - 1, big) === 0) {
    from--;
  }
  return { from, to };
}

type Parsed = {
  count: number;
  command: string;
  char?: string;
  operator?: string;
};

function counted(keys: string[]): { count: number; rest: string[] } {
  let digits = "";
  let at = 0;
  while (
    at < keys.length &&
    /^[0-9]$/.test(keys[at] ?? "") &&
    !(digits === "" && keys[at] === "0")
  ) {
    digits += keys[at];
    at++;
  }
  const count = digits === "" ? 1 : Math.min(Number(digits), COUNT_LIMIT);
  return { count, rest: keys.slice(at) };
}

function parse(keys: string[]): Parsed | "pending" | "invalid" {
  const first = counted(keys);
  const [head, ...tail] = first.rest;
  if (head === undefined) {
    return "pending";
  }
  if (OPERATORS.has(head)) {
    const second = counted(tail);
    const [motion, char] = second.rest;
    const count = Math.min(first.count * second.count, COUNT_LIMIT);
    if (motion === undefined) {
      return "pending";
    }
    if (motion === head || motion === "G") {
      return { count, command: "line", operator: head };
    }
    if (motion === "g") {
      if (char === undefined) {
        return "pending";
      }
      return char === "g" ? { count, command: "line", operator: head } : "invalid";
    }
    if (OBJECTS.has(motion)) {
      if (char === undefined) {
        return "pending";
      }
      return char === "w" || char === "W"
        ? { count, command: motion + char, operator: head }
        : "invalid";
    }
    if (FINDS.has(motion)) {
      return char === undefined ? "pending" : { count, command: motion, char, operator: head };
    }
    return { count, command: motion, operator: head };
  }
  if (WANTS_CHAR.has(head)) {
    const [char] = tail;
    return char === undefined ? "pending" : { count: first.count, command: head, char };
  }
  return { count: first.count, command: head };
}

export function wantsChar(keys: string[]): boolean {
  return parse(keys) === "pending" && WANTS_CHAR.has(keys.at(-1) ?? "");
}

export function tokenOf(keys: string[], event: Keystroke): string | undefined {
  if (wantsChar(keys)) {
    return [...event.key].length === 1 ? event.key : undefined;
  }
  if (event.altKey === true) {
    return OPTION_SYMBOLS.has(event.key) ? event.key : undefined;
  }
  if (event.key === "Home" && /^[0-9]$/.test(keys.at(-1) ?? "")) {
    return undefined;
  }
  const typed = /^[0-9$^;,]$/.test(event.key) ? event.key : undefined;
  const letter = KEYS[event.key] ?? typed ?? vimLetter(event);
  return [...letter].length === 1 ? letter : undefined;
}

export const remember = (editor: Editor, line: Line): Editor => ({
  ...editor,
  undo: [...editor.undo, line].slice(-UNDO_LIMIT),
});

function operate(editor: Editor, line: Line, operator: string, range: Range): Edited {
  const { text } = line;
  const taken = text.slice(range.from, range.to);
  if (operator === "y") {
    if (taken === "") {
      return { editor, line: { text, at: rest(text, line.at) }, insert: false };
    }
    return {
      editor: { ...editor, register: taken },
      line: { text, at: rest(text, range.from) },
      insert: false,
    };
  }
  if (range.from === range.to && operator === "d") {
    return { editor, line: { text, at: rest(text, line.at) }, insert: false };
  }
  const left = text.slice(0, range.from) + text.slice(range.to);
  const register = taken === "" ? editor.register : taken;
  if (operator === "c") {
    return {
      editor: { ...editor, register, opened: line },
      line: { text: left, at: range.from },
      insert: true,
    };
  }
  return {
    editor: { ...remember(editor, line), register },
    line: { text: left, at: rest(left, range.from) },
    insert: false,
  };
}

function rangeOf(motion: Motion, at: number, text: string): Range {
  if (motion.to < at) {
    return { from: motion.to, to: motion.inclusive ? after(text, at) : at };
  }
  return {
    from: at,
    to: motion.inclusive ? after(text, motion.to) : motion.to,
  };
}

function withOperator(editor: Editor, line: Line, parsed: Parsed): Edited | undefined {
  const { text, at } = line;
  const operator = parsed.operator ?? "";
  if (parsed.command === "line" && operator === "y") {
    return {
      editor: { ...editor, register: text },
      line: { text, at: rest(text, at) },
      insert: false,
    };
  }
  if (parsed.command === "line") {
    return operate(editor, line, operator, { from: 0, to: text.length });
  }
  if (parsed.command.length === 2 && OBJECTS.has(parsed.command[0] ?? "")) {
    const around = parsed.command[0] === "a";
    const big = parsed.command[1] === "W";
    const range = wordObject(text, at, around, big);
    for (let time = 1; time < parsed.count && range.to < text.length; time++) {
      range.to = wordObject(text, range.to, around, big).to;
    }
    return operate(editor, line, operator, range);
  }
  const big = parsed.command === "W";
  const motion =
    operator === "c" && (parsed.command === "w" || big) && kindAt(text, at, big) !== 0
      ? changeWord(text, at, parsed.count, big)
      : moveBy(parsed.command, parsed.char, text, at, parsed.count, editor.find);
  if (!motion) {
    return undefined;
  }
  const found = motion.find ? { ...editor, find: motion.find } : editor;
  return operate(found, line, operator, rangeOf(motion, at, text));
}

function changeWord(text: string, at: number, count: number, big: boolean): Motion {
  let to = wordEnd(text, at - 1, big);
  for (let time = 1; time < count; time++) {
    to = wordEnd(text, to, big);
  }
  return { to: charStart(text, to), inclusive: true };
}

const insertAt = (editor: Editor, line: Line, at: number): Edited => ({
  editor: { ...editor, opened: line },
  line: { text: line.text, at },
  insert: true,
});

function paste(editor: Editor, line: Line, count: number, later: boolean): Edited | undefined {
  if (editor.register === "") {
    return undefined;
  }
  const { text, at } = line;
  const spot = later && text !== "" ? after(text, at) : at;
  if (text.length + editor.register.length * count > TEXT_LIMIT) {
    return undefined;
  }
  const pasted = editor.register.repeat(count);
  const next = text.slice(0, spot) + pasted + text.slice(spot);
  return {
    editor: remember(editor, line),
    line: { text: next, at: before(next, spot + pasted.length) },
    insert: false,
  };
}

function replace(editor: Editor, line: Line, count: number, char: string): Edited | undefined {
  const { text, at } = line;
  let end = at;
  for (let time = 0; time < count; time++) {
    if (end >= text.length) {
      return undefined;
    }
    end = after(text, end);
  }
  const next = text.slice(0, at) + char.repeat(count) + text.slice(end);
  return {
    editor: remember(editor, line),
    line: { text: next, at: at + char.length * (count - 1) },
    insert: false,
  };
}

function undo(editor: Editor, count: number): Edited | undefined {
  const kept = editor.undo.slice(0, Math.max(0, editor.undo.length - count));
  const last = editor.undo[kept.length];
  if (!last) {
    return undefined;
  }
  return {
    editor: { ...editor, undo: kept },
    line: { text: last.text, at: rest(last.text, last.at) },
    insert: false,
  };
}

function run(editor: Editor, line: Line, parsed: Parsed): Edited | undefined | "skip" {
  if (parsed.operator !== undefined) {
    return withOperator(editor, line, parsed);
  }
  const { text, at } = line;
  const shorthand = (operator: string, command: string) =>
    withOperator(editor, line, { count: parsed.count, operator, command });
  switch (parsed.command) {
    case "x":
      return text === "" ? undefined : shorthand("d", "l");
    case "X":
      return shorthand("d", "h");
    case "s":
      return text === "" ? insertAt(editor, line, 0) : shorthand("c", "l");
    case "D":
      return shorthand("d", "$");
    case "C":
      return shorthand("c", "$");
    case "S":
      return shorthand("c", "line");
    case "Y":
      return shorthand("y", "line");
    case "p":
    case "P":
      return paste(editor, line, parsed.count, parsed.command === "p");
    case "r":
      return replace(editor, line, parsed.count, parsed.char ?? "");
    case "u":
      return undo(editor, parsed.count);
    case "i":
      return insertAt(editor, line, at);
    case "a":
      return insertAt(editor, line, after(text, at));
    case "I":
      return insertAt(editor, line, firstWord(text));
    case "A":
      return insertAt(editor, line, text.length);
    default: {
      const motion = moveBy(parsed.command, parsed.char, text, at, parsed.count, editor.find);
      if (!motion) {
        return /^[hlwbeWBE0^$;,]$/.test(parsed.command) || FINDS.has(parsed.command)
          ? undefined
          : "skip";
      }
      const found = motion.find ? { ...editor, find: motion.find } : editor;
      return {
        editor: found,
        line: { text, at: rest(text, motion.to) },
        insert: false,
      };
    }
  }
}

export function press(editor: Editor, line: Line, token: string): Edited | "skip" {
  const keys = [...editor.keys, token];
  const parsed = parse(keys);
  if (parsed === "pending") {
    return { editor: { ...editor, keys }, line, insert: false };
  }
  if (parsed === "invalid") {
    return { editor: { ...editor, keys: [] }, line, insert: false };
  }
  const find =
    FINDS.has(parsed.command) && parsed.char !== undefined
      ? { kind: parsed.command as Find["kind"], char: parsed.char }
      : editor.find;
  const cleared = { ...editor, keys: [], find };
  const unchanged = { editor: cleared, line, insert: false };
  const done = run(cleared, line, parsed);
  if (done === "skip") {
    return editor.keys.every((key) => /^[0-9]$/.test(key)) ? "skip" : unchanged;
  }
  return done ?? unchanged;
}

export function settle(editor: Editor, line: Line): { editor: Editor; line: Line } {
  const opened = editor.opened;
  const kept = { ...editor, keys: [], opened: undefined };
  const settled = opened && opened.text !== line.text ? remember(kept, opened) : kept;
  return {
    editor: settled,
    line: { text: line.text, at: rest(line.text, before(line.text, line.at)) },
  };
}

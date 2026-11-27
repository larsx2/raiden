import hljs from "highlight.js/lib/core";
import bash from "highlight.js/lib/languages/bash";
import c from "highlight.js/lib/languages/c";
import cpp from "highlight.js/lib/languages/cpp";
import css from "highlight.js/lib/languages/css";
import diff from "highlight.js/lib/languages/diff";
import dockerfile from "highlight.js/lib/languages/dockerfile";
import go from "highlight.js/lib/languages/go";
import ini from "highlight.js/lib/languages/ini";
import java from "highlight.js/lib/languages/java";
import javascript from "highlight.js/lib/languages/javascript";
import json from "highlight.js/lib/languages/json";
import kotlin from "highlight.js/lib/languages/kotlin";
import python from "highlight.js/lib/languages/python";
import ruby from "highlight.js/lib/languages/ruby";
import rust from "highlight.js/lib/languages/rust";
import sql from "highlight.js/lib/languages/sql";
import swift from "highlight.js/lib/languages/swift";
import typescript from "highlight.js/lib/languages/typescript";
import xml from "highlight.js/lib/languages/xml";
import yaml from "highlight.js/lib/languages/yaml";

export type Tone =
  | "comment"
  | "keyword"
  | "string"
  | "number"
  | "function"
  | "type"
  | "punct"
  | "added"
  | "removed";

export type Token = { tone: Tone | null; text: string };

const GRAMMARS = {
  bash,
  c,
  cpp,
  css,
  diff,
  dockerfile,
  go,
  ini,
  java,
  javascript,
  json,
  kotlin,
  python,
  ruby,
  rust,
  sql,
  swift,
  typescript,
  xml,
  yaml,
};

type Grammar = keyof typeof GRAMMARS;

const NAMES: Record<string, Grammar> = {
  bash: "bash",
  sh: "bash",
  zsh: "bash",
  shell: "bash",
  console: "bash",
  "shell-session": "bash",
  c: "c",
  h: "c",
  cpp: "cpp",
  "c++": "cpp",
  cc: "cpp",
  cxx: "cpp",
  hpp: "cpp",
  css: "css",
  diff: "diff",
  patch: "diff",
  dockerfile: "dockerfile",
  docker: "dockerfile",
  go: "go",
  golang: "go",
  toml: "ini",
  ini: "ini",
  java: "java",
  js: "javascript",
  javascript: "javascript",
  jsx: "javascript",
  mjs: "javascript",
  cjs: "javascript",
  json: "json",
  jsonc: "json",
  kotlin: "kotlin",
  kt: "kotlin",
  kts: "kotlin",
  python: "python",
  py: "python",
  python3: "python",
  py3: "python",
  ruby: "ruby",
  rb: "ruby",
  rust: "rust",
  rs: "rust",
  sql: "sql",
  swift: "swift",
  ts: "typescript",
  typescript: "typescript",
  tsx: "typescript",
  mts: "typescript",
  cts: "typescript",
  html: "xml",
  xml: "xml",
  svg: "xml",
  yaml: "yaml",
  yml: "yaml",
};

const TONES: Record<string, Tone | null> = {
  subst: null,
  params: null,
  comment: "comment",
  quote: "comment",
  keyword: "keyword",
  "selector-tag": "keyword",
  name: "keyword",
  meta: "keyword",
  string: "string",
  regexp: "string",
  symbol: "string",
  char: "string",
  "meta-string": "string",
  "selector-attr": "string",
  number: "number",
  literal: "number",
  title: "function",
  "title.function": "function",
  "title.function.invoke": "function",
  attr: "function",
  attribute: "function",
  "selector-id": "function",
  "selector-class": "function",
  "title.class": "type",
  "title.class.inherited": "type",
  type: "type",
  built_in: "type",
  section: "type",
  "selector-pseudo": "type",
  punctuation: "punct",
  operator: "punct",
  tag: "punct",
  addition: "added",
  deletion: "removed",
};

for (const [name, grammar] of Object.entries(GRAMMARS)) {
  hljs.registerLanguage(name, grammar);
}

const own = <T>(table: Record<string, T>, key: string): T | undefined =>
  Object.hasOwn(table, key) ? table[key] : undefined;

const CACHED = 400;
const cache = new Map<string, Token[]>();
let highlighted = 0;

export function grammarOf(lang: string): Grammar | null {
  return own(NAMES, lang.toLowerCase().split(/[:,]/)[0] ?? "") ?? null;
}

function toneOf(classes: string, outer: Tone | null): Tone | null {
  if (classes.startsWith("language-")) {
    return null;
  }
  const scope = classes
    .split(" ")
    .map((part) => part.replace(/^hljs-/, "").replace(/_+$/, ""))
    .join(".");
  const base = scope.split(".")[0] ?? "";
  const tone = own(TONES, scope) ?? own(TONES, base);
  return tone === undefined ? outer : tone;
}

const ENTITIES: Record<string, string> = {
  "&amp;": "&",
  "&lt;": "<",
  "&gt;": ">",
  "&quot;": '"',
  "&#x27;": "'",
};

const PIECE =
  /<span class="((?:hljs|language)-[\w-]+(?: [\w-]+)*)">|<\/span>|&(?:amp|lt|gt|quot|#x27);|[^<&]+/y;

export function tokensOfHtml(html: string): Token[] | null {
  const tokens: Token[] = [];
  const open: Array<Tone | null> = [];
  PIECE.lastIndex = 0;
  while (PIECE.lastIndex < html.length) {
    const found = PIECE.exec(html);
    if (!found) {
      return null;
    }
    const [piece, classes] = found;
    if (classes !== undefined) {
      open.push(toneOf(classes, open.at(-1) ?? null));
      continue;
    }
    if (piece === "</span>") {
      if (open.pop() === undefined) {
        return null;
      }
      continue;
    }
    const text = own(ENTITIES, piece) ?? piece;
    const tone = open.at(-1) ?? null;
    const last = tokens.at(-1);
    if (last && last.tone === tone) {
      last.text += text;
    } else {
      tokens.push({ tone, text });
    }
  }
  return open.length === 0 ? tokens : null;
}

function colored(text: string, grammar: Grammar): Token[] {
  highlighted++;
  const html = hljs.highlight(text, { language: grammar, ignoreIllegals: true }).value;
  return tokensOfHtml(html) ?? [{ tone: null, text }];
}

let draft = { key: "", tokens: [] as Token[] };

function draftOf(text: string, grammar: Grammar): Token[] {
  const key = keyOf(text, grammar);
  if (draft.key !== key) {
    draft = { key, tokens: cache.get(key) ?? colored(text, grammar) };
  }
  return draft.tokens;
}

export function tokensOf(text: string, grammar: Grammar): Token[] {
  const key = keyOf(text, grammar);
  const known = cache.get(key);
  if (known) {
    cache.delete(key);
    cache.set(key, known);
    return known;
  }
  const tokens = colored(text, grammar);
  cache.set(key, tokens);
  if (cache.size > CACHED) {
    cache.delete(cache.keys().next().value ?? "");
  }
  return tokens;
}

export const CHUNK_LINES = 40;

export type Code = { done: Token[][]; tail: string };

const keyOf = (text: string, grammar: Grammar) => `${grammar}\u0000${text}`;

function chunksOf(text: string, grammar: Grammar): Token[][] {
  const lines = text.split(/(?<=\n)/);
  const chunks: Token[][] = [];
  for (let at = 0; at < lines.length; at += CHUNK_LINES) {
    const part = lines.slice(at, at + CHUNK_LINES);
    const chunk = part.join("");
    if (chunk) {
      chunks.push(part.length === CHUNK_LINES ? tokensOf(chunk, grammar) : draftOf(chunk, grammar));
    }
  }
  return chunks;
}

export const LONGEST_CODE = 20_000;
const LONGEST_LINE = 1000;

function hasLongLine(text: string): boolean {
  let start = 0;
  while (start <= text.length) {
    const end = text.indexOf("\n", start);
    const stop = end < 0 ? text.length : end;
    if (stop - start >= LONGEST_LINE) {
      return true;
    }
    start = stop + 1;
  }
  return false;
}

export type Stage = "arriving" | "closed" | "settled";

export function codeOf(text: string, lang: string, stage: Stage): Code | null {
  const grammar = grammarOf(lang);
  if (!grammar || text.length > LONGEST_CODE || hasLongLine(text)) {
    return null;
  }
  if (stage === "arriving") {
    const cut = text.lastIndexOf("\n") + 1;
    return { done: chunksOf(text.slice(0, cut), grammar), tail: text.slice(cut) };
  }
  const whole =
    stage === "settled" ||
    cache.has(keyOf(text, grammar)) ||
    text.split("\n").length <= CHUNK_LINES;
  return whole
    ? { done: [tokensOf(text, grammar)], tail: "" }
    : { done: chunksOf(text, grammar), tail: "" };
}

export function highlightCount(): number {
  return highlighted;
}

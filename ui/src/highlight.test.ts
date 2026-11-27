import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import {
  CHUNK_LINES,
  codeOf,
  LONGEST_CODE,
  grammarOf,
  highlightCount,
  tokensOf,
  tokensOfHtml,
  type Token,
} from "./highlight";

const SAMPLES: Record<string, [code: string, tone: string, text: string]> = {
  bash: ['if [ -f x ]; then echo "hi"; fi', "keyword", "if"],
  sh: ["# set up\njust setup", "comment", "# set up"],
  zsh: ["echo 'zsh'", "string", "'zsh'"],
  shell: ["for f in *; do :; done", "keyword", "for"],
  js: ["const a = 42;", "number", "42"],
  ts: ["let n: number = 1;", "type", "number"],
  jsx: ["function App() { return 1; }", "function", "App"],
  tsx: ["const x = 'y';", "string", "'y'"],
  json: ['{"name": true}', "function", '"name"'],
  rust: ["fn main() {}", "function", "main"],
  python: ["def run(): pass", "keyword", "def"],
  go: ["package main", "keyword", "package"],
  swift: ['let s = "hi"', "string", '"hi"'],
  toml: ['[tool]\nname = "x"', "type", "[tool]"],
  yaml: ["name: raiden", "function", "name:"],
  diff: ["-old\n+new", "added", "+new"],
  css: ["a { color: red; }", "function", "color"],
  html: ['<a href="x">y</a>', "keyword", "a"],
  sql: ["SELECT 1", "keyword", "SELECT"],
  ruby: ["def go; end", "keyword", "def"],
  c: ["int main(void) { return 0; }", "type", "int"],
  cpp: ["auto n = 0u;", "keyword", "auto"],
  java: ["public class A {}", "keyword", "public"],
  kotlin: ["fun main() {}", "keyword", "fun"],
  dockerfile: ["FROM node:22\nRUN echo 'hi'", "string", "'hi'"],
  svg: ["<style>a { color: red; }</style>", "function", "color"],
  xml: ["<script>let a = 1;</script>", "number", "1"],
};

const toned = (tokens: Token[]) => tokens.filter((token) => token.tone);

describe("code colors", () => {
  for (const [lang, [code, tone, text]] of Object.entries(SAMPLES)) {
    it(`colors ${lang}`, () => {
      const shown = codeOf(code, lang, "settled");
      expect(
        shown?.done
          .flat()
          .map((token) => token.text)
          .join(""),
      ).toBe(code);
      expect(toned(shown?.done.flat() ?? [])).toContainEqual({ tone, text });
    });
  }

  it("reads a fence's language in any case", () => {
    expect(grammarOf("TypeScript")).toBe("typescript");
    expect(grammarOf("YML")).toBe("yaml");
    expect(grammarOf("ts:src/main.ts")).toBe("typescript");
    expect(grammarOf("rust,ignore")).toBe("rust");
    expect(grammarOf("python3")).toBe("python");
    expect(grammarOf("constructor")).toBeNull();
    expect(grammarOf("__proto__")).toBeNull();
    expect(codeOf("x", "toString", "settled")).toBeNull();
  });

  it("leaves a block with no language or an unknown one plain", () => {
    expect(codeOf("SELECT 1", "", "settled")).toBeNull();
    expect(codeOf("SELECT 1", "brainfuck", "settled")).toBeNull();
  });

  it("colors the same text and language once, and again when either changes", () => {
    const before = highlightCount();
    const first = tokensOf("let once = 1;", "typescript");
    expect(tokensOf("let once = 1;", "typescript")).toBe(first);
    expect(highlightCount()).toBe(before + 1);
    tokensOf("let once = 1;", "javascript");
    tokensOf("let once = 2;", "typescript");
    expect(highlightCount()).toBe(before + 3);
  });

  it("colors a block still arriving by whole lines, leaving the line being typed plain", () => {
    const shown = codeOf("const a = 1;\nconst b", "ts", "arriving");
    expect(
      shown?.done
        .flat()
        .map((token) => token.text)
        .join(""),
    ).toBe("const a = 1;\n");
    expect(shown?.tail).toBe("const b");
  });

  it("keeps the earlier parts of a long arriving block as they were", () => {
    const lines = Array.from({ length: CHUNK_LINES * 2 + 3 }, (_, at) => `let v${at} = ${at};\n`);
    const text = lines.join("");
    const before = codeOf(text, "ts", "arriving");
    const counted = highlightCount();
    const after = codeOf(`${text}let more = 1;\n`, "ts", "arriving");
    expect(highlightCount()).toBe(counted + 1);
    expect(after?.done[0]).toBe(before?.done[0]);
    expect(after?.done[1]).toBe(before?.done[1]);
    expect(after?.done).toHaveLength(3);
  });

  it("colors a long closed block in parts while the answer streams, and whole once it ends", () => {
    const text = Array.from({ length: CHUNK_LINES + 5 }, (_, at) => `x${at} = ${at}`).join("\n");
    const parts = codeOf(text, "py", "closed");
    expect(parts?.done).toHaveLength(2);
    expect(
      parts?.done
        .flat()
        .map((token) => token.text)
        .join(""),
    ).toBe(text);
    const whole = codeOf(text, "py", "settled");
    expect(whole?.done).toHaveLength(1);
    expect(codeOf(text, "py", "closed")?.done).toHaveLength(1);
  });

  it("leaves a block plain when a line or the whole is too long to color quickly", () => {
    expect(codeOf(`let a = "${"x".repeat(2000)}";`, "ts", "settled")).toBeNull();
    expect(codeOf(`let a = "${"x".repeat(2000)}";\n`, "ts", "arriving")).toBeNull();
    const many = "let a = 1;\n".repeat(LONGEST_CODE / 10);
    expect(codeOf(many, "ts", "settled")).toBeNull();
    expect(codeOf("let a = 1;\n".repeat(300), "ts", "settled")).not.toBeNull();
    expect(codeOf(`${"a".repeat(999)}\n${"b".repeat(999)}`, "ts", "settled")).not.toBeNull();
    expect(codeOf(`a\n${"b".repeat(1000)}`, "ts", "settled")).toBeNull();
  });

  it("keeps words that every object has as the words they are", () => {
    const code = "a.toString();\nclass A { constructor() { this.valueOf(); } }";
    for (const lang of ["ts", "js", "java"]) {
      const tokens = codeOf(code, lang, "settled")?.done.flat() ?? [];
      expect(tokens.every((token) => typeof token.text === "string")).toBe(true);
      expect(tokens.map((token) => token.text).join("")).toBe(code);
    }
  });

  it("keeps markup in the answer as text", () => {
    const attack = '<img src=x onerror="alert(1)">\n</code><script>alert(2)</script>';
    for (const lang of ["html", "ts", "sh", "json", "diff"]) {
      const tokens = codeOf(attack, lang, "settled")?.done.flat() ?? [];
      expect(tokens.map((token) => token.text).join("")).toBe(attack);
    }
  });

  it("refuses highlighter output that is anything but its own spans and escaped text", () => {
    expect(tokensOfHtml('<span class="hljs-string">&quot;a&quot; &amp; b</span>')).toEqual([
      { tone: "string", text: '"a" & b' },
    ]);
    expect(
      tokensOfHtml('<span class="hljs-string">a<span class="language-bash">b</span></span>'),
    ).toEqual([
      { tone: "string", text: "a" },
      { tone: null, text: "b" },
    ]);
    expect(tokensOfHtml("<img src=x>")).toBeNull();
    expect(tokensOfHtml('<span class="x" onclick="y">a</span>')).toBeNull();
    expect(tokensOfHtml('<span class="hljs-string" style="x">a</span>')).toBeNull();
    expect(tokensOfHtml("&nbsp;")).toBeNull();
    expect(tokensOfHtml('<span class="hljs-string">a')).toBeNull();
    expect(tokensOfHtml("a</span>")).toBeNull();
  });
});

const luminance = (color: string) => {
  const [red = 0, green = 0, blue = 0] = [1, 3, 5].map((at) => {
    const value = Number.parseInt(color.slice(at, at + 2), 16) / 255;
    return value <= 0.04045 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * red + 0.7152 * green + 0.0722 * blue;
};

const contrast = (one: string, other: string) => {
  const [light, dark] = [luminance(one), luminance(other)].toSorted((a, b) => b - a);
  return ((light ?? 0) + 0.05) / ((dark ?? 0) + 0.05);
};

const BACKGROUNDS = { light: "#bfbfc0", dark: "#5a5a5c" };

describe("code color contrast", () => {
  const css = readFileSync(join(process.cwd(), "src", "style.css"), "utf8");
  const themes = {
    light: css.slice(0, css.indexOf("@media (prefers-color-scheme: dark)")),
    dark: css.slice(css.indexOf("@media (prefers-color-scheme: dark)")),
  };
  for (const [theme, rules] of Object.entries(themes) as Array<[keyof typeof themes, string]>) {
    it(`reads on the code block in ${theme}`, () => {
      const colors = [...rules.matchAll(/--code-(\w+): (#[0-9a-f]{6});/g)];
      expect(colors.map(([, name]) => name)).toEqual([
        "comment",
        "keyword",
        "string",
        "number",
        "function",
        "type",
        "punct",
        "added",
        "removed",
      ]);
      for (const [, name, color = ""] of colors) {
        const least = name === "comment" ? 3 : 4.5;
        expect(contrast(color, BACKGROUNDS[theme]), name).toBeGreaterThanOrEqual(least);
      }
    });
  }
});

import { createSignal } from "solid-js";
import { render } from "solid-js/web";
import { describe, expect, it } from "vitest";
import { highlightCount } from "./highlight";
import { plainOf } from "./markdown";
import { Reply } from "./Reply";

const shown = (text: () => string, streaming = () => false) => {
  const root = document.createElement("div");
  document.body.append(root);
  const dispose = render(
    () => <Reply text={text()} streaming={streaming()} open={() => undefined} />,
    root,
  );
  return { root, dispose };
};

const ATTACK = [
  "Look:",
  "",
  "```html",
  '<img src=x onerror="window.hacked = 1">',
  "</code><script>window.hacked = 2</script>",
  "```",
  "",
  "```",
  '<img src=x onerror="window.hacked = 3">',
  "```",
].join("\n");

describe("code in answers", () => {
  it("shows colored tokens as spans with only token classes", () => {
    const { root, dispose } = shown(() => "```ts\nconst a = new Map(); // one\n```");
    const code = root.querySelector("pre code");
    expect(code?.textContent).toBe("const a = new Map(); // one");
    const classes = [...(code?.querySelectorAll("span[class]") ?? [])].map(
      (span) => span.className,
    );
    expect(classes).toEqual(["tk-keyword", "tk-keyword", "tk-type", "tk-comment"]);
    dispose();
  });

  it("never turns answer text into markup", () => {
    const { root, dispose } = shown(() => ATTACK);
    expect(root.querySelectorAll("img, script")).toHaveLength(0);
    const elements = [...root.querySelectorAll("pre *")];
    expect(elements.every((node) => ["CODE", "SPAN"].includes(node.tagName))).toBe(true);
    expect(elements.every((node) => [...node.attributes].every((at) => at.name === "class"))).toBe(
      true,
    );
    expect([...root.querySelectorAll("pre")].map((pre) => pre.textContent)).toEqual([
      '<img src=x onerror="window.hacked = 1">\n</code><script>window.hacked = 2</script>',
      '<img src=x onerror="window.hacked = 3">',
    ]);
    expect((window as unknown as { hacked?: number }).hacked).toBeUndefined();
    dispose();
  });

  it("leaves a block with no language or an unknown one as plain text", () => {
    const { root, dispose } = shown(() => "```\nlet a = 1\n```\n\n```brainfuck\n+++.\n```");
    expect(root.querySelectorAll("pre code, pre span")).toHaveLength(0);
    expect([...root.querySelectorAll("pre")].map((pre) => pre.textContent)).toEqual([
      "let a = 1",
      "+++.",
    ]);
    dispose();
  });

  it("leaves a finished block as drawn while the answer keeps streaming", () => {
    const [text, setText] = createSignal("```rs\nfn main() {}\n```\n\nThen");
    const { root, dispose } = shown(text, () => true);
    const drawn = root.querySelector("pre code span span");
    expect(drawn?.className).toBe("tk-keyword");
    const counted = highlightCount();
    for (const more of [" run", " it", " with", " cargo."]) {
      setText(text() + more);
    }
    expect(root.querySelector("pre code span span")).toBe(drawn);
    expect(highlightCount()).toBe(counted);
    dispose();
  });

  it("colors a long block whole once the answer has finished", async () => {
    const lines = Array.from({ length: 100 }, (_, at) => `x${at} = ${at}`);
    const [text, setText] = createSignal(`\`\`\`py\n${lines.join("\n")}\n`);
    const [streaming, setStreaming] = createSignal(true);
    const { root, dispose } = shown(text, streaming);
    const parts = () => root.querySelectorAll("pre code > span").length;
    expect(parts()).toBe(3);
    setText(`${text()}\`\`\`\n\nDone.`);
    await new Promise((next) => setTimeout(next, 20));
    expect(parts()).toBe(3);
    setStreaming(false);
    expect(parts()).toBe(3);
    await new Promise((next) => setTimeout(next, 20));
    expect(parts()).toBe(1);
    expect(root.querySelector("pre")?.textContent).toBe(lines.join("\n"));
    dispose();
  });

  it("colors a long block of a finished answer whole at once", () => {
    const lines = Array.from({ length: 100 }, (_, at) => `x${at} = ${at}`);
    const { root, dispose } = shown(() => `\`\`\`py\n${lines.join("\n")}\n\`\`\``);
    expect(root.querySelectorAll("pre code > span")).toHaveLength(1);
    dispose();
  });

  it("colors an arriving block only when a line is finished", () => {
    const [text, setText] = createSignal("```py\nimport os\n");
    const { root, dispose } = shown(text, () => true);
    const counted = highlightCount();
    for (const more of ["pri", "nt(", "os.", "sep"]) {
      setText(text() + more);
    }
    expect(highlightCount()).toBe(counted);
    expect(root.querySelector("pre")?.textContent).toBe("import os\nprint(os.sep");
    setText(`${text()})\n`);
    expect(highlightCount()).toBe(counted + 1);
    dispose();
  });

  it("copies plain text, whatever the colors", () => {
    expect(plainOf("```ts\nconst a = 1;\n```")).toBe("const a = 1;");
  });
});

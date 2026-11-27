import { describe, expect, it } from "vitest";
import { blocksAfter, blocksOf, plainOf, spansOf, webLink } from "./markdown";

const text = (value: string) => ({ kind: "text", text: value });
const paragraph = (...spans: unknown[]) => ({ kind: "paragraph", spans });

describe("answer blocks", () => {
  it("splits paragraphs, code blocks and inline code", () => {
    const answer = "Run `just land` now.\n\n```sh\ngit fetch\njust land\n```\n\nDone.";
    expect(blocksOf(answer)).toEqual([
      paragraph(text("Run "), { kind: "code", text: "just land" }, text(" now.")),
      { kind: "code", text: "git fetch\njust land", lang: "sh", open: false },
      paragraph(text("Done.")),
    ]);
  });

  it("shows a code block that is still arriving", () => {
    expect(blocksOf("Try:\n\n```\ngit fe")).toEqual([
      paragraph(text("Try:")),
      { kind: "code", text: "git fe", lang: "", open: true },
    ]);
  });

  it("reads the language named on a fence, in lowercase and without its extra words", () => {
    expect(blocksOf("```TypeScript title=a.ts\nlet a = 1\n```")).toEqual([
      { kind: "code", text: "let a = 1", lang: "typescript", open: false },
    ]);
    expect(blocksOf("~~~ py\nx = 1\n~~~")).toEqual([
      { kind: "code", text: "x = 1", lang: "py", open: false },
    ]);
  });

  it("keeps the lines of a paragraph", () => {
    expect(blocksOf("one\ntwo")).toEqual([paragraph(text("one\ntwo"))]);
  });

  it("reads bulleted and numbered lists, with one level of nesting", () => {
    const answer = "Steps:\n1. Fetch\n2. Land it\n   - rebases\n   - pushes\n3. Wait";
    expect(blocksOf(answer)).toEqual([
      paragraph(text("Steps:")),
      {
        kind: "list",
        ordered: true,
        start: 1,
        items: [
          { spans: [text("Fetch")], blocks: [] },
          {
            spans: [text("Land it")],
            blocks: [
              {
                kind: "list",
                ordered: false,
                start: 1,
                items: [
                  { spans: [text("rebases")], blocks: [] },
                  { spans: [text("pushes")], blocks: [] },
                ],
              },
            ],
          },
          { spans: [text("Wait")], blocks: [] },
        ],
      },
    ]);
  });

  it("keeps a list going across blank lines and lines that wrap", () => {
    const [list, after] = blocksOf("- one\n  more\n\n- two\n\nAfter.");
    expect(list).toEqual({
      kind: "list",
      ordered: false,
      start: 1,
      items: [
        { spans: [text("one\nmore")], blocks: [] },
        { spans: [text("two")], blocks: [] },
      ],
    });
    expect(after).toEqual(paragraph(text("After.")));
  });

  it("lets only a list starting at 1 break into a paragraph", () => {
    expect(blocksOf("In the year\n2024. It happened")).toEqual([
      paragraph(text("In the year\n2024. It happened")),
    ]);
    expect(blocksOf("Steps:\n1. one")[1]).toMatchObject({ kind: "list", ordered: true });
  });

  it("starts a numbered list where the answer starts it", () => {
    expect(blocksOf("3. three\n4. four")[0]).toMatchObject({ ordered: true, start: 3 });
  });

  it("reads headings as one line and block quotes as their own blocks", () => {
    expect(blocksOf("## Rates ##\n> quoted **line**\n> more")).toEqual([
      { kind: "heading", spans: [text("Rates")] },
      {
        kind: "quote",
        blocks: [
          paragraph(text("quoted "), { kind: "strong", spans: [text("line")] }, text("\nmore")),
        ],
      },
    ]);
    expect(blocksOf("#hashtag")).toEqual([paragraph(text("#hashtag"))]);
  });

  it("drops horizontal rules", () => {
    expect(blocksOf("one\n\n---\n\ntwo")).toEqual([paragraph(text("one")), paragraph(text("two"))]);
  });

  it("shows raw HTML as text", () => {
    expect(blocksOf('<b>hi</b> <a href="javascript:x">x</a>')).toEqual([
      paragraph(text('<b>hi</b> <a href="javascript:x">x</a>')),
    ]);
  });
});

describe("answer spans", () => {
  it("reads bold, italic and both", () => {
    expect(spansOf("**bold** and *it* and __b__ and _i_")).toEqual([
      { kind: "strong", spans: [text("bold")] },
      text(" and "),
      { kind: "em", spans: [text("it")] },
      text(" and "),
      { kind: "strong", spans: [text("b")] },
      text(" and "),
      { kind: "em", spans: [text("i")] },
    ]);
    expect(spansOf("***both***")).toEqual([
      { kind: "strong", spans: [{ kind: "em", spans: [text("both")] }] },
    ]);
    expect(spansOf("*a **b** c*")).toEqual([
      { kind: "em", spans: [text("a "), { kind: "strong", spans: [text("b")] }, text(" c")] },
    ]);
  });

  it("leaves asterisks and underscores that mark nothing", () => {
    expect(spansOf("2 * 3 * 4")).toEqual([text("2 * 3 * 4")]);
    expect(spansOf("snake_case_name")).toEqual([text("snake_case_name")]);
    expect(spansOf("\\*not\\*")).toEqual([text("*not*")]);
    expect(spansOf("Multiply 4*5, then read *this* note.")).toEqual([
      text("Multiply 4*5, then read "),
      { kind: "em", spans: [text("this")] },
      text(" note."),
    ]);
    expect(spansOf("`a*b` *c*")).toEqual([
      { kind: "code", text: "a*b" },
      text(" "),
      { kind: "em", spans: [text("c")] },
    ]);
  });

  it("reads links, keeping parentheses in the URL", () => {
    expect(spansOf("[Ria](https://www.riamoneytransfer.com/en-mx/rates-conversion/)")).toEqual([
      {
        kind: "link",
        url: "https://www.riamoneytransfer.com/en-mx/rates-conversion/",
        href: "https://www.riamoneytransfer.com/en-mx/rates-conversion/",
        spans: [text("Ria")],
      },
    ]);
    expect(spansOf("[Foo](https://en.wikipedia.org/wiki/Foo_(bar)).")).toEqual([
      {
        kind: "link",
        url: "https://en.wikipedia.org/wiki/Foo_(bar)",
        href: "https://en.wikipedia.org/wiki/Foo_(bar)",
        spans: [text("Foo")],
      },
      text("."),
    ]);
    expect(spansOf('[**x**](https://a.example "title")')).toEqual([
      {
        kind: "link",
        url: "https://a.example",
        href: "https://a.example/",
        spans: [{ kind: "strong", spans: [text("x")] }],
      },
    ]);
  });

  it("links bare and bracketed web addresses", () => {
    expect(spansOf("See https://a.example/x_(y). Or <http://b.example>")).toEqual([
      text("See "),
      {
        kind: "link",
        url: "https://a.example/x_(y)",
        href: "https://a.example/x_(y)",
        spans: [text("https://a.example/x_(y)")],
      },
      text(". Or "),
      {
        kind: "link",
        url: "http://b.example",
        href: "http://b.example/",
        spans: [text("http://b.example")],
      },
    ]);
    expect(spansOf("(see https://a.example)")).toEqual([
      text("(see "),
      {
        kind: "link",
        url: "https://a.example",
        href: "https://a.example/",
        spans: [text("https://a.example")],
      },
      text(")"),
    ]);
  });

  it("shows links to anything but the web as their text alone", () => {
    expect(spansOf("[run](javascript:alert(1)) [file](file:///etc/passwd) [rel](/docs)")).toEqual([
      text("run file rel"),
    ]);
    expect(spansOf("[x](JavaScript:alert(1))")).toEqual([text("x")]);
    expect(spansOf("<javascript:alert(1)>")).toEqual([text("<javascript:alert(1)>")]);
    expect(webLink("data:text/html,hi")).toBeNull();
    expect(webLink(" https://a.example ")).toBe("https://a.example/");
  });
});

describe("answers while they stream", () => {
  it("show a half-arrived link or bold as plain text until it closes", () => {
    expect(blocksOf("Rate: [Ria](https://www.ria")).toEqual([
      paragraph(text("Rate: [Ria](https://www.ria")),
    ]);
    expect(blocksOf("Rate: [Ria")).toEqual([paragraph(text("Rate: [Ria"))]);
    expect(blocksOf("It is **very")).toEqual([paragraph(text("It is **very"))]);
    expect(blocksOf("It is **")).toEqual([paragraph(text("It is **"))]);
    expect(blocksOf("Run `just")).toEqual([paragraph(text("Run `just"))]);
  });

  it("read every prefix of an answer without breaking", () => {
    const answer =
      "## Rate\n\n1 USD ≈ **19.45 MXN** at [Ria](https://ria.example/a_(b)).\n\n- one *x*\n  - two\n> q\n\n```\ncode\n```";
    for (let end = 0; end <= answer.length; end++) {
      expect(() => blocksOf(answer.slice(0, end))).not.toThrow();
    }
    expect(blocksOf(answer).map((block) => block.kind)).toEqual([
      "heading",
      "paragraph",
      "list",
      "quote",
      "code",
    ]);
  });
});

describe("answers as they grow", () => {
  it("keep the blocks that did not change, so only the last one is drawn again", () => {
    const before = blocksOf("- one\n- two\n\nThree");
    const after = blocksAfter(before, "- one\n- two\n\nThree and four");
    expect(after[0]).toBe(before[0]);
    expect(after[1]).not.toBe(before[1]);
    expect(after[1]).toEqual(paragraph(text("Three and four")));
    expect(blocksAfter(after, "- one\n- two\n\nThree and four")).toBe(after);
  });
});

describe("odd answers", () => {
  it("keep reading past a backtick that never closes", () => {
    expect(
      spansOf("Press ⌘` to cycle, see [Apple](https://support.apple.com) and **this**"),
    ).toEqual([
      text("Press ⌘` to cycle, see "),
      {
        kind: "link",
        url: "https://support.apple.com",
        href: "https://support.apple.com/",
        spans: [text("Apple")],
      },
      text(" and "),
      { kind: "strong", spans: [text("this")] },
    ]);
  });

  it("read a list item with very many lines", () => {
    expect(() => blocksOf(`- a\n${"  b\n".repeat(150000)}`)).not.toThrow();
  });

  it("read emphasis the same after a link holding a backtick", () => {
    expect(spansOf("_a [x](https://e.example/`) (_b_) `")).toContainEqual({
      kind: "em",
      spans: [text("b")],
    });
  });

  it("read a web address with nothing after its scheme as text", () => {
    expect(spansOf("see http:// now")).toEqual([text("see http:// now")]);
    expect(spansOf("http://")).toEqual([text("http://")]);
    expect(spansOf("https://<your-domain>/x")).toEqual([text("https://<your-domain>/x")]);
  });

  it("stop nesting quotes and lists after a few levels", () => {
    expect(() => blocksOf(">".repeat(16000))).not.toThrow();
    expect(() => blocksOf("> - > - > - > - > - x")).not.toThrow();
  });

  it("stay quick to read", () => {
    const started = performance.now();
    blocksOf("*a [b ".repeat(2000));
    blocksOf("*[a](b) ".repeat(2000));
    blocksOf("_x [y](https://a.example/z) ".repeat(2000));
    blocksOf("[`".repeat(8000));
    blocksOf("[".repeat(8000));
    blocksOf("[a](".repeat(5000));
    blocksOf("![a](".repeat(5000));
    blocksOf("[a](x".repeat(5000));
    blocksOf('[a](<x "'.repeat(5000));
    blocksOf(("http://[".repeat(100) + "]".repeat(1200) + " ").repeat(10));
    blocksOf(`para\n# a${" ".repeat(20000)}b`);
    blocksOf("._a".repeat(17000));
    blocksOf("[\\``".repeat(5000));
    blocksOf("<h\\``[".repeat(8000));
    blocksOf("http://[".repeat(6000));
    blocksOf("(_build/_static/_x) and /_next/_app ".repeat(1400));
    blocksOf("._a ``` ".repeat(6000));
    blocksOf(`\` ${" ".repeat(3000)} \``.repeat(5));
    expect(performance.now() - started).toBeLessThan(1000);
  });
});

describe("answers as plain text", () => {
  it("writes links as text and URL, without asterisks or heading hashes", () => {
    expect(
      plainOf(
        "## Rate\n\n1 USD ≈ **19.45 MXN** at Ria's listed *mid-market* rate. [Ria](https://www.riamoneytransfer.com/en-mx/rates-conversion/)",
      ),
    ).toBe(
      "Rate\n\n1 USD ≈ 19.45 MXN at Ria's listed mid-market rate. Ria (https://www.riamoneytransfer.com/en-mx/rates-conversion/)",
    );
  });

  it("keeps list markers, nesting, inline code and code blocks", () => {
    expect(
      plainOf(
        "Steps:\n\n* Fetch\n* Land `it`\n  * rebases\n\n1. one\n2. two\n\n```sh\njust land\n```",
      ),
    ).toBe("Steps:\n\n- Fetch\n- Land `it`\n  - rebases\n\n1. one\n2. two\n\njust land");
  });

  it("writes a bare link once and quotes with their marker", () => {
    expect(plainOf("See https://a.example.\n\n> quoted\n>\n> more")).toBe(
      "See https://a.example.\n\n> quoted\n>\n> more",
    );
  });
});

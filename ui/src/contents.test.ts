import { describe, expect, it } from "vitest";
import {
  contentPlace,
  contentQuery,
  startsContent,
  stillContaining,
  type ContentRow,
} from "./contents";

const row = (text: string, line = 1): ContentRow => ({
  name: "notes.md",
  path: "/Users/me/Documents/notes.md",
  folder: "~/Documents",
  icon: ".md",
  line,
  text,
});

describe("text in files", () => {
  it("searches text when the query starts with a double quote", () => {
    expect(contentQuery('"quarterly plan')).toBe("quarterly plan");
    expect(contentQuery('  "fn main')).toBe("fn main");
    expect(contentQuery("“TODO")).toBe("TODO");
  });

  it("drops a closing quote", () => {
    expect(contentQuery('"quarterly plan"')).toBe("quarterly plan");
    expect(contentQuery("“due Friday”")).toBe("due Friday");
    expect(contentQuery('" plan "')).toBe("plan");
    expect(contentQuery('"quarterly plan" ')).toBe("quarterly plan");
    expect(contentQuery(" “due Friday”  ")).toBe("due Friday");
  });

  it("waits for 3 letters after the quote", () => {
    expect(contentQuery('"')).toBeNull();
    expect(contentQuery('"ab')).toBeNull();
    expect(contentQuery('"ab"')).toBeNull();
    expect(contentQuery('"  ab ')).toBeNull();
    expect(contentQuery('"abc')).toBe("abc");
    expect(contentQuery('"😀😀😀')).toBe("😀😀😀");
  });

  it("keeps a short quoted query away from the name search", () => {
    expect(startsContent('"pl')).toBe(true);
    expect(startsContent(' "')).toBe(true);
    expect(startsContent("plan")).toBe(false);
  });

  it("leaves a plain query to the name search", () => {
    expect(contentQuery("quarterly plan")).toBeNull();
    expect(contentQuery('plan"')).toBeNull();
    expect(contentQuery("")).toBeNull();
  });

  it("places a line after the folder", () => {
    expect(contentPlace(row("The quarterly plan", 3))).toBe("~/Documents:3");
  });

  it("keeps earlier lines only while they still hold the exact phrase, as the search does", () => {
    const shown = [row("The quarterly plan is due Friday"), row("The quarterly release", 2)];
    expect(stillContaining(shown, "quarterly PLAN").map((kept) => kept.line)).toEqual([1]);
    expect(stillContaining([row("Résumé draft")], "resume").length).toBe(0);
    expect(stillContaining([row("the plan is reviewed quarterly")], "quarterly plan")).toEqual([]);
    expect(stillContaining(shown, "zzz")).toEqual([]);
  });
});

import { describe, expect, it } from "vitest";
import { searchesFiles, stillMatching, wordPositions, type FileRow } from "./files";

const file = (name: string): FileRow => ({
  name,
  path: `/Users/me/${name}`,
  folder: "~",
  icon: "",
});

describe("file search", () => {
  it("waits for a word of 3 letters", () => {
    expect(searchesFiles("ab")).toBe(false);
    expect(searchesFiles(" a b ")).toBe(false);
    expect(searchesFiles("a b c")).toBe(false);
    expect(searchesFiles("abc")).toBe(true);
    expect(searchesFiles("a bcd")).toBe(true);
  });

  it("marks where each typed word sits in the name", () => {
    expect(wordPositions("Release notes.pdf", "notes")).toEqual([8, 9, 10, 11, 12]);
    expect(wordPositions("AGENTS.md", "agents.md")).toEqual([0, 1, 2, 3, 4, 5, 6, 7, 8]);
    expect(wordPositions("Meeting notes", "not meet")).toEqual([0, 1, 2, 3, 8, 9, 10]);
    expect(wordPositions("notes.md", "zzz")).toEqual([]);
  });

  it("counts letters, not UTF-16 units", () => {
    expect(wordPositions("😀 notes", "notes")).toEqual([2, 3, 4, 5, 6]);
  });

  it("ignores case and accents as Spotlight does", () => {
    expect(wordPositions("Résumé 2026.pdf", "resume")).toEqual([0, 1, 2, 3, 4, 5]);
    expect(wordPositions("resume.txt", "RÉSUMÉ")).toEqual([0, 1, 2, 3, 4, 5]);
    expect(wordPositions("Résumé.pdf".normalize("NFD"), "sum")).toEqual([2, 3, 4]);
    expect(stillMatching([file("Résumé.pdf")], "resume").map((kept) => kept.name)).toEqual([
      "Résumé.pdf",
    ]);
  });

  it("keeps earlier files that still match while the new search runs", () => {
    const shown = [file("notes.md"), file("Release notes.pdf"), file("Meeting notes")];
    expect(stillMatching(shown, "notes.m").map((kept) => kept.name)).toEqual(["notes.md"]);
    expect(stillMatching(shown, "NOTES meet").map((kept) => kept.name)).toEqual(["Meeting notes"]);
    expect(stillMatching(shown, "zzz")).toEqual([]);
  });
});

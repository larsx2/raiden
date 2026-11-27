import { describe, expect, it } from "vitest";
import {
  pieces,
  sectionsOf,
  slotAt,
  slotCount,
  slotOf,
  startingRows,
  startsSection,
  clamp,
} from "./list";

describe("the list's sections", () => {
  it("shows Suggestions over the most-used apps and Applications over the rest", () => {
    const sections = sectionsOf([
      ["Suggestions", 3],
      ["Applications", 7],
    ]);
    expect(sections).toEqual([
      { name: "Suggestions", start: 0 },
      { name: "Applications", start: 3 },
    ]);
    expect(slotCount(sections, 10)).toBe(12);
    const slots = Array.from({ length: 12 }, (_, slot) => slotAt(sections, slot));
    expect(slots).toEqual([
      { heading: "Suggestions" },
      { row: 0 },
      { row: 1 },
      { row: 2 },
      { heading: "Applications" },
      ...Array.from({ length: 7 }, (_, row) => ({ row: row + 3 })),
    ]);
    expect(Array.from({ length: 10 }, (_, row) => slotOf(sections, row))).toEqual([
      1, 2, 3, 5, 6, 7, 8, 9, 10, 11,
    ]);
    expect([0, 1, 3].map((row) => startsSection(sections, row))).toEqual([true, false, true]);
  });

  it("leaves out empty groups, and has no sections for no rows", () => {
    expect(
      sectionsOf([
        ["Results", 1],
        ["Recent answers", 0],
        ["Use “saf” with", 1],
      ]),
    ).toEqual([
      { name: "Results", start: 0 },
      { name: "Use “saf” with", start: 1 },
    ]);
    expect(
      sectionsOf([
        ["Suggestions", 0],
        ["Applications", 4],
      ]),
    ).toEqual([{ name: "Applications", start: 0 }]);
    expect(sectionsOf([["Results", 0]])).toEqual([]);
    expect(slotAt([], 2)).toEqual({ row: 2 });
    expect(slotOf([], 2)).toBe(2);
  });
});

describe("the starting rows", () => {
  it("puts the suggestions first and keeps the rest in listing order", () => {
    expect([...startingRows(6, [4, 1])]).toEqual([4, 1, 0, 2, 3, 5]);
    expect([...startingRows(3, [])]).toEqual([0, 1, 2]);
  });
});

describe("stopping at the ends", () => {
  it("stays on the first or last row instead of going round", () => {
    expect(clamp(5, 5)).toBe(4);
    expect(clamp(-1, 5)).toBe(0);
    expect(clamp(2, 5)).toBe(2);
  });
});

describe("matched letters", () => {
  it("joins neighbouring letters into pieces, counting letters rather than code units", () => {
    expect(pieces("Safari", [0, 1, 2])).toEqual([
      { text: "Saf", matched: true },
      { text: "ari", matched: false },
    ]);
    expect(pieces("System Settings", [0, 7])).toEqual([
      { text: "S", matched: true },
      { text: "ystem ", matched: false },
      { text: "S", matched: true },
      { text: "ettings", matched: false },
    ]);
    expect(pieces("😀 Zed", [2])).toEqual([
      { text: "😀 ", matched: false },
      { text: "Z", matched: true },
      { text: "ed", matched: false },
    ]);
    expect(pieces("Notes", [])).toEqual([{ text: "Notes", matched: false }]);
  });
});

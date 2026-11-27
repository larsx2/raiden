import { describe, expect, it } from "vitest";
import { dayOf, filterClips, sectionsOf, subtitleOf, type Clip } from "./clipboard";

const NOW = new Date(2026, 10, 25, 15, 0).getTime() / 1000;
const HOUR = 3600;

const clip = (id: string, hoursAgo: number, more: Partial<Clip> = {}): Clip => ({
  id,
  kind: "text",
  title: id,
  text: id,
  app: "Terminal",
  at: NOW - hoursAgo * HOUR,
  width: 0,
  height: 0,
  thumb: false,
  ...more,
});

describe("clipboard items", () => {
  it("say what they are, where they came from and how old they are", () => {
    expect(subtitleOf(clip("a", 0), NOW)).toBe("Text from Terminal, just now");
    expect(subtitleOf(clip("b", 0.5, { kind: "link", app: "Safari" }), NOW)).toBe(
      "Link from Safari, 30 min ago",
    );
    expect(subtitleOf(clip("c", 26, { kind: "image", app: "" }), NOW)).toBe("Image, yesterday");
  });

  it("are grouped by the day they were copied", () => {
    expect(dayOf(NOW - 14 * HOUR, NOW)).toBe("Today");
    expect(dayOf(NOW - 16 * HOUR, NOW)).toBe("Yesterday");
    expect(dayOf(NOW - 40 * HOUR, NOW)).toBe("Earlier");
    const days = sectionsOf([clip("a", 1), clip("b", 2), clip("c", 30), clip("d", 80)], NOW);
    expect(days.map(([day, clips]) => [day, clips.map((one) => one.id)])).toEqual([
      ["Today", ["a", "b"]],
      ["Yesterday", ["c"]],
      ["Earlier", ["d"]],
    ]);
  });

  it("are found by any part of their text, app or kind", () => {
    const clips = [
      clip("just land", 1),
      clip("https://gpui.rs/", 2, { kind: "link", app: "Safari" }),
    ];
    const found = (query: string) => filterClips(clips, query).map((one) => one.id);
    expect(found("")).toEqual(["just land", "https://gpui.rs/"]);
    expect(found("gpui")).toEqual(["https://gpui.rs/"]);
    expect(found("LAND term")).toEqual(["just land"]);
    expect(found("link")).toEqual(["https://gpui.rs/"]);
    expect(found("zzzz")).toEqual([]);
  });
});

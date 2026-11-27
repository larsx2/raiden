import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { createSearch } from "./search";

const bytes = () => readFileSync(join(process.cwd(), "src", "generated", "search.wasm"));

describe("search compiled from raiden-core", () => {
  it("finds Safari for saf and nothing for sen in System Settings", async () => {
    const search = await createSearch(bytes());
    expect(search.load("Safari\nSystem Settings\nSlack\n\n")).toBe(3);
    expect([...search.rank("saf")]).toEqual([0]);
    expect([...search.rank("sen")]).toEqual([]);
  });

  it("marks the letters of System Settings that sys set matched", async () => {
    const search = await createSearch(bytes());
    search.load("Safari\nSystem Settings\n\n");
    expect([...search.positions(1, "sys set")]).toEqual([0, 1, 2, 7, 8, 9]);
    expect([...search.positions(0, "sen")]).toEqual([]);
  });

  it("suggests the most-used apps first, and only used ones", async () => {
    const search = await createSearch(bytes());
    search.load("Safari\nSlack\nZed\n\n2\t1\t\n2\t1\tz\n0\t1\t\n");
    expect([...search.suggestions(5)]).toEqual([2, 0]);
    expect([...search.suggestions(1)]).toEqual([2]);
  });

  it.runIf(process.platform === "darwin")(
    "ranks 5,000 apps in under 2 ms on this Mac",
    async () => {
      const search = await createSearch(bytes());
      const names = Array.from({ length: 5000 }, (_, index) => `Application ${index} Studio`);
      search.load(`${names.join("\n")}\n\n`);
      search.rank("as");
      const times = Array.from({ length: 20 }, () => {
        const started = performance.now();
        search.rank("aps");
        return performance.now() - started;
      }).sort((a, b) => a - b);
      expect(times[10]).toBeLessThan(2);
    },
  );
});

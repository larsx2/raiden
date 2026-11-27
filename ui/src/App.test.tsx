import { render } from "solid-js/web";
import { describe, expect, it } from "vitest";
import { App, SEARCH_PLACEHOLDER, type Call, type Listing } from "./App";
import type { Search } from "./search";

describe("App", () => {
  it("shows the search field with its placeholder and asks for the apps", async () => {
    const asked: string[] = [];
    const call: Call = async <T,>(command: string) => {
      asked.push(command);
      const listing: Listing = {
        apps: [{ name: "Safari", path: "/Applications/Safari.app" }],
        catalog: "Safari\n\n",
      };
      return (command === "apps" ? listing : null) as T;
    };
    const root = document.createElement("main");
    document.body.append(root);
    const search = async (): Promise<Search> => ({
      load: () => 1,
      rank: () => new Uint32Array([0]),
      positions: () => new Uint32Array([0]),
      suggestions: () => new Uint32Array(0),
    });
    const dispose = render(
      () => (
        <App
          call={call}
          search={search}
          listen={async () => () => {}}
          hear={async () => () => {}}
        />
      ),
      root,
    );
    const field = root.querySelector("input");
    expect(field?.placeholder).toBe(SEARCH_PLACEHOLDER);
    await Promise.resolve();
    expect(asked).toContain("apps");
    dispose();
  });
});

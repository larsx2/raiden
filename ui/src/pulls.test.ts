import { createRoot } from "solid-js";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import fixture from "./pulls.fixture.json";
import {
  checkLines,
  clockOf,
  compactAge,
  countsOf,
  createPulls,
  emptyText,
  keptSelection,
  labelOf,
  matches,
  outsideCount,
  problemAfter,
  problemLine,
  quickFilterKeys,
  repoChoices,
  shownOf,
  updatedAgo,
  type Pull,
  type PullsView,
} from "./pulls";

const NOW = new Date(2026, 10, 25, 15, 0).getTime() / 1000;
const HOUR = 3600;

const PULLS: Pull[] = fixture.pulls.map(({ hoursAgo, ...pull }) => ({
  ...pull,
  checks: pull.checks as Pull["checks"],
  look: pull.look as Pull["look"],
  updatedAt: NOW - hoursAgo * HOUR,
}));

const viewOf = (more: Partial<PullsView> = {}): PullsView => ({
  allowed: true,
  login: fixture.login,
  pulls: PULLS,
  fetchedAt: NOW - fixture.fetchedSecondsAgo,
  reviewRequested: fixture.reviewRequested,
  owners: fixture.owners,
  pinned: fixture.pinned,
  lost: fixture.lost,
  refreshing: false,
  problem: null,
  ...more,
});

const numbers = (pulls: Pull[]) => pulls.map((pull) => pull.number);
const pull = (number: number) => PULLS.find((one) => one.number === number) as Pull;

describe("the tabs", () => {
  it("count every open pull request, or only those inside the filter", () => {
    expect(countsOf(PULLS, null)).toEqual({ all: 13, failing: 4, ready: 5, drafts: 3 });
    expect(countsOf(PULLS, "fernhill/beacon")).toEqual({
      all: 5,
      failing: 3,
      ready: 0,
      drafts: 3,
    });
    expect(countsOf(PULLS, "fernhill").all).toBe(10);
  });

  it("show the pull requests in a state, newest first as they came", () => {
    expect(numbers(shownOf(PULLS, "failing", null, ""))).toEqual([169, 168, 937, 163]);
    expect(numbers(shownOf(PULLS, "drafts", "fernhill/beacon", ""))).toEqual([169, 168, 122]);
  });
});

describe("typing", () => {
  it("matches the title, the shown repo, owner/repo, the number and the branch", () => {
    expect(matches(pull(169), "retry budget")).toBe(true);
    expect(matches(pull(169), "beacon")).toBe(true);
    expect(matches(pull(41), "tidewater/lantern")).toBe(true);
    expect(matches(pull(169), "169")).toBe(true);
    expect(matches(pull(169), "#169")).toBe(true);
    expect(matches(pull(169), "#168")).toBe(false);
    expect(matches(pull(164), "sealed-settings")).toBe(true);
    expect(matches(pull(169), "zzz")).toBe(false);
  });

  it("counts what matches outside the filter in the same tab", () => {
    expect(shownOf(PULLS, "all", "fernhill/beacon", "release")).toEqual([]);
    expect(outsideCount(PULLS, "all", "fernhill/beacon", "release")).toBe(2);
    expect(outsideCount(PULLS, "all", null, "release")).toBe(0);
  });
});

describe("words on screen", () => {
  it("name a filter by its shown repo or its owner", () => {
    expect(labelOf("fernhill/beacon", PULLS)).toBe("beacon");
    expect(labelOf("tidewater/lantern", PULLS)).toBe("tidewater/lantern");
    expect(labelOf("fernhill", PULLS)).toBe("fernhill");
    expect(labelOf("gone/away", PULLS)).toBe("gone/away");
  });

  it("give ages compactly in rows and in words in the detail", () => {
    expect(compactAge(NOW - 30, NOW)).toBe("now");
    expect(compactAge(NOW - 5 * 60, NOW)).toBe("5m");
    expect(compactAge(NOW - 3 * HOUR, NOW)).toBe("3h");
    expect(compactAge(NOW - 26 * HOUR, NOW)).toBe("1d");
    expect(updatedAgo(NOW - 30, NOW)).toBe("updated just now");
    expect(updatedAgo(NOW - 26 * HOUR, NOW)).toBe("updated 1d ago");
  });

  it("say an empty tab is empty, inside the filter when there is one", () => {
    expect(emptyText("failing", "beacon")).toBe("Nothing failing in beacon.");
    expect(emptyText("failing", null)).toBe("Nothing failing.");
    expect(emptyText("ready", "beacon")).toBe("Nothing ready to merge in beacon.");
    expect(emptyText("drafts", null)).toBe("No drafts.");
    expect(emptyText("all", null)).toBe("No open pull requests.");
  });

  it("list failing and running checks, then how many passed", () => {
    expect(checkLines(pull(169))).toEqual([
      { look: "fail", text: "test" },
      { look: "pass", text: "and 2 passed" },
    ]);
    expect(checkLines(pull(64))).toEqual([{ look: "pass", text: "2 passed" }]);
    expect(checkLines(pull(164))).toEqual([
      { look: "run", text: "test" },
      { look: "pass", text: "and 3 passed" },
    ]);
    expect(checkLines(pull(12))).toEqual([]);
  });
});

describe("narrowing", () => {
  it("offers repositories with the most open first", () => {
    expect(repoChoices(PULLS).slice(0, 3)).toEqual([
      { repo: "fernhill/beacon", shown: "beacon", open: 5 },
      { repo: "fernhill/harbor", shown: "harbor", open: 2 },
      { repo: "fernhill/ledger", shown: "ledger", open: 2 },
    ]);
  });

  it("gives pinned repos ⌘5 to ⌘9", () => {
    expect(quickFilterKeys(0)).toBe("⌘ 5");
    expect(quickFilterKeys(4)).toBe("⌘ 9");
    expect(quickFilterKeys(5)).toBe("");
  });
});

describe("selection", () => {
  it("stays on the same pull request when the list changes, else at the same place", () => {
    const before = shownOf(PULLS, "all", null, "");
    const after = before.filter((one) => one.number !== 64);
    expect(keptSelection(pull(193), 2, after)).toBe(1);
    expect(keptSelection(pull(64), 1, after)).toBe(1);
    expect(keptSelection(pull(64), 40, after)).toBe(after.length - 1);
    expect(keptSelection(undefined, 3, [])).toBe(0);
  });
});

describe("the view", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(NOW * 1000);
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  const start = (answers: Record<string, (args?: Record<string, unknown>) => unknown>) => {
    const sent: Array<[string, Record<string, unknown> | undefined]> = [];
    const call = async <T>(command: string, args?: Record<string, unknown>) => {
      sent.push([command, args]);
      const answer = answers[command];
      return (answer ? answer(args) : null) as T;
    };
    let dispose = () => undefined as void;
    const pulls = createRoot((done) => {
      dispose = done;
      return createPulls({ call });
    });
    return { pulls, sent, dispose };
  };

  it("opens from the cache at once, then refreshes behind it and keeps the selection", async () => {
    const refreshed = viewOf({
      pulls: PULLS.filter((one) => one.number !== 169),
      fetchedAt: NOW + 1,
    });
    const { pulls, sent, dispose } = start({
      pull_requests: () => viewOf({ refreshing: true, fetchedAt: NOW - 4 * 60 }),
      pull_requests_refresh: () =>
        new Promise((resolve) => setTimeout(() => resolve(refreshed), 1000)),
    });
    await pulls.show();
    expect(pulls.context()).toBe("");
    pulls.move(2);
    expect(pulls.selectedPull()?.number).toBe(193);
    expect(sent.map(([command, args]) => [command, args])).toEqual([
      ["pull_requests", undefined],
      ["pull_requests_refresh", { force: false }],
    ]);
    await vi.advanceTimersByTimeAsync(1000);
    expect(pulls.shown()).toHaveLength(12);
    expect(pulls.selectedPull()?.number).toBe(193);
    expect(pulls.context()).toBe("");
    dispose();
  });

  it("reads the first list while there is none saved, then shows it", async () => {
    const { pulls, dispose } = start({
      pull_requests: () => viewOf({ pulls: [], fetchedAt: null }),
      pull_requests_refresh: () =>
        new Promise((resolve) => setTimeout(() => resolve(viewOf({ fetchedAt: NOW })), 1000)),
    });
    await pulls.show();
    expect(pulls.reading()).toBe(true);
    expect(pulls.shown()).toEqual([]);
    await vi.advanceTimersByTimeAsync(1000);
    expect(pulls.reading()).toBe(false);
    expect(pulls.shown()).toHaveLength(13);
    dispose();
  });

  it("drops an Ask AI reply when the tab, filter or typed text changed, or the view reopened, while it waited", async () => {
    const later = <T>(value: T) =>
      new Promise<T>((resolve) => setTimeout(() => resolve(value), 1000));
    const { pulls, dispose } = start({
      pull_requests: () => viewOf(),
      pull_requests_refresh: () => viewOf(),
      pull_requests_context: () => later({ label: "Open pull requests", text: "" }),
    });
    await pulls.show();
    const changes = [
      () => pulls.showTab("failing"),
      () => pulls.narrow("fernhill"),
      () => pulls.filter("beacon"),
      () => {
        pulls.hide();
        void pulls.show();
      },
    ];
    for (const change of changes) {
      const reply = pulls.ask();
      change();
      await vi.advanceTimersByTimeAsync(1000);
      expect(await reply).toBe(null);
    }
    const kept = pulls.ask();
    await vi.advanceTimersByTimeAsync(1000);
    expect(await kept).toEqual({ label: "Open pull requests", text: "" });
    dispose();
  });

  it("shows an error from opening only while that opening is the current one", async () => {
    let openings = 0;
    const { pulls, dispose } = start({
      pull_requests: () =>
        ++openings === 1
          ? new Promise((_, reject) => setTimeout(() => reject(new Error("gh is slow")), 1000))
          : viewOf(),
      pull_requests_refresh: () => viewOf(),
    });
    const first = pulls.show();
    pulls.hide();
    await pulls.show();
    await vi.advanceTimersByTimeAsync(1000);
    await first;
    expect(pulls.context()).toBe("");
    dispose();
  });

  const slowThenFast = () => {
    let refreshes = 0;
    const older = viewOf({ pulls: PULLS.slice(0, 2), fetchedAt: NOW - 600 });
    const newer = viewOf({ fetchedAt: NOW });
    return {
      pull_requests: () => viewOf(),
      pull_requests_refresh: () =>
        ++refreshes === 1
          ? new Promise((resolve) => setTimeout(() => resolve(older), 2000))
          : new Promise((resolve) => setTimeout(() => resolve(newer), 100)),
    };
  };

  it("drops an older, slower refresh once ⌘R started a newer one", async () => {
    const { pulls, dispose } = start(slowThenFast());
    await pulls.show();
    expect(pulls.shortcut("⌘ R")).toBe(true);
    await vi.advanceTimersByTimeAsync(2000);
    expect(pulls.shown()).toHaveLength(13);
    expect(pulls.view()?.fetchedAt).toBe(NOW);
    dispose();
  });

  it("drops a refresh from before the view was closed and opened again", async () => {
    const { pulls, dispose } = start(slowThenFast());
    await pulls.show();
    pulls.hide();
    await pulls.show();
    await vi.advanceTimersByTimeAsync(2000);
    expect(pulls.shown()).toHaveLength(13);
    expect(pulls.view()?.fetchedAt).toBe(NOW);
    dispose();
  });

  it("tries again at the time GitHub gives while the view is open", async () => {
    const limited = viewOf({
      problem: {
        kind: "limited",
        text: "GitHub's limit on requests is used up for now.",
        retryAt: NOW + 300,
      },
    });
    let refreshes = 0;
    const { pulls, sent, dispose } = start({
      pull_requests: () => viewOf(),
      pull_requests_refresh: () =>
        ++refreshes === 1 ? limited : viewOf({ fetchedAt: Date.now() / 1000 }),
    });
    await pulls.show();
    await vi.advanceTimersByTimeAsync(0);
    expect(pulls.context()).toBe("GitHub limit, retrying at 15:05.");
    await vi.advanceTimersByTimeAsync(300_000);
    expect(sent.filter(([command]) => command === "pull_requests_refresh").at(-1)?.[1]).toEqual({
      force: true,
    });
    expect(pulls.context()).toBe("");
    dispose();
  });

  it("asks first, and refreshes only once allowed", async () => {
    const { pulls, sent, dispose } = start({
      pull_requests: () => viewOf({ allowed: false, pulls: [], fetchedAt: null }),
      pull_requests_allow: () => viewOf(),
      pull_requests_refresh: () => viewOf(),
    });
    await pulls.show();
    expect(pulls.gate()).toBe("allow");
    expect(pulls.primary()?.title).toBe("Continue");
    expect(sent.map(([command]) => command)).toEqual(["pull_requests", "pull_requests_avatar"]);
    pulls.act();
    await vi.advanceTimersByTimeAsync(0);
    expect(pulls.gate()).toBe(null);
    expect(sent.map(([command]) => command)).toEqual([
      "pull_requests",
      "pull_requests_avatar",
      "pull_requests_allow",
      "pull_requests_refresh",
    ]);
    dispose();
  });

  it("shows the avatar for the signed-in login once it arrives, and asks once per opening", async () => {
    let answer: (image: string | null) => void = () => undefined;
    const { pulls, sent, dispose } = start({
      pull_requests: () => viewOf({ allowed: false, pulls: [], fetchedAt: null }),
      pull_requests_avatar: () =>
        new Promise((resolve) => {
          answer = resolve;
        }),
    });
    await pulls.show();
    expect(pulls.avatar()).toBe(null);
    answer("data:image/png;base64,AA==");
    await vi.advanceTimersByTimeAsync(0);
    expect(pulls.avatar()).toBe("data:image/png;base64,AA==");
    pulls.hide();
    await pulls.show();
    answer("https://avatars.githubusercontent.com/u/1");
    await vi.advanceTimersByTimeAsync(0);
    expect(pulls.avatar()).toBe("data:image/png;base64,AA==");
    expect(sent.filter(([command]) => command === "pull_requests_avatar")).toHaveLength(2);
    dispose();
  });

  const signedOut = viewOf({
    login: null,
    pulls: [],
    fetchedAt: null,
    problem: { kind: "signedOut", text: "gh is not signed in." },
  });

  it("copies gh auth login when gh is signed out", async () => {
    const { pulls, sent, dispose } = start({
      pull_requests: () => signedOut,
      pull_requests_refresh: () => signedOut,
    });
    await pulls.show();
    await vi.advanceTimersByTimeAsync(0);
    expect(pulls.gate()).toBe("signIn");
    pulls.act();
    await vi.advanceTimersByTimeAsync(0);
    expect(sent.at(-1)).toEqual(["copy_text", { text: "gh auth login" }]);
    dispose();
  });

  it("refreshes behind a saved sign-in problem, and ⌘R tries again from it", async () => {
    let signedIn = false;
    const { pulls, sent, dispose } = start({
      pull_requests: () => signedOut,
      pull_requests_refresh: () => (signedIn ? viewOf() : signedOut),
    });
    await pulls.show();
    await vi.advanceTimersByTimeAsync(0);
    expect(sent.map(([command]) => command)).toEqual(["pull_requests", "pull_requests_refresh"]);
    expect(pulls.gate()).toBe("signIn");
    signedIn = true;
    expect(pulls.shortcut("⌘ R")).toBe(true);
    await vi.advanceTimersByTimeAsync(0);
    expect(sent.at(-1)).toEqual(["pull_requests_refresh", { force: true }]);
    expect(pulls.gate()).toBe(null);
    expect(pulls.shown()).toHaveLength(13);
    dispose();
  });

  it("neither refreshes nor takes ⌘R on the first-use screen", async () => {
    const { pulls, sent, dispose } = start({
      pull_requests: () => ({ ...signedOut, allowed: false }),
    });
    await pulls.show();
    await vi.advanceTimersByTimeAsync(0);
    expect(pulls.shortcut("⌘ R")).toBe(false);
    expect(sent.map(([command]) => command)).toEqual(["pull_requests"]);
    dispose();
  });

  it("says why the first read failed, and ⌘R reads again", async () => {
    let reads = 0;
    const { pulls, sent, dispose } = start({
      pull_requests: () => {
        if (++reads === 1) {
          throw new Error("The saved list could not be read.");
        }
        return viewOf();
      },
      pull_requests_refresh: () => viewOf(),
    });
    await pulls.show({ tab: "failing" }, { query: "pull", row: 0 });
    expect(pulls.loaded()).toBe(false);
    expect(pulls.unread()).toBe("The saved list could not be read.");
    expect(pulls.context()).toBe("");
    expect(pulls.shortcut("⌘ R")).toBe(true);
    await vi.advanceTimersByTimeAsync(0);
    expect(pulls.unread()).toBe(null);
    expect(pulls.tab()).toBe("failing");
    expect(pulls.shown()).toHaveLength(4);
    expect(sent.filter(([command]) => command === "pull_requests")).toEqual([
      ["pull_requests", { query: "pull", row: 0 }],
      ["pull_requests", undefined],
    ]);
    dispose();
  });

  it("drops an answer to allowing or pinning that comes after the view was opened again", async () => {
    const later = <T>(value: T) =>
      new Promise<T>((resolve) => setTimeout(() => resolve(value), 500));
    let readings = 0;
    const { pulls, dispose } = start({
      pull_requests: () => (++readings === 1 ? viewOf({ allowed: false }) : viewOf()),
      pull_requests_allow: () => later(viewOf({ pinned: ["gone/allowed"] })),
      pull_requests_pin: () => later(["gone/pinned"]),
      pull_requests_refresh: () => viewOf(),
    });
    await pulls.show();
    pulls.act();
    pulls.hide();
    await pulls.show();
    await vi.advanceTimersByTimeAsync(0);
    pulls.shortcut("⌘ P");
    pulls.hide();
    await pulls.show();
    await vi.advanceTimersByTimeAsync(1000);
    expect(pulls.view()?.pinned).toEqual(fixture.pinned);
    dispose();
  });

  it("narrows, pins and switches tabs with its keys, never changing the tab when typing", async () => {
    const { pulls, sent, dispose } = start({
      pull_requests: () => viewOf(),
      pull_requests_refresh: () => viewOf(),
      pull_requests_pin: (args) => [...fixture.pinned, String(args?.["repo"])],
    });
    await pulls.show({ tab: "failing" });
    pulls.filter("beacon");
    expect(pulls.tab()).toBe("failing");
    expect(numbers(pulls.shown())).toEqual([169, 168, 163]);
    expect(pulls.shortcut("⌘ 3")).toBe(true);
    expect(pulls.tab()).toBe("ready");
    expect(pulls.shortcut("⌘ 5")).toBe(true);
    expect(pulls.only()).toBe("fernhill/ledger");
    expect(pulls.context()).toBe("");
    pulls.filter("");
    pulls.narrow(null);
    pulls.showTab("all");
    expect(pulls.shortcut("⌘ P")).toBe(true);
    await vi.advanceTimersByTimeAsync(0);
    expect(sent.at(-1)).toEqual(["pull_requests_pin", { repo: "fernhill/beacon", pin: true }]);
    expect(pulls.view()?.pinned).toEqual([...fixture.pinned, "fernhill/beacon"]);
    expect(pulls.shortcut("⌘ C")).toBe(false);
    dispose();
  });

  it("lists every pinned repo under Show only, giving keys to the first five", async () => {
    const pinned = ["a", "b", "c", "d", "e", "f"].map((name) => `fernhill/${name}`);
    const { pulls, dispose } = start({
      pull_requests: () => viewOf({ pinned }),
      pull_requests_refresh: () => viewOf({ pinned }),
    });
    await pulls.show();
    const quick = pulls
      .actions({
        organizations: () => undefined,
        repositories: () => undefined,
        ask: () => undefined,
      })
      .filter((action) => action.note === "pinned");
    expect(quick.map((action) => [action.title, action.keys])).toEqual([
      ["fernhill/a", "⌘ 5"],
      ["fernhill/b", "⌘ 6"],
      ["fernhill/c", "⌘ 7"],
      ["fernhill/d", "⌘ 8"],
      ["fernhill/e", "⌘ 9"],
      ["fernhill/f", ""],
    ]);
    expect(pulls.shortcut("")).toBe(false);
    quick.at(-1)?.run();
    expect(pulls.only()).toBe("fernhill/f");
    dispose();
  });

  it("acts from ⌘K on the pull request it was opened for, even after the selection moved", async () => {
    const { pulls, sent, dispose } = start({
      pull_requests: () => viewOf(),
      pull_requests_refresh: () => viewOf(),
      pull_requests_pin: () => [],
    });
    await pulls.show();
    const actions = pulls.actions({
      organizations: () => undefined,
      repositories: () => undefined,
      ask: () => undefined,
    });
    pulls.move(2);
    for (const title of ["Open in Browser", "Copy Link", "Pin beacon"]) {
      actions.find((action) => action.title === title)?.run();
    }
    await vi.advanceTimersByTimeAsync(0);
    const url = pull(169).url;
    expect(sent.slice(-3)).toEqual([
      ["open_link", { url }],
      ["copy_text", { text: url }],
      ["pull_requests_pin", { repo: "fernhill/beacon", pin: true }],
    ]);
    dispose();
  });

  it("says why the first list could not be read, and how to try again", async () => {
    const offline = { kind: "offline" as const, text: "Raiden could not reach GitHub." };
    const { pulls, dispose } = start({
      pull_requests: () => viewOf({ pulls: [], fetchedAt: null }),
      pull_requests_refresh: () => viewOf({ pulls: [], fetchedAt: null, problem: offline }),
    });
    await pulls.show();
    await vi.advanceTimersByTimeAsync(0);
    expect(pulls.failedFirst()).toEqual(offline);
    expect(pulls.context()).toBe("");
    expect(problemAfter(offline)).toBe("⌘R tries again.");
    expect(problemAfter({ kind: "limited", text: "", retryAt: NOW + 300 })).toBe(
      `Raiden tries again at ${clockOf(NOW + 300)}.`,
    );
    dispose();
  });

  it("words the footer for being offline or limited with a saved list, from the times given", async () => {
    const offline = { kind: "offline" as const, text: "Raiden could not reach GitHub." };
    const { pulls, dispose } = start({
      pull_requests: () => viewOf({ fetchedAt: NOW - 18 * 60 }),
      pull_requests_refresh: () => viewOf({ fetchedAt: NOW - 18 * 60, problem: offline }),
    });
    await pulls.show();
    await vi.advanceTimersByTimeAsync(0);
    expect(pulls.context()).toBe("Offline. Showing the list from 14:42.");
    expect(problemLine(offline, null)).toBe("Raiden could not reach GitHub.");
    expect(problemLine({ kind: "limited", text: "used up", retryAt: NOW + 5 * 60 }, NOW - 40)).toBe(
      "GitHub limit, retrying at 15:05.",
    );
    expect(problemLine({ kind: "failed", text: "GitHub did not answer." }, NOW)).toBe(
      "GitHub did not answer.",
    );
    dispose();
  });

  it("matches pinned and lost repos whatever their letter case", async () => {
    const { pulls, sent, dispose } = start({
      pull_requests: () =>
        viewOf({ pinned: ["FernHill/Beacon", "FernHill/Old"], lost: ["fernhill/old"] }),
      pull_requests_refresh: () =>
        viewOf({ pinned: ["FernHill/Beacon", "FernHill/Old"], lost: ["fernhill/old"] }),
      pull_requests_pin: () => [],
    });
    await pulls.show();
    expect(pulls.shortcut("⌘ 5")).toBe(true);
    expect(numbers(pulls.shown())).toHaveLength(5);
    expect(pulls.onlyLabel()).toBe("beacon");
    pulls.narrow("FERNHILL");
    expect(pulls.shown()).toHaveLength(10);
    expect(pulls.shortcut("⌘ 6")).toBe(true);
    expect(pulls.lost()).toBe("FernHill/Old");
    pulls.narrow(null);
    const pin = pulls
      .actions({
        organizations: () => undefined,
        repositories: () => undefined,
        ask: () => undefined,
      })
      .at(-1);
    expect(pin?.title).toBe("Unpin beacon");
    expect(pulls.shortcut("⌘ P")).toBe(true);
    await vi.advanceTimersByTimeAsync(0);
    expect(sent.at(-1)).toEqual(["pull_requests_pin", { repo: "fernhill/beacon", pin: false }]);
    dispose();
  });

  it("says a pinned repo is gone and unpins it with ⌘⌫", async () => {
    const { pulls, sent, dispose } = start({
      pull_requests: () => viewOf({ pinned: ["fernhill/old"], lost: ["fernhill/old"] }),
      pull_requests_refresh: () => viewOf({ pinned: ["fernhill/old"], lost: ["fernhill/old"] }),
      pull_requests_pin: () => [],
    });
    await pulls.show();
    expect(pulls.shortcut("⌘ 5")).toBe(true);
    expect(pulls.lost()).toBe("fernhill/old");
    expect(pulls.shortcut("⌘ ⌫")).toBe(true);
    await vi.advanceTimersByTimeAsync(0);
    expect(sent.at(-1)).toEqual(["pull_requests_pin", { repo: "fernhill/old", pin: false }]);
    expect(pulls.only()).toBe(null);
    expect(pulls.shortcut("⌘ ⌫")).toBe(false);
    dispose();
  });
});

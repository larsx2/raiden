import { createRoot } from "solid-js";
import { describe, expect, test, vi } from "vitest";
import { cleanToken, createSlack, detailOf, type Hit, type SlackView } from "./slack";

const hit = (over: Partial<Hit>): Hit => ({
  id: "x",
  kind: "channel",
  title: "#general",
  subtitle: "channel",
  unread: 0,
  mention: false,
  updated: 0,
  link: "",
  ...over,
});

describe("cleanToken", () => {
  test("pulls a lone token out of quotes or a Bearer wrapper", () => {
    expect(cleanToken('"xoxp-1-abc"')).toBe("xoxp-1-abc");
    expect(cleanToken("Bearer xoxp-1-abc ")).toBe("xoxp-1-abc");
    expect(cleanToken("xoxp-1-abc")).toBe("xoxp-1-abc");
  });

  test("falls back to the trimmed text when there is no token", () => {
    expect(cleanToken("  nope  ")).toBe("nope");
  });
});

describe("detailOf", () => {
  test("labels a conversation by kind and shows a message's own subtitle", () => {
    expect(detailOf(hit({ kind: "person", subtitle: "@ana" }))).toBe("Direct message · @ana");
    expect(detailOf(hit({ kind: "channel", subtitle: "" }))).toBe("Channel");
    expect(detailOf(hit({ kind: "message", subtitle: "ana · #general" }))).toBe("ana · #general");
  });
});

describe("createSlack", () => {
  test("↵ typed ahead of the reply opens the newest query's top row, not the old one", async () => {
    vi.useFakeTimers();
    const opened: string[] = [];
    const replies: Record<string, Hit[]> = {
      "": [hit({ title: "Old", link: "slack://user?team=T&id=OLD" })],
      ifa: [hit({ title: "#ifa", link: "slack://channel?team=T&id=IFA" })],
    };
    const call = async <T>(command: string, args?: Record<string, unknown>) => {
      if (command === "slack_open") {
        opened.push(String(args?.url));
      }
      const view: SlackView = { hasToken: true, hits: replies[String(args?.query)] ?? [] };
      return view as T;
    };
    await createRoot(async (dispose) => {
      const slack = createSlack({ call });
      void slack.show();
      await vi.runAllTimersAsync();
      slack.filter("ifa");
      slack.act();
      expect(opened).toEqual([]);
      await vi.runAllTimersAsync();
      expect(opened).toEqual(["slack://channel?team=T&id=IFA"]);
      // An ↵ voided by more typing does not fire when that later reply lands.
      slack.filter("if");
      slack.act();
      slack.filter("ifa");
      await vi.runAllTimersAsync();
      expect(opened).toHaveLength(1);
      dispose();
    }).finally(() => vi.useRealTimers());
  });

  test("the list asks again while unread counts are still arriving, then stops", async () => {
    vi.useFakeTimers();
    let asked = 0;
    const call = async <T>(command: string) => {
      if (command === "slack_search") {
        asked++;
      }
      const view: SlackView = {
        hasToken: true,
        hits: [hit({ unread: asked })],
        updating: asked < 3,
      };
      return view as T;
    };
    await createRoot(async (dispose) => {
      const slack = createSlack({ call });
      void slack.show();
      await vi.advanceTimersByTimeAsync(10_000);
      expect(asked).toBe(3);
      expect(slack.shown()[0]?.unread).toBe(3);
      dispose();
    }).finally(() => vi.useRealTimers());
  });
});

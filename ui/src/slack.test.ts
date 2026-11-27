import { describe, expect, test } from "vitest";
import { cleanToken, detailOf, type Hit } from "./slack";

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

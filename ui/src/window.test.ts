import { describe, expect, it } from "vitest";
import { createWindowSize, rowsGained, sizeTitle } from "./window";

type Sent = { command: string; args?: Record<string, unknown> };

const fakeCall = (saved: boolean) => {
  const sent: Sent[] = [];
  let kept = saved;
  const call = async <T>(command: string, args?: Record<string, unknown>): Promise<T> => {
    sent.push(args === undefined ? { command } : { command, args });
    if (command === "window_size") {
      return kept as T;
    }
    if (command === "expand_window") {
      kept = args?.["expanded"] === true;
      return kept as T;
    }
    return null as T;
  };
  return { call, sent };
};

describe("window size", () => {
  it("names the action after what it will do", () => {
    expect(sizeTitle(false)).toBe("Expand Window");
    expect(sizeTitle(true)).toBe("Shrink Window");
  });

  it("pages through as many more rows as the taller window shows", () => {
    expect(rowsGained(452, 452, 48)).toBe(0);
    expect(rowsGained(0, 452, 48)).toBe(0);
    expect(rowsGained(660, 452, 48)).toBe(4);
    expect(rowsGained(660, 452, 49)).toBe(4);
  });

  it("keeps one choice for the whole launcher, starting from what was saved", async () => {
    const { call, sent } = fakeCall(true);
    const size = createWindowSize({ call });
    await size.load();
    expect(size.isExpanded()).toBe(true);
    expect(size.action()).toMatchObject({ title: "Shrink Window", keys: "⌘ ⇧ E" });
    size.action().run();
    expect(size.isExpanded()).toBe(false);
    expect(size.action().title).toBe("Expand Window");
    size.toggle();
    await Promise.resolve();
    expect(size.isExpanded()).toBe(true);
    expect(sent).toEqual([
      { command: "window_size" },
      { command: "expand_window", args: { expanded: false } },
      { command: "expand_window", args: { expanded: true } },
    ]);
  });

  it("starts compact when nothing was saved", async () => {
    const { call } = fakeCall(false);
    const size = createWindowSize({ call });
    await size.load();
    expect(size.isExpanded()).toBe(false);
  });

  it("keeps the latest choice when replies arrive out of order", async () => {
    const replies: Array<(kept: boolean) => void> = [];
    const call = <T>(command: string): Promise<T> =>
      command === "expand_window"
        ? new Promise<T>((resolve) => replies.push((kept) => resolve(kept as T)))
        : Promise.resolve(null as T);
    const size = createWindowSize({ call });
    size.toggle();
    size.toggle();
    expect(size.isExpanded()).toBe(false);
    replies[1]?.(false);
    replies[0]?.(true);
    await Promise.resolve();
    await Promise.resolve();
    expect(size.isExpanded()).toBe(false);
  });

  it("does not let a slow first read undo a press made while it was out", async () => {
    let answer: (saved: boolean) => void = () => undefined;
    const call = <T>(command: string, args?: Record<string, unknown>): Promise<T> =>
      command === "window_size"
        ? new Promise<T>((resolve) => {
            answer = (saved) => resolve(saved as T);
          })
        : Promise.resolve((args?.["expanded"] === true) as T);
    const size = createWindowSize({ call });
    const loading = size.load();
    size.toggle();
    answer(false);
    await loading;
    expect(size.isExpanded()).toBe(true);
  });
});

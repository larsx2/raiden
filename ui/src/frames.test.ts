import { describe, expect, it } from "vitest";
import { createFrameWatcher, type FrameSchedule } from "./frames";

describe("createFrameWatcher", () => {
  it("reports the time between frames while activity continues, then stops", () => {
    const reported: number[] = [];
    const queue: Array<(time: number) => void> = [];
    const nextFrame: FrameSchedule = (callback) => queue.push(callback);
    const { activity } = createFrameWatcher((ms) => reported.push(ms), nextFrame, 50);
    activity(0);
    queue.shift()?.(10);
    queue.shift()?.(20);
    queue.shift()?.(40);
    expect(reported).toEqual([10, 20]);
    queue.shift()?.(70);
    expect(queue).toHaveLength(0);
    expect(reported).toEqual([10, 20, 30]);
  });

  it("drops the gap while the panel was hidden", () => {
    const reported: number[] = [];
    const queue: Array<(time: number) => void> = [];
    const nextFrame: FrameSchedule = (callback) => queue.push(callback);
    const watcher = createFrameWatcher((ms) => reported.push(ms), nextFrame, 50);
    watcher.activity(0);
    queue.shift()?.(10);
    watcher.reset();
    queue.shift()?.(900);
    watcher.activity(900);
    queue.shift()?.(910);
    queue.shift()?.(925);
    expect(reported).toEqual([15]);
  });
});

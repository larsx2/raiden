import { describe, expect, it } from "vitest";
import { afterNextPaint, type Schedule } from "./speed";

describe("afterNextPaint", () => {
  it("runs in the task right after the next frame, not a whole frame later", () => {
    const steps: string[] = [];
    const frames: Array<() => void> = [];
    const tasks: Array<() => void> = [];
    const nextFrame: Schedule = (callback) => frames.push(callback);
    const nextTask: Schedule = (callback) => tasks.push(callback);
    afterNextPaint(() => steps.push("measured"), nextFrame, nextTask);
    expect(steps).toEqual([]);
    frames.shift()?.();
    expect(frames).toHaveLength(0);
    expect(steps).toEqual([]);
    tasks.shift()?.();
    expect(steps).toEqual(["measured"]);
  });
});

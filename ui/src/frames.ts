export type FrameSchedule = (callback: (time: number) => void) => void;

export type FrameWatcher = {
  activity: (now: number) => void;
  reset: () => void;
};

export function createFrameWatcher(
  report: (ms: number) => void,
  nextFrame: FrameSchedule,
  idleMs = 150,
): FrameWatcher {
  let previous = 0;
  let lastActivity = 0;
  let running = false;
  let generation = 0;
  const tick = (time: number, mine: number) => {
    if (mine !== generation) {
      return;
    }
    if (previous > 0) {
      report(time - previous);
    }
    previous = time;
    if (time - lastActivity < idleMs) {
      nextFrame((next) => tick(next, mine));
    } else {
      running = false;
      previous = 0;
    }
  };
  return {
    activity: (now: number) => {
      lastActivity = now;
      if (!running) {
        running = true;
        previous = 0;
        const mine = generation;
        nextFrame((next) => tick(next, mine));
      }
    },
    reset: () => {
      generation += 1;
      running = false;
      previous = 0;
    },
  };
}

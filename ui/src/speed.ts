export type Schedule = (callback: () => void) => void;

export function afterNextPaint(
  callback: () => void,
  nextFrame: Schedule,
  nextTask: Schedule,
): void {
  nextFrame(() => nextTask(callback));
}

export const nextTask: Schedule = (callback) => {
  const channel = new MessageChannel();
  channel.port1.onmessage = () => {
    channel.port1.close();
    callback();
  };
  channel.port2.postMessage(null);
};

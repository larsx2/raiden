import { createSignal } from "solid-js";
import type { Action } from "./actions";

type Call = <T>(command: string, args?: Record<string, unknown>) => Promise<T>;

export const EXPANDED = { width: 900, height: 660 };
export const SIZE_KEYS = "⌘ ⇧ E";
const EXPAND_TITLE = "Expand Window";
const SHRINK_TITLE = "Shrink Window";

export function rowsGained(height: number, compactHeight: number, rowHeight: number): number {
  return height > compactHeight ? Math.floor((height - compactHeight) / rowHeight) : 0;
}

export function sizeTitle(expanded: boolean): string {
  return expanded ? SHRINK_TITLE : EXPAND_TITLE;
}

export function createWindowSize(options: { call: Call }) {
  const [expanded, setExpanded] = createSignal(false);
  let asked = 0;

  const load = () => {
    const at = asked;
    return options
      .call<boolean | null>("window_size")
      .then((saved) => {
        if (at === asked && typeof saved === "boolean") {
          setExpanded(saved);
        }
      })
      .catch(() => undefined);
  };

  const toggle = () => {
    const next = !expanded();
    const at = ++asked;
    setExpanded(next);
    options
      .call<boolean | null>("expand_window", { expanded: next })
      .then((kept) => {
        if (at === asked && typeof kept === "boolean") {
          setExpanded(kept);
        }
      })
      .catch(() => undefined);
  };

  const action = (): Action => ({
    title: sizeTitle(expanded()),
    keys: SIZE_KEYS,
    hides: false,
    run: toggle,
  });

  return { load, isExpanded: expanded, toggle, action };
}

export type WindowSize = ReturnType<typeof createWindowSize>;

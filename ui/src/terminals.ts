import type { Action } from "./actions";
import alacritty from "./terminal-icons/alacritty.png";
import kitty from "./terminal-icons/kitty.png";
import warp from "./terminal-icons/warp.png";

export type TerminalId =
  | "ghostty"
  | "iterm"
  | "rex"
  | "tern"
  | "terminal"
  | "kitty"
  | "alacritty"
  | "warp";
export type TerminalRow = { id: TerminalId; name: string; app: string | null };

export const NOT_INSTALLED = "Not installed";
export const OPEN_IN_TERMINAL_KEYS = "⌘ ⇧ T";

const BUNDLED: Partial<Record<TerminalId, string>> = { kitty, alacritty, warp };

export function terminalActions(
  terminals: TerminalRow[],
  inUse: TerminalId,
  choose: (id: TerminalId) => void,
): Action[] {
  const installed = terminals.filter((terminal) => terminal.app !== null);
  const missing = terminals.filter((terminal) => terminal.app === null);
  return [
    ...installed.map((terminal, at): Action => ({
      title: terminal.name,
      keys: at < 9 ? `⌘ ${at + 1}` : "",
      hides: false,
      run: () => choose(terminal.id),
      app: terminal.app ?? undefined,
      checked: terminal.id === inUse,
    })),
    ...missing.map((terminal): Action => ({
      title: terminal.name,
      keys: "",
      hides: false,
      run: () => undefined,
      image: BUNDLED[terminal.id] ?? "",
      section: NOT_INSTALLED,
      off: true,
    })),
  ];
}

export const terminalNamed = (terminals: TerminalRow[], id: TerminalId) =>
  terminals.find((terminal) => terminal.id === id);

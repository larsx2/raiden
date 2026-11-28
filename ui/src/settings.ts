import { batch, createMemo, createSignal } from "solid-js";
import type { Action } from "./actions";
import type { Account, AccountId } from "./ask";
import { clamp } from "./list";
import { terminalActions, type TerminalId, type TerminalRow } from "./terminals";

export type RowId =
  | "hotkey"
  | "clipboard"
  | "login"
  | "terminal"
  | "vim"
  | "ai"
  | "effort"
  | "grammar"
  | "file";
export type Effort = "low" | "medium" | "high";
export type HotkeyRow = "hotkey" | "clipboard";
export type SettingRow = { id: RowId; group: string; name: string; hint: string; words: string };
export type AtLogin = { on: boolean; canChange: boolean; note: string | null };
export type SettingsView = {
  hotkey: string;
  clipboardHotkey: string;
  ai: AccountId;
  effort: Effort;
  vim: boolean;
  terminal: TerminalId;
  terminals: TerminalRow[];
  file: string;
  atLogin: AtLogin;
};
export type SettingsMenu = { title: string; actions: Action[]; tall: boolean; chooser: boolean };
export type EngineStep =
  | { kind: "use"; id: AccountId; title: string }
  | { kind: "signIn"; id: AccountId; title: string };
export type Pressed = Pick<
  KeyboardEvent,
  "key" | "code" | "ctrlKey" | "altKey" | "shiftKey" | "metaKey"
>;

type Call = <T>(command: string, args?: Record<string, unknown>) => Promise<T>;

export const SETTINGS_PLACEHOLDER = "Search settings";
export const OPEN_FILE_KEYS = "⌘ O";
const MODIFIER_KEYS = new Set(["Shift", "Control", "Alt", "Meta", "CapsLock", "Fn"]);
const MODIFIER_SYMBOLS = ["⌃", "⌥", "⇧", "⌘"];

export const SETTING_ROWS: SettingRow[] = [
  {
    id: "hotkey",
    group: "General",
    name: "Hotkey",
    hint: "Opens Raiden from any app",
    words: "shortcut keys keyboard",
  },
  {
    id: "clipboard",
    group: "General",
    name: "Clipboard history",
    hint: "Opens what you copied in the last 7 days",
    words: "hotkey shortcut keys keyboard copy paste",
  },
  {
    id: "login",
    group: "General",
    name: "Open at login",
    hint: "Starts quietly when you log in",
    words: "startup launch agent",
  },
  {
    id: "terminal",
    group: "General",
    name: "Terminal",
    hint: "Where anything Raiden opens in a terminal starts",
    words: "shell console ghostty iterm rex tern kitty alacritty warp folder",
  },
  {
    id: "vim",
    group: "General",
    name: "Vim keys",
    hint: "Esc switches to Normal, where j and k move. i or / types again",
    words: "vim vi modal normal insert hjkl keyboard navigation",
  },
  {
    id: "ai",
    group: "Ask AI",
    name: "Answers come from",
    hint: "Uses your own subscription, never an API key",
    words: "engine model claude chatgpt codex",
  },
  {
    id: "effort",
    group: "Ask AI",
    name: "Effort",
    hint: "Higher effort gives better answers but a slower start",
    words: "reasoning thinking speed low medium high",
  },
  {
    id: "grammar",
    group: "Ask AI",
    name: "Fix grammar",
    hint: "Always Claude Code at low effort for speed, web search off",
    words: "claude",
  },
  {
    id: "file",
    group: "File",
    name: "Settings file",
    hint: "",
    words: "raiden.toml toml edit editor",
  },
];

const wordsOf = (row: SettingRow) =>
  `${row.group} ${row.name} ${row.hint} ${row.words}`.toLowerCase().split(/[^\p{L}\p{N}.]+/u);

export function filterRows(rows: SettingRow[], query: string): SettingRow[] {
  const wanted = query.toLowerCase().split(/\s+/).filter(Boolean);
  return rows.filter((row) => {
    const words = wordsOf(row);
    return wanted.every((part) => words.some((word) => word.startsWith(part)));
  });
}

export function groupsOf(rows: SettingRow[]): Array<[group: string, rows: SettingRow[]]> {
  const groups = new Map<string, SettingRow[]>();
  for (const row of rows) {
    groups.set(row.group, [...(groups.get(row.group) ?? []), row]);
  }
  return [...groups];
}

export function keyName(code: string, key: string): string {
  const short = /^(?:Key([A-Z])|Digit(\d))$/.exec(code);
  if (short) {
    return short[1] ?? short[2] ?? code;
  }
  return code || (key.length === 1 ? key.toUpperCase() : key);
}

export function hotkeyText(event: Pressed): string | null {
  if (MODIFIER_KEYS.has(event.key)) {
    return null;
  }
  const held = [event.ctrlKey, event.altKey, event.shiftKey, event.metaKey]
    .map((on, at) => (on ? MODIFIER_SYMBOLS[at] : ""))
    .join("");
  return `${held}${keyName(event.code, event.key)}`;
}

export function hotkeyKeys(hotkey: string): string[] {
  const keys: string[] = [];
  let rest = hotkey;
  while (rest.length > 1 && MODIFIER_SYMBOLS.includes(rest.charAt(0))) {
    keys.push(rest.charAt(0));
    rest = rest.slice(1);
  }
  return rest ? [...keys, rest] : keys;
}

export function tryAnother(problem: string): string {
  const said = problem.trim();
  return `${/[.!?]$/.test(said) ? said : `${said}.`} Try another.`;
}

export const EFFORTS: Effort[] = ["low", "medium", "high"];

export const effortName = (effort: Effort) => effort.charAt(0).toUpperCase() + effort.slice(1);

export function nextEffort(effort: Effort): Effort {
  return EFFORTS[(EFFORTS.indexOf(effort) + 1) % EFFORTS.length] ?? "low";
}

export function engineStep(other: Account): EngineStep {
  return other.signedIn
    ? { kind: "use", id: other.id, title: `Use ${other.name}` }
    : { kind: "signIn", id: other.id, title: `Sign in to ${other.name}` };
}

const problemOf = (error: unknown) =>
  error instanceof Error ? error.message : typeof error === "string" ? error : String(error);

const fileName = (path: string) => path.split("/").pop() || path;

export function createSettings(options: {
  call: Call;
  accountOf: (id: AccountId) => Account;
  refreshAccounts: () => Promise<void>;
  refreshChoice: () => Promise<void>;
  vimChanged: (on: boolean) => void;
  showMenu: (menu: SettingsMenu) => void;
}) {
  const { call } = options;
  const [open, setOpen] = createSignal(false);
  const [view, setView] = createSignal<SettingsView | null>(null);
  const [query, setQuery] = createSignal("");
  const [selected, setSelected] = createSignal(0);
  const [recording, setRecording] = createSignal<HotkeyRow | null>(null);
  const [problem, setProblem] = createSignal<{ row: RowId; text: string } | null>(null);
  const [signing, setSigning] = createSignal<AccountId | null>(null);
  const rows = createMemo(() => filterRows(SETTING_ROWS, query()));
  let shown = 0;

  const other = (id: AccountId): AccountId => (id === "claude" ? "chatgpt" : "claude");
  const chosen = () => view()?.ai ?? "claude";
  const step = () => engineStep(options.accountOf(other(chosen())));
  const selectedRow = () => rows()[Math.min(selected(), rows().length - 1)];

  const load = async (args: Record<string, unknown> = {}) => {
    const mine = ++shown;
    void options.refreshAccounts();
    const found = await call<SettingsView | null>("settings_view", args).catch((error: unknown) => {
      setProblem({ row: "file", text: problemOf(error) });
      return null;
    });
    if (found && mine === shown) {
      setView(found);
      options.vimChanged(found.vim);
    }
  };

  const show = (picked?: { query: string; row: number }) => {
    batch(() => {
      setOpen(true);
      setQuery("");
      setSelected(0);
      setProblem(null);
      setRecording(null);
      setView(null);
    });
    return load(picked);
  };

  const stopRecording = () => {
    if (recording()) {
      setRecording(null);
      void call("record_hotkey", { on: false });
    }
  };

  const hide = () => {
    stopRecording();
    shown++;
    batch(() => {
      setOpen(false);
      setProblem(null);
    });
  };

  const filter = (text: string) => {
    batch(() => {
      setQuery(text);
      setSelected(0);
    });
  };

  const select = (row: number) => setSelected(row);

  const move = (by: 1 | -1) => {
    const count = rows().length;
    if (count > 0) {
      select(clamp(selected() + by, count));
    }
  };

  const startRecording = (row: HotkeyRow) => {
    if (!open()) {
      return;
    }
    batch(() => {
      setProblem(null);
      setRecording(row);
    });
    void call("record_hotkey", { on: true });
  };

  const tryHotkey = (text: string) => {
    const row = recording();
    if (!row) {
      return Promise.resolve();
    }
    return call<string>("set_hotkey", { hotkey: text, which: row }).then(
      (saved) => {
        const now = view();
        batch(() => {
          if (now) {
            setView(
              row === "clipboard" ? { ...now, clipboardHotkey: saved } : { ...now, hotkey: saved },
            );
          }
          setProblem(null);
          setRecording(null);
        });
      },
      (error: unknown) => {
        if (recording() === row) {
          setProblem({ row, text: tryAnother(problemOf(error)) });
        }
      },
    );
  };

  const cancel = () => {
    stopRecording();
    setProblem(null);
  };

  const press = (event: Pressed) => {
    if (event.key === "Escape") {
      cancel();
      return;
    }
    const text = hotkeyText(event);
    if (text) {
      void tryHotkey(text);
    }
  };

  const heard = (text: string) => {
    if (recording()) {
      void tryHotkey(text);
    }
  };

  const choose = async (id: AccountId) => {
    try {
      await call("choose_ai", { id });
    } catch (error) {
      setProblem({ row: "ai", text: problemOf(error) });
      return;
    }
    const now = view();
    if (now) {
      setView({ ...now, ai: id });
    }
    setProblem(null);
    await options.refreshChoice();
  };

  let savingEffort = false;

  const cycleEffort = async () => {
    const now = view();
    if (!now || savingEffort) {
      return;
    }
    savingEffort = true;
    const effort = nextEffort(now.effort);
    try {
      await call("choose_effort", { id: effort });
    } catch (error) {
      setProblem({ row: "effort", text: problemOf(error) });
      return;
    } finally {
      savingEffort = false;
    }
    const latest = view();
    if (latest) {
      setView({ ...latest, effort });
    }
    setProblem(null);
  };

  const signIn = async (id: AccountId) => {
    const before = await call<unknown>("chosen_ai").catch(() => chosen());
    setSigning(id);
    setProblem(null);
    let failed = "";
    try {
      await call("ai_sign_in", { id });
    } catch (error) {
      failed = problemOf(error);
    }
    await options.refreshAccounts();
    setSigning(null);
    if (failed) {
      setProblem({ row: "ai", text: failed });
    } else if (id !== before && options.accountOf(id).signedIn) {
      const now = await call<unknown>("chosen_ai").catch(() => before);
      if (now === before) {
        await choose(id);
      }
    }
  };

  const pick = (id: AccountId) => {
    if (signing() || recording()) {
      return;
    }
    if (!options.accountOf(id).signedIn) {
      void signIn(id);
    } else if (id !== chosen()) {
      void choose(id);
    }
  };

  const switchEngine = () => {
    const next = step();
    if (signing()) {
      return;
    }
    if (next.kind === "signIn") {
      void signIn(next.id);
    } else {
      void choose(next.id);
    }
  };

  const toggleLogin = () => {
    const now = view();
    if (!now?.atLogin.canChange) {
      return;
    }
    call<AtLogin>("set_open_at_login", { on: !now.atLogin.on }).then(
      (atLogin) => {
        const latest = view();
        if (latest) {
          setView({ ...latest, atLogin });
        }
        setProblem(null);
      },
      (error: unknown) => {
        setProblem({ row: "login", text: problemOf(error) });
        void call<SettingsView | null>("settings_view").then((found) => found && setView(found));
      },
    );
  };

  let savingVim = false;

  const toggleVim = async () => {
    const now = view();
    if (!now || savingVim) {
      return;
    }
    savingVim = true;
    try {
      const on = await call<boolean>("set_vim_keys", { on: !now.vim });
      const latest = view();
      if (latest) {
        setView({ ...latest, vim: on });
      }
      setProblem(null);
      options.vimChanged(on);
    } catch (error) {
      setProblem({ row: "vim", text: problemOf(error) });
    } finally {
      savingVim = false;
    }
  };

  const chooseTerminal = async (id: TerminalId) => {
    try {
      const chosen = await call<TerminalId>("choose_terminal", { id });
      const latest = view();
      if (latest) {
        setView({ ...latest, terminal: chosen ?? id });
      }
      setProblem(null);
    } catch (error) {
      setProblem({ row: "terminal", text: problemOf(error) });
    }
  };

  const chooseTerminalMenu = () => {
    const now = view();
    if (now) {
      options.showMenu({
        title: "Terminal",
        actions: terminalActions(now.terminals, now.terminal, (id) => void chooseTerminal(id)),
        tall: true,
        chooser: true,
      });
    }
  };

  const openFile = () =>
    void call("open_settings_file").catch((error: unknown) =>
      setProblem({ row: "file", text: problemOf(error) }),
    );

  const actionOf = (id: RowId): Action | undefined => {
    const now = view();
    switch (id) {
      case "hotkey":
        return now
          ? { title: "Change hotkey", keys: "↵", hides: false, run: () => startRecording("hotkey") }
          : undefined;
      case "clipboard":
        return now
          ? {
              title: "Change Clipboard history hotkey",
              keys: "↵",
              hides: false,
              run: () => startRecording("clipboard"),
            }
          : undefined;
      case "login":
        return now?.atLogin.canChange
          ? {
              title: now.atLogin.on ? "Turn off Open at login" : "Turn on Open at login",
              keys: "↵",
              hides: false,
              run: toggleLogin,
            }
          : undefined;
      case "terminal":
        return now
          ? { title: "Choose terminal", keys: "↵", hides: false, run: chooseTerminalMenu }
          : undefined;
      case "vim":
        return now
          ? {
              title: now.vim ? "Turn off Vim keys" : "Turn on Vim keys",
              keys: "↵",
              hides: false,
              run: () => void toggleVim(),
            }
          : undefined;
      case "ai":
        return now && !signing()
          ? { title: step().title, keys: "↵", hides: false, run: switchEngine }
          : undefined;
      case "effort":
        return now
          ? {
              title: `Change effort to ${effortName(nextEffort(now.effort))}`,
              keys: "↵",
              hides: false,
              run: () => void cycleEffort(),
            }
          : undefined;
      case "grammar":
        return undefined;
      case "file":
        return { title: "Open settings file", keys: "↵", hides: true, run: openFile };
    }
  };

  const act = (row?: SettingRow) => {
    const target = row ?? selectedRow();
    if (target) {
      actionOf(target.id)?.run();
    }
  };

  const primary = (): Action | undefined => {
    const row = selectedRow();
    return row ? actionOf(row.id) : undefined;
  };

  const menuActions = (): Action[] => {
    const current = selectedRow()?.id;
    const first = SETTING_ROWS.filter((row) => row.id === current);
    return [...first, ...SETTING_ROWS.filter((row) => row.id !== current)].flatMap((row) => {
      const action = actionOf(row.id);
      if (!action) {
        return [];
      }
      const keys = row.id === current ? "↵" : row.id === "file" ? OPEN_FILE_KEYS : "";
      return [{ ...action, keys }];
    });
  };

  const shortcut = (keys: string) => {
    if (keys === OPEN_FILE_KEYS) {
      openFile();
      return true;
    }
    return false;
  };

  const context = (): string => {
    if (recording()) {
      return "The old hotkey keeps working until the new one is set";
    }
    const pending = signing();
    if (pending) {
      return `Finish signing in to ${options.accountOf(pending).through}`;
    }
    const next = step();
    if (selectedRow()?.id === "ai" && next.kind === "signIn") {
      return `Opens ${options.accountOf(next.id).through}'s own sign-in. Raiden never sees your password or token`;
    }
    return `Saved to ${fileName(view()?.file ?? "raiden.toml")} as you change things`;
  };

  const hintOf = (row: SettingRow): { text: string; warn: boolean } => {
    const said = problem();
    if (said?.row === row.id) {
      return { text: said.text, warn: true };
    }
    const now = view();
    if (row.id === "login" && now?.atLogin.note) {
      return { text: now.atLogin.note, warn: false };
    }
    if (row.id === "ai") {
      const out = [chosen(), other(chosen())]
        .map((id) => options.accountOf(id))
        .find((account) => !account.signedIn);
      if (out) {
        return { text: `${out.name}: ${out.through} is signed out on this Mac`, warn: true };
      }
    }
    return { text: row.hint, warn: false };
  };

  return {
    open,
    view,
    query,
    rows,
    selected: () => Math.min(selected(), rows().length - 1),
    select,
    selectedRow,
    recording,
    signing,
    chosen,
    show,
    hide,
    filter,
    move,
    press,
    cancel,
    heard,
    pick,
    act,
    primary,
    menuActions,
    shortcut,
    context,
    hintOf,
  };
}

export type Settings = ReturnType<typeof createSettings>;

import { batch, createMemo, createSignal } from "solid-js";
import type { Action } from "./actions";
import { clamp } from "./list";

const STILL_READING_MS = 2000;

export type Look = "bad" | "ok" | "run" | "none";
export type Checks = "passed" | "failed" | "running" | "none";
export type Pull = {
  owner: string;
  repo: string;
  shown: string;
  number: number;
  title: string;
  url: string;
  draft: boolean;
  updatedAt: number;
  branch: string;
  base: string;
  additions: number;
  deletions: number;
  files: number;
  conflicts: boolean;
  checks: Checks;
  failed: string[];
  running: string[];
  passed: number;
  failing: boolean;
  ready: boolean;
  status: string;
  look: Look;
};
export type Owner = { name: string; open: number };
export type ProblemKind = "missing" | "signedOut" | "offline" | "limited" | "failed";
export type Problem = { kind: ProblemKind; text: string; retryAt?: number };
export type PullsView = {
  allowed: boolean;
  login: string | null;
  pulls: Pull[];
  fetchedAt: number | null;
  reviewRequested: number;
  owners: Owner[];
  pinned: string[];
  lost: string[];
  refreshing: boolean;
  problem: Problem | null;
};
export type Tab = "all" | "failing" | "ready" | "drafts";
export type PullsContext = { label: string; text: string };
export type PullsStart = { tab?: Tab; only?: string | null; query?: string };
export type Picked = { query: string; row: number };
export type CheckLine = { look: "fail" | "run" | "pass"; text: string };
export type RepoChoice = { repo: string; shown: string; open: number };

type Call = <T>(command: string, args?: Record<string, unknown>) => Promise<T>;

export const PULLS_TITLE = "Pull Requests";
export const PULLS_PLACEHOLDER = "Filter pull requests";
export const SIGN_IN_COMMAND = "gh auth login";
export const OPEN_KEYS = "↵";
export const COPY_LINK_KEYS = "⌘ ⇧ C";
export const PIN_KEYS = "⌘ P";
export const ORGS_KEYS = "⌘ O";
export const REPOS_KEYS = "⌘ ⇧ R";
export const REFRESH_KEYS = "⌘ R";
export const UNPIN_KEYS = "⌘ ⌫";
export const SHOW_ONLY = "Show only";
export const RETRY_TEXT = "⌘R tries again.";
const FIRST_QUICK_FILTER = 5;
const LAST_QUICK_FILTER = 9;
const TICK_MS = 10_000;

export const TABS: Array<{ id: Tab; title: string }> = [
  { id: "all", title: "All" },
  { id: "failing", title: "Failing" },
  { id: "ready", title: "Ready" },
  { id: "drafts", title: "Drafts" },
];

export function tabAfter(tab: Tab, step: 1 | -1): Tab {
  const at = TABS.findIndex((candidate) => candidate.id === tab);
  return TABS[clamp(at + step, TABS.length)]?.id ?? tab;
}

const EMPTY: Record<Tab, string> = {
  all: "No open pull requests",
  failing: "Nothing failing",
  ready: "Nothing ready to merge",
  drafts: "No drafts",
};

export const repoOf = (pull: Pull) => `${pull.owner}/${pull.repo}`;
export const sameName = (one: string, other: string) => one.toLowerCase() === other.toLowerCase();
export const keyOf = (pull: Pull) => `${repoOf(pull)}#${pull.number}`;

export function inTab(pull: Pull, tab: Tab): boolean {
  switch (tab) {
    case "failing":
      return pull.failing;
    case "ready":
      return pull.ready;
    case "drafts":
      return pull.draft;
    default:
      return true;
  }
}

export function within(pull: Pull, only: string | null): boolean {
  if (!only) {
    return true;
  }
  return sameName(only.includes("/") ? repoOf(pull) : pull.owner, only);
}

export function matches(pull: Pull, query: string): boolean {
  const wanted = query.toLowerCase().split(/\s+/).filter(Boolean);
  if (wanted.length === 0) {
    return true;
  }
  const words =
    `${pull.title}\n${pull.shown}\n${repoOf(pull)}\n#${pull.number}\n${pull.branch}`.toLowerCase();
  return wanted.every((word) => words.includes(word));
}

export function countsOf(pulls: Pull[], only: string | null): Record<Tab, number> {
  const counts: Record<Tab, number> = { all: 0, failing: 0, ready: 0, drafts: 0 };
  for (const pull of pulls) {
    if (within(pull, only)) {
      for (const { id } of TABS) {
        if (inTab(pull, id)) {
          counts[id]++;
        }
      }
    }
  }
  return counts;
}

export function shownOf(pulls: Pull[], tab: Tab, only: string | null, query: string): Pull[] {
  return pulls.filter((pull) => inTab(pull, tab) && within(pull, only) && matches(pull, query));
}

export function outsideCount(pulls: Pull[], tab: Tab, only: string | null, query: string): number {
  if (!only) {
    return 0;
  }
  return pulls.filter((pull) => inTab(pull, tab) && !within(pull, only) && matches(pull, query))
    .length;
}

export function labelOf(only: string, pulls: Pull[]): string {
  if (!only.includes("/")) {
    return only;
  }
  return pulls.find((pull) => sameName(repoOf(pull), only))?.shown ?? only;
}

export function compactAge(at: number, now: number): string {
  const seconds = Math.max(0, now - at);
  if (seconds < 60) {
    return "now";
  }
  if (seconds < 3600) {
    return `${Math.floor(seconds / 60)}m`;
  }
  if (seconds < 86_400) {
    return `${Math.floor(seconds / 3600)}h`;
  }
  return `${Math.floor(seconds / 86_400)}d`;
}

export function updatedAgo(at: number, now: number): string {
  const age = compactAge(at, now);
  return age === "now" ? "updated just now" : `updated ${age} ago`;
}

export function clockOf(at: number): string {
  return new Date(at * 1000).toLocaleTimeString("en-GB", { hour: "2-digit", minute: "2-digit" });
}

export function problemAfter(problem: Problem): string {
  if (problem.kind === "limited" && problem.retryAt !== undefined) {
    return `Raiden tries again at ${clockOf(problem.retryAt)}.`;
  }
  return problem.kind === "offline" || problem.kind === "failed" ? RETRY_TEXT : "";
}

export function problemLine(problem: Problem, fetchedAt: number | null): string {
  if (problem.kind === "offline" && fetchedAt !== null) {
    return `Offline. Showing the list from ${clockOf(fetchedAt)}.`;
  }
  if (problem.kind === "limited" && problem.retryAt !== undefined) {
    return `GitHub limit, retrying at ${clockOf(problem.retryAt)}.`;
  }
  return problem.text;
}

export function emptyText(tab: Tab, label: string | null): string {
  return label ? `${EMPTY[tab]} in ${label}.` : `${EMPTY[tab]}.`;
}

export function checkLines(pull: Pull): CheckLine[] {
  const lines: CheckLine[] = [
    ...pull.failed.map((text): CheckLine => ({ look: "fail", text })),
    ...pull.running.map((text): CheckLine => ({ look: "run", text })),
  ];
  if (pull.passed > 0) {
    const passed = `${pull.passed} passed`;
    lines.push({ look: "pass", text: lines.length > 0 ? `and ${passed}` : passed });
  }
  return lines;
}

export function repoChoices(pulls: Pull[]): RepoChoice[] {
  const found = new Map<string, RepoChoice>();
  for (const pull of pulls) {
    const repo = repoOf(pull);
    const before = found.get(repo);
    found.set(repo, { repo, shown: pull.shown, open: (before?.open ?? 0) + 1 });
  }
  return [...found.values()].toSorted(
    (one, other) => other.open - one.open || one.shown.localeCompare(other.shown),
  );
}

export function keptSelection(before: Pull | undefined, at: number, after: Pull[]): number {
  if (after.length === 0) {
    return 0;
  }
  const key = before ? keyOf(before) : "";
  const found = after.findIndex((pull) => keyOf(pull) === key);
  return found >= 0 ? found : Math.min(at, after.length - 1);
}

export function quickFilterKeys(at: number): string {
  const key = FIRST_QUICK_FILTER + at;
  return key <= LAST_QUICK_FILTER ? `⌘ ${key}` : "";
}

const problemOf = (error: unknown) =>
  error instanceof Error ? error.message : typeof error === "string" ? error : String(error);

export function createPulls(options: { call: Call }) {
  const { call } = options;
  const [open, setOpen] = createSignal(false);
  const [view, setView] = createSignal<PullsView | null>(null);
  const [tab, setTab] = createSignal<Tab>("all");
  const [only, setOnly] = createSignal<string | null>(null);
  const [query, setQuery] = createSignal("");
  const [selected, setSelected] = createSignal(0);
  const [notice, setNotice] = createSignal("");
  const [unread, setUnread] = createSignal<string | null>(null);
  const [now, setNow] = createSignal(Date.now() / 1000);
  const [face, setFace] = createSignal<{ login: string; image: string } | null>(null);
  const all = () => view()?.pulls ?? [];
  const shown = createMemo(() => shownOf(all(), tab(), only(), query()));
  const counts = createMemo(() => countsOf(all(), only()));
  let opened = 0;
  let refreshes = 0;
  let avatarAsked = -1;
  let ticker: ReturnType<typeof setInterval> | undefined;
  let retry: ReturnType<typeof setTimeout> | undefined;

  const at = () => Math.max(0, Math.min(selected(), shown().length - 1));
  const selectedPull = () => shown()[at()];
  const onlyLabel = () => {
    const narrowed = only();
    return narrowed ? labelOf(narrowed, all()) : null;
  };
  const gate = (): "allow" | "signIn" | null => {
    const current = view();
    if (!current) {
      return null;
    }
    const kind = current.problem?.kind;
    if (kind === "missing" || kind === "signedOut") {
      return "signIn";
    }
    return current.allowed ? null : "allow";
  };
  const reading = () => {
    const current = view();
    return current !== null && current.fetchedAt === null && current.refreshing;
  };
  const failedFirst = () => {
    const current = view();
    return current && current.fetchedAt === null && !current.refreshing && gate() === null
      ? current.problem
      : null;
  };
  const canAsk = () => shown().length > 0 && !reading() && !failedFirst();
  const lost = () => {
    const narrowed = only();
    return narrowed && view()?.lost.some((repo) => sameName(repo, narrowed)) ? narrowed : null;
  };

  const avatar = () => {
    const login = view()?.login;
    const known = face();
    return login && known && sameName(known.login, login) ? known.image : null;
  };

  const askAvatar = () => {
    const login = view()?.login;
    if (gate() !== "allow" || !login || avatarAsked === opened) {
      return;
    }
    avatarAsked = opened;
    call<string | null>("pull_requests_avatar").then(
      (image) => {
        if (image?.startsWith("data:image/")) {
          setFace({ login, image });
        }
      },
      () => undefined,
    );
  };

  const take = (next: PullsView) => {
    const before = selectedPull();
    const was = at();
    batch(() => {
      setView(next);
      setNow(Date.now() / 1000);
    });
    setSelected(keptSelection(before, was, shown()));
    askAvatar();
    clearTimeout(retry);
    const retryAt = next.problem?.kind === "limited" ? next.problem.retryAt : undefined;
    if (retryAt !== undefined) {
      retry = setTimeout(() => void refresh(true), Math.max(0, retryAt * 1000 - Date.now()));
    } else if (next.allowed && next.fetchedAt === null && next.refreshing) {
      retry = setTimeout(() => void refresh(false), STILL_READING_MS);
    }
  };

  const refresh = async (force: boolean) => {
    const mine = ++refreshes;
    const session = opened;
    const current = view();
    if (current) {
      setView({ ...current, refreshing: true });
    }
    try {
      const next = await call<PullsView>("pull_requests_refresh", { force });
      if (mine === refreshes && session === opened && open()) {
        take(next);
      }
    } catch (error) {
      if (mine === refreshes && session === opened && open()) {
        clearTimeout(retry);
        const latest = view();
        const text = problemOf(error);
        if (latest?.fetchedAt === null) {
          setView({ ...latest, refreshing: false, problem: { kind: "failed", text } });
        } else {
          if (latest) {
            setView({ ...latest, refreshing: false });
          }
          setNotice(text);
        }
      }
    }
  };

  const isPinned = (repo: string) => view()?.pinned.some((one) => sameName(one, repo)) ?? false;

  const read = async (picked?: Picked) => {
    const mine = opened;
    setUnread(null);
    const found = await call<PullsView>("pull_requests", picked).catch((error: unknown) => {
      if (mine === opened) {
        setUnread(problemOf(error));
      }
      return null;
    });
    if (mine !== opened || !found) {
      return;
    }
    take(found);
    if (found.allowed) {
      void refresh(gate() === "signIn");
    }
  };

  const show = async (start?: PullsStart, picked?: Picked) => {
    ++opened;
    batch(() => {
      setOpen(true);
      setTab(start?.tab ?? "all");
      setOnly(start?.only ?? null);
      setQuery(start?.query ?? "");
      setSelected(0);
      setNotice("");
      setNow(Date.now() / 1000);
    });
    clearInterval(ticker);
    ticker = setInterval(() => setNow(Date.now() / 1000), TICK_MS);
    await read(picked);
  };

  const hide = () => {
    opened++;
    refreshes++;
    clearInterval(ticker);
    clearTimeout(retry);
    batch(() => {
      setOpen(false);
      setView(null);
      setNotice("");
      setUnread(null);
    });
  };

  const filter = (text: string) => {
    batch(() => {
      setQuery(text);
      setSelected(0);
      setNotice("");
    });
  };

  const showTab = (next: Tab) => {
    batch(() => {
      setTab(next);
      setSelected(0);
      setNotice("");
    });
  };

  const narrow = (next: string | null) => {
    batch(() => {
      setOnly(next);
      setSelected(0);
      setNotice("");
    });
  };

  const page = (by: number) => {
    const count = shown().length;
    if (count > 0) {
      setSelected(clamp(at() + by, count));
    }
  };

  const move = (by: number) => {
    if (shown().length > 0) {
      setNotice("");
    }
    page(by);
  };

  const openPull = (pull: Pull | undefined) => {
    if (pull) {
      call("open_link", { url: pull.url }).catch((error: unknown) => setNotice(problemOf(error)));
    }
  };

  const openSelected = () => openPull(selectedPull());

  const copyLink = (pull: Pull | undefined) => {
    if (pull) {
      call("copy_text", { text: pull.url }).then(
        () => setNotice("Copied the link"),
        (error: unknown) => setNotice(problemOf(error)),
      );
    }
  };

  const allow = () => {
    const session = opened;
    const current = () => session === opened && open();
    call<PullsView>("pull_requests_allow").then(
      (next) => {
        if (current()) {
          take(next);
          void refresh(false);
        }
      },
      (error: unknown) => {
        if (current()) {
          setNotice(problemOf(error));
        }
      },
    );
  };

  const copySignIn = () => {
    call("copy_text", { text: SIGN_IN_COMMAND }).then(
      () => setNotice(`Copied ${SIGN_IN_COMMAND}. Run it in Terminal, then open this again.`),
      (error: unknown) => setNotice(problemOf(error)),
    );
  };

  const pin = (repo: string, on: boolean) => {
    const session = opened;
    call<string[]>("pull_requests_pin", { repo, pin: on }).then(
      (pinned) => {
        const current = view();
        if (session === opened && current && Array.isArray(pinned)) {
          setView({ ...current, pinned });
        }
      },
      (error: unknown) => {
        if (session === opened && open()) {
          setNotice(problemOf(error));
        }
      },
    );
  };

  const togglePin = (pull: Pull | undefined) => {
    if (pull) {
      const repo = repoOf(pull);
      pin(repo, !isPinned(repo));
    }
  };

  const unpinLost = () => {
    const repo = lost();
    if (repo) {
      pin(repo, false);
      narrow(null);
    }
    return repo !== null;
  };

  const pinnedLabel = (repo: string) => labelOf(repo, all());

  const quickFilters = () =>
    (view()?.pinned ?? []).map((repo, index) => ({ repo, keys: quickFilterKeys(index) }));

  const act = () => {
    switch (gate()) {
      case "allow":
        allow();
        return;
      case "signIn":
        copySignIn();
        return;
      default:
        openSelected();
    }
  };

  const primary = (): Action | undefined => {
    switch (gate()) {
      case "allow":
        return { title: "Continue", keys: OPEN_KEYS, hides: false, run: allow };
      case "signIn":
        return { title: `Copy ${SIGN_IN_COMMAND}`, keys: OPEN_KEYS, hides: false, run: copySignIn };
      default:
        return selectedPull()
          ? { title: "Open in Browser", keys: OPEN_KEYS, hides: true, run: openSelected }
          : undefined;
    }
  };

  const actions = (pickers: {
    organizations: () => void;
    repositories: () => void;
    ask: () => void;
  }): Action[] => {
    if (gate() !== null || !view()) {
      return [];
    }
    const pull = selectedPull();
    const forPull: Action[] = pull
      ? [
          { title: "Open in Browser", keys: OPEN_KEYS, hides: true, run: () => openPull(pull) },
          { title: "Copy Link", keys: COPY_LINK_KEYS, hides: false, run: () => copyLink(pull) },
        ]
      : [];
    const pinned = pull ? isPinned(repoOf(pull)) : false;
    const askable: Action[] = canAsk()
      ? [{ title: "Ask AI About These", keys: "", hides: false, run: pickers.ask }]
      : [];
    return [
      ...forPull,
      ...askable,
      ...quickFilters().map(({ repo, keys }): Action => ({
        title: pinnedLabel(repo),
        about: `${pinnedLabel(repo)} ${repo}`,
        note: "pinned",
        keys,
        hides: false,
        section: SHOW_ONLY,
        run: () => narrow(repo),
      })),
      {
        title: "An organization…",
        keys: ORGS_KEYS,
        hides: false,
        section: SHOW_ONLY,
        run: pickers.organizations,
      },
      {
        title: "A repository…",
        keys: REPOS_KEYS,
        hides: false,
        section: SHOW_ONLY,
        run: pickers.repositories,
      },
      ...(pull
        ? [
            {
              title: `${pinned ? "Unpin" : "Pin"} ${pull.shown}`,
              keys: PIN_KEYS,
              hides: false,
              section: SHOW_ONLY,
              run: () => togglePin(pull),
            },
          ]
        : []),
    ];
  };

  const organizationActions = (): Action[] =>
    (view()?.owners ?? []).map((owner) => ({
      title: owner.name,
      note: `${owner.open} open`,
      keys: "",
      hides: false,
      run: () => narrow(owner.name),
    }));

  const repositoryActions = (): Action[] =>
    repoChoices(all()).map((choice) => ({
      title: choice.shown,
      about: `${choice.shown} ${choice.repo}`,
      note: `${choice.open} open`,
      keys: "",
      hides: false,
      run: () => narrow(choice.repo),
    }));

  const shortcut = (keys: string) => {
    if (keys === REFRESH_KEYS && open()) {
      if (unread() !== null) {
        void read();
        return true;
      }
      if (view()?.allowed) {
        void refresh(true);
        return true;
      }
    }
    if (gate() !== null || !view()) {
      return false;
    }
    const numbered = TABS.findIndex((_, index) => keys === `⌘ ${index + 1}`);
    const chosen = TABS[numbered];
    if (chosen) {
      showTab(chosen.id);
      return true;
    }
    const quick = quickFilters().find(
      (candidate) => candidate.keys !== "" && candidate.keys === keys,
    );
    if (quick) {
      narrow(quick.repo);
      return true;
    }
    switch (keys) {
      case COPY_LINK_KEYS:
        copyLink(selectedPull());
        return selectedPull() !== undefined;
      case PIN_KEYS:
        togglePin(selectedPull());
        return selectedPull() !== undefined;
      case UNPIN_KEYS:
        return unpinLost();
      default:
        return false;
    }
  };

  const context = () => {
    const current = view();
    if (notice()) {
      return notice();
    }
    if (!current || gate() !== null) {
      return "";
    }
    const problem = current.problem;
    if (
      problem &&
      !failedFirst() &&
      (problem.kind === "offline" || problem.kind === "limited" || problem.kind === "failed")
    ) {
      return problemLine(problem, current.fetchedAt);
    }
    return "";
  };

  const askArgs = () => {
    const typed = query().trim();
    return typed
      ? { tab: tab(), only: only(), query: typed, urls: shown().map((pull) => pull.url) }
      : { tab: tab(), only: only(), query: "" };
  };

  const ask = async (): Promise<PullsContext | null> => {
    const asked = { opening: opened, tab: tab(), only: only(), query: query() };
    const current = () =>
      open() &&
      opened === asked.opening &&
      tab() === asked.tab &&
      only() === asked.only &&
      query() === asked.query;
    try {
      const context = await call<PullsContext>("pull_requests_context", askArgs());
      return current() ? context : null;
    } catch (error) {
      if (current()) {
        throw error;
      }
      return null;
    }
  };

  return {
    open,
    view,
    loaded: () => view() !== null,
    unread,
    gate,
    avatar,
    reading,
    failedFirst,
    canAsk,
    lost,
    tab,
    only,
    onlyLabel,
    query,
    shown,
    counts,
    now,
    selected: at,
    select: setSelected,
    selectedPull,
    outside: () => outsideCount(all(), tab(), only(), query()),
    show,
    hide,
    filter,
    showTab,
    narrow,
    move,
    page,
    act,
    primary,
    actions,
    organizationActions,
    repositoryActions,
    shortcut,
    context,
    ask,
    setNotice,
  };
}

export type Pulls = ReturnType<typeof createPulls>;

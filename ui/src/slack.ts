import { batch, createMemo, createSignal } from "solid-js";
import type { Action } from "./actions";
import { clamp } from "./list";

export type SlackKind = "person" | "channel" | "group" | "message";
export type Hit = {
  id: string;
  kind: SlackKind;
  title: string;
  subtitle: string;
  unread: number;
  mention: boolean;
  updated: number;
  link: string;
};
export type ProblemKind = "noToken" | "signedOut" | "offline" | "limited" | "failed";
export type Problem = { kind: ProblemKind; text: string; retryAt?: number };
export type SlackView = { hasToken: boolean; hits: Hit[]; problem?: Problem };

type Call = <T>(command: string, args?: Record<string, unknown>) => Promise<T>;

export const SLACK_TITLE = "Slack";
export const SLACK_PLACEHOLDER = "Search Slack";
export const TOKEN_PLACEHOLDER = "Paste your Slack User OAuth Token (xoxp-…)";
export const APPS_URL = "https://api.slack.com/apps?new_app=1";
export const OPEN_KEYS = "↵";
export const COPY_LINK_KEYS = "⌘ ⇧ C";
export const SIGN_OUT_KEYS = "⌘ ⌫";
export const SEARCH_WAIT = 180;
export const STEPS = [
  "Open Slack’s app builder and choose From a manifest.",
  "Pick your workspace, paste Raiden’s manifest, and create the app.",
  "Install to Workspace, then copy the User OAuth Token (xoxp-…).",
  "Paste it above and press ↵.",
];

const KIND_LABEL: Record<SlackKind, string> = {
  person: "Direct message",
  channel: "Channel",
  group: "Group",
  message: "Message",
};

export function detailOf(hit: Hit): string {
  if (hit.subtitle) {
    return hit.kind === "message" ? hit.subtitle : `${KIND_LABEL[hit.kind]} · ${hit.subtitle}`;
  }
  return KIND_LABEL[hit.kind];
}

// Pulls a lone xoxp token out of whatever got pasted, so stray quotes or
// "Bearer " wrappers do not reach the Keychain.
export function cleanToken(raw: string): string {
  const match = raw.match(/xox[a-z]-[A-Za-z0-9-]+/);
  return match ? match[0] : raw.trim();
}

const problemOf = (error: unknown) =>
  error instanceof Error ? error.message : typeof error === "string" ? error : String(error);

export function createSlack(options: { call: Call }) {
  const { call } = options;
  const [open, setOpen] = createSignal(false);
  const [loaded, setLoaded] = createSignal(false);
  const [hasToken, setHasToken] = createSignal(false);
  const [hits, setHits] = createSignal<Hit[]>([]);
  const [query, setQuery] = createSignal("");
  const [selected, setSelected] = createSignal(0);
  const [problem, setProblem] = createSignal<Problem | null>(null);
  const [notice, setNotice] = createSignal("");
  const [busy, setBusy] = createSignal(false);
  let opened = 0;
  let queried = 0;
  let timer: ReturnType<typeof setTimeout> | undefined;

  const shown = createMemo(() => hits());
  const at = () => Math.max(0, Math.min(selected(), shown().length - 1));
  const selectedHit = () => shown()[at()];
  // Onboarding shows only once the first reply says there is no token, so a
  // signed-in person never flashes the gate and early keystrokes are not lost.
  const gate = () => (loaded() && !hasToken() ? "token" : null);

  const take = (view: SlackView, mine: number) => {
    if (mine !== opened) {
      return;
    }
    batch(() => {
      setHasToken(view.hasToken);
      setHits(Array.isArray(view.hits) ? view.hits : []);
      setProblem(view.problem ?? null);
      setLoaded(true);
    });
  };

  const run = (text: string) => {
    const mine = opened;
    const query = queried;
    const current = () => mine === opened && query === queried;
    call<SlackView>("slack_search", { query: text })
      // Only the newest query wins, so a slow reply cannot replace newer hits
      // or raise a notice after a later query has already answered.
      .then((view) => current() && take(view, mine))
      .catch((error: unknown) => current() && setNotice(problemOf(error)));
  };

  const show = async () => {
    const mine = ++opened;
    batch(() => {
      setOpen(true);
      setLoaded(false);
      setQuery("");
      setSelected(0);
      setProblem(null);
      setNotice("");
      setBusy(false);
    });
    run("");
    void mine;
  };

  const hide = () => {
    opened++;
    clearTimeout(timer);
    batch(() => {
      setOpen(false);
      setLoaded(false);
      setHits([]);
      setQuery("");
      setNotice("");
      setBusy(false);
    });
  };

  const filter = (text: string) => {
    // Each edit retires any query still in flight, so its late reply is dropped.
    queried++;
    batch(() => {
      setQuery(text);
      setSelected(0);
    });
    if (gate()) {
      return;
    }
    clearTimeout(timer);
    timer = setTimeout(() => run(text.trim()), SEARCH_WAIT);
  };

  const move = (by: number) => {
    const count = shown().length;
    if (count > 0) {
      setSelected(clamp(at() + by, count));
    }
  };

  const saveToken = () => {
    const token = cleanToken(query());
    if (!token || busy()) {
      return;
    }
    const mine = opened;
    batch(() => {
      setBusy(true);
      setNotice("Checking the token with Slack…");
    });
    call<SlackView>("slack_save_token", { token })
      .then((view) => {
        if (mine !== opened) {
          return;
        }
        take(view, mine);
        batch(() => {
          if (view.hasToken && !view.problem) {
            setNotice("");
            setQuery("");
          } else {
            setNotice(view.problem?.text ?? "That token did not work.");
          }
        });
      })
      .catch((error: unknown) => mine === opened && setNotice(problemOf(error)))
      .finally(() => mine === opened && setBusy(false));
  };

  const openHit = () => {
    const hit = selectedHit();
    if (!hit) {
      return;
    }
    // A Slack destination, not a web link: the native side validates the
    // slack:// or permalink and hides the launcher on success.
    call("slack_open", { url: hit.link }).catch((error: unknown) => setNotice(problemOf(error)));
  };

  const copyLink = () => {
    const hit = selectedHit();
    if (!hit) {
      return;
    }
    call("copy_text", { text: hit.link }).then(
      () => setNotice("Copied the link"),
      (error: unknown) => setNotice(problemOf(error)),
    );
  };

  const copyManifest = () => {
    call<string>("slack_manifest").then(
      (text) => call("copy_text", { text }).then(() => setNotice("Copied the manifest")),
      (error: unknown) => setNotice(problemOf(error)),
    );
  };

  const openBuilder = () => {
    call("open_link", { url: APPS_URL }).catch(() => undefined);
  };

  const signOut = () => {
    const mine = ++opened;
    call<SlackView>("slack_sign_out")
      .then((view) => {
        if (mine === opened) {
          take(view, mine);
          setQuery("");
        }
      })
      .catch((error: unknown) => mine === opened && setNotice(problemOf(error)));
  };

  const act = () => {
    if (gate()) {
      saveToken();
    } else {
      openHit();
    }
  };

  const actions = (): Action[] => {
    if (gate()) {
      return [
        { title: "Save token", keys: OPEN_KEYS, hides: false, run: saveToken },
        { title: "Copy manifest", keys: "⌘ ⇧ M", hides: false, run: copyManifest },
        { title: "Open Slack app builder", keys: "⌘ O", hides: true, run: openBuilder },
      ];
    }
    const list: Action[] = [];
    if (selectedHit()) {
      list.push({ title: "Open in Slack", keys: OPEN_KEYS, hides: true, run: openHit });
      list.push({ title: "Copy Link", keys: COPY_LINK_KEYS, hides: false, run: copyLink });
    }
    // Always offer sign-out, so a revoked token can be replaced even when the
    // list is empty.
    list.push({ title: "Sign out of Slack", keys: SIGN_OUT_KEYS, hides: false, run: signOut });
    return list;
  };

  const primary = () => {
    if (gate()) {
      return { title: "Save token", keys: OPEN_KEYS, hides: false, run: saveToken };
    }
    return selectedHit()
      ? { title: "Open in Slack", keys: OPEN_KEYS, hides: true, run: openHit }
      : undefined;
  };

  const shortcut = (keys: string) => {
    if (gate()) {
      if (keys === "⌘ ⇧ M") {
        copyManifest();
        return true;
      }
      if (keys === "⌘ O") {
        openBuilder();
        return true;
      }
      return false;
    }
    if (keys === COPY_LINK_KEYS && selectedHit()) {
      copyLink();
      return true;
    }
    if (keys === SIGN_OUT_KEYS) {
      signOut();
      return true;
    }
    return false;
  };

  return {
    open,
    loaded,
    hasToken,
    gate,
    query,
    shown,
    selected: at,
    select: setSelected,
    selectedHit,
    problem,
    busy,
    show,
    hide,
    filter,
    move,
    page: move,
    act,
    saveToken,
    copyManifest,
    openBuilder,
    actions,
    primary,
    shortcut,
    context: () =>
      notice() || problem()?.text || (gate() ? "Install Raiden in your Slack to search it." : ""),
  };
}

export type Slack = ReturnType<typeof createSlack>;

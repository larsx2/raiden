import { batch, createMemo, createSignal } from "solid-js";
import { createStore, reconcile } from "solid-js/store";
import type { Action } from "./actions";
import { ageLabel, type Front } from "./ask";
import { clamp } from "./list";

export type ClipKind = "text" | "link" | "file" | "image";
export type Clip = {
  id: string;
  kind: ClipKind;
  title: string;
  text: string;
  app: string;
  at: number;
  width: number;
  height: number;
  thumb: boolean;
};

type Call = <T>(command: string, args?: Record<string, unknown>) => Promise<T>;

export const CLIPBOARD_PLACEHOLDER = "Search clipboard";
export const CLIPBOARD_TITLE = "Clipboard History";
export const COPY_KEYS = "⌘ C";
export const DELETE_KEYS = "⌘ ⌫";
export const KEPT_NOTE = "Kept for 7 days, never passwords";
const DAY_MS = 24 * 60 * 60 * 1000;

const KIND_LABELS: Record<ClipKind, string> = {
  text: "Text",
  link: "Link",
  file: "File",
  image: "Image",
};

const lowerFirst = (text: string) => text.charAt(0).toLowerCase() + text.slice(1);

export function subtitleOf(clip: Clip, now: number): string {
  const from = clip.app ? ` from ${clip.app}` : "";
  return `${KIND_LABELS[clip.kind]}${from}, ${lowerFirst(ageLabel(clip.at, now))}`;
}

export function filterClips(clips: Clip[], query: string): Clip[] {
  const wanted = query.toLowerCase().split(/\s+/).filter(Boolean);
  if (wanted.length === 0) {
    return clips;
  }
  return clips.filter((clip) => {
    const words =
      `${clip.title}\n${clip.text}\n${clip.app}\n${KIND_LABELS[clip.kind]}`.toLowerCase();
    return wanted.every((part) => words.includes(part));
  });
}

const startOfDay = (ms: number) => {
  const day = new Date(ms);
  day.setHours(0, 0, 0, 0);
  return day.getTime();
};

export function dayOf(at: number, now: number): string {
  const today = startOfDay(now * 1000);
  const then = at * 1000;
  if (then >= today) {
    return "Today";
  }
  return then >= startOfDay(today - DAY_MS / 2) ? "Yesterday" : "Earlier";
}

export function sectionsOf(clips: Clip[], now: number): Array<[day: string, clips: Clip[]]> {
  const sections: Array<[string, Clip[]]> = [];
  for (const clip of clips) {
    const day = dayOf(clip.at, now);
    const last = sections.at(-1);
    if (last && last[0] === day) {
      last[1].push(clip);
    } else {
      sections.push([day, [clip]]);
    }
  }
  return sections;
}

const problemOf = (error: unknown) =>
  error instanceof Error ? error.message : typeof error === "string" ? error : String(error);

export function createClipboard(options: { call: Call }) {
  const { call } = options;
  const [open, setOpen] = createSignal(false);
  const [clips, setClips] = createSignal<Clip[]>([]);
  const [loaded, setLoaded] = createSignal(false);
  const [query, setQuery] = createSignal("");
  const [selected, setSelected] = createSignal(0);
  const [notice, setNotice] = createSignal("");
  const [app, setApp] = createSignal("");
  const [now, setNow] = createSignal(Date.now() / 1000);
  const [thumbs, setThumbs] = createStore<Record<string, string | undefined>>({});
  const requested = new Set<string>();
  const shown = createMemo(() => filterClips(clips(), query()));
  const sections = createMemo(() => sectionsOf(shown(), now()));
  const indexes = createMemo(() => new Map(shown().map((clip, at) => [clip.id, at])));
  let opened = 0;

  const at = () => Math.max(0, Math.min(selected(), shown().length - 1));
  const selectedClip = () => shown()[at()];

  const show = async (picked?: { query: string; row: number }) => {
    const mine = ++opened;
    batch(() => {
      setOpen(true);
      setQuery("");
      setSelected(0);
      setNotice("");
      setNow(Date.now() / 1000);
    });
    void call<Front | null>("front_context")
      .then((front) => mine === opened && setApp(front?.app ?? ""))
      .catch(() => undefined);
    const found = await call<Clip[] | null>("clipboard_items", picked).catch((error: unknown) => {
      setNotice(problemOf(error));
      return null;
    });
    if (mine === opened) {
      batch(() => {
        setClips(Array.isArray(found) ? found : []);
        setLoaded(true);
      });
    }
  };

  const hide = () => {
    opened++;
    batch(() => {
      setOpen(false);
      setNotice("");
      setLoaded(false);
      setClips([]);
      setThumbs(reconcile({}));
    });
    requested.clear();
  };

  const filter = (text: string) => {
    batch(() => {
      setQuery(text);
      setSelected(0);
    });
  };

  const move = (by: number) => {
    const count = shown().length;
    if (count > 0) {
      setSelected(clamp(at() + by, count));
    }
  };

  const page = move;

  const thumb = (id: string) => {
    if (!requested.has(id)) {
      requested.add(id);
      call<string | null>("clipboard_thumb", { id }).then(
        (url) => {
          if (url) {
            setThumbs(id, url);
          } else {
            requested.delete(id);
          }
        },
        () => requested.delete(id),
      );
    }
    return thumbs[id];
  };

  let pasting = false;

  const paste = () => {
    const clip = selectedClip();
    if (!clip || pasting) {
      return;
    }
    pasting = true;
    setNotice("");
    call("clipboard_paste", { id: clip.id })
      .catch((error: unknown) => setNotice(problemOf(error)))
      .finally(() => {
        pasting = false;
      });
  };

  const copy = () => {
    const clip = selectedClip();
    if (!clip) {
      return;
    }
    call("clipboard_copy", { id: clip.id }).then(
      () =>
        batch(() => {
          const raised = { ...clip, at: Date.now() / 1000 };
          setClips((before) => [raised, ...before.filter((other) => other.id !== clip.id)]);
          setNow(Date.now() / 1000);
          setSelected(0);
          setNotice("Copied to the clipboard");
        }),
      (error: unknown) => setNotice(problemOf(error)),
    );
  };

  const remove = () => {
    const clip = selectedClip();
    if (!clip) {
      return;
    }
    call("clipboard_delete", { id: clip.id }).then(
      () =>
        batch(() => {
          setClips((before) => before.filter((other) => other.id !== clip.id));
          setThumbs(clip.id, undefined);
          requested.delete(clip.id);
          setNotice("Deleted from Clipboard history");
        }),
      (error: unknown) => setNotice(problemOf(error)),
    );
  };

  const pasteTitle = () => (app() ? `Paste into ${app()}` : "Paste");

  const actions = (): Action[] =>
    selectedClip()
      ? [
          { title: pasteTitle(), keys: "↵", hides: true, run: paste },
          { title: "Copy", keys: COPY_KEYS, hides: false, run: copy },
          { title: "Delete", keys: DELETE_KEYS, hides: false, run: remove },
        ]
      : [];

  const shortcut = (keys: string) => {
    if (keys === COPY_KEYS && selectedClip()) {
      copy();
      return true;
    }
    if (keys === DELETE_KEYS && selectedClip()) {
      remove();
      return true;
    }
    return false;
  };

  return {
    open,
    loaded,
    query,
    shown,
    sections,
    indexOf: (clip: Clip) => indexes().get(clip.id) ?? -1,
    now,
    selected: at,
    select: setSelected,
    selectedClip,
    show,
    hide,
    filter,
    move,
    page,
    thumb,
    act: paste,
    actions,
    primary: () => actions()[0],
    shortcut,
    context: () => notice() || KEPT_NOTE,
  };
}

export type Clipboard = ReturnType<typeof createClipboard>;

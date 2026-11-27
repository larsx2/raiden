import { createVirtualizer } from "@tanstack/solid-virtual";
import {
  For,
  Index,
  Match,
  Show,
  Switch,
  batch,
  createEffect,
  createMemo,
  createSignal,
  on,
  onCleanup,
  onMount,
  type JSX,
} from "solid-js";
import { createStore } from "solid-js/store";
import { appActions, filterActions, type Action } from "./actions";
import {
  ageLabel,
  findable,
  hiddenLabel,
  matchesAnswer,
  modelLabel,
  titleOf,
  type AnswerEvent,
  type Conversation,
} from "./ask";
import { createAsking } from "./asking";
import { ClipboardList } from "./ClipboardList";
import { CLIPBOARD_PLACEHOLDER, CLIPBOARD_TITLE, DELETE_KEYS, createClipboard } from "./clipboard";
import {
  searchesFiles,
  stillMatching,
  wordPositions,
  type FileAnswer,
  type FileRow,
} from "./files";
import { createFrameWatcher } from "./frames";
import { handleKey, stepsBack, switchesToInsert } from "./keys";
import {
  pieces,
  sectionsOf,
  slotAt,
  slotCount,
  slotOf,
  startingRows,
  startsSection,
  clamp,
  type Section,
  type Slot,
} from "./list";
import { plainOf } from "./markdown";
import { AiIcon, Brand, Check, GitHubMark, Glyph, Keys, MenuIcon, NoMatch } from "./parts";
import {
  ORGS_KEYS,
  PULLS_PLACEHOLDER,
  PULLS_TITLE,
  REPOS_KEYS,
  createPulls,
  tabAfter,
  type Picked,
  type PullsStart,
} from "./pulls";
import { PullsList } from "./PullsList";
import { Reply } from "./Reply";
import { SLACK_PLACEHOLDER, SLACK_TITLE, TOKEN_PLACEHOLDER, createSlack } from "./slack";
import { SlackList, SlackMark } from "./SlackList";
import type { Search } from "./search";
import { SETTINGS_PLACEHOLDER, createSettings, type Pressed } from "./settings";
import { SettingsList } from "./SettingsList";
import { OPEN_IN_TERMINAL_KEYS } from "./terminals";
import { afterNextPaint, nextTask } from "./speed";
import { applyTyped, catchUp, type Typed } from "./typeahead";
import tauri from "../../tauri.conf.json";
import { SIZE_KEYS, createWindowSize, rowsGained } from "./window";

export const SEARCH_PLACEHOLDER = "Search apps and commands";
export const FOLLOW_UP_PLACEHOLDER = "Ask a follow-up";
const ROW_HEIGHT = 40;
const HEADING_HEIGHT = 29.5;
const SUGGESTIONS = 5;
const RECENT_SHOWN = 3;
const MOST_FOUND_ANSWERS = 5;
const NO_POSITIONS = new Uint32Array(0);
const PRESS_MS = 120;
const FOLLOW_SLACK = 24;
const CLIPS_PER_PAGE = 5;
const PULLS_PER_PAGE = 6;
const ANSWER_PAGE = 0.8;
const HALF_PAGE = 0.5;
const ANSWER_LINES = 3;
const ANSWER_LINE = 21;
const NORMAL_SAID = "Normal mode. j and k move, i types.";
const INSERT_SAID = "Insert mode.";
const EARLY_MS = 500;
const CLIP_HEIGHT = 48;
const PULL_HEIGHT = 49;
const COMPACT_HEIGHT = tauri.app.windows[0]?.height ?? 0;

export type Call = <T>(command: string, args?: Record<string, unknown>) => Promise<T>;
export type Listen = (handler: (event: AnswerEvent) => void) => Promise<() => void>;
export type Hear = (handler: (keys: string) => void) => Promise<() => void>;
export type Opens = (handler: (shown: boolean) => void) => Promise<() => void>;
export type InstalledApp = { name: string; path: string; under?: string; accessory?: string };
export type PullsOpening = PullsStart & { menu?: boolean };
export type Listing = { apps: InstalledApp[]; catalog: string };
type Menu = {
  title: string;
  actions: Action[];
  noun?: string;
  tall?: boolean;
  primary?: string;
  chooser?: boolean;
};
type Press = { where: "row" | "action"; index: number };
type Extra =
  | { kind: "file"; file: FileRow }
  | { kind: "answer"; conversation: Conversation }
  | { kind: "ask"; question: string };

const COMMAND = "raiden:";
const GRAMMAR = "raiden:fix-grammar";
const SETTINGS_VIEW = "raiden:settings";
const SETTINGS_FILE = "raiden:settings-file";
const CLIPBOARD_VIEW = "raiden:clipboard";
const PULLS_VIEW = "raiden:pull-requests";
const SLACK_VIEW = "raiden:slack";
const isCommand = (app: InstalledApp | undefined) => app?.path.startsWith(COMMAND) === true;
const isAiCommand = (app: InstalledApp | undefined) => app?.path === GRAMMAR;
const SETTING = "x-apple.systempreferences:";
const SETTINGS_APP = "/System/Applications/System Settings.app";
const isSetting = (app: InstalledApp | undefined) => app?.path.startsWith(SETTING) === true;
const iconPath = (app: InstalledApp | undefined) =>
  isSetting(app) ? SETTINGS_APP : (app?.path ?? "");

const headingOf = (slot: Slot) => ("heading" in slot ? slot.heading : undefined);
const rowOf = (slot: Slot) => ("row" in slot ? slot.row : -1);
const problemOf = (error: unknown) => (typeof error === "string" ? error : String(error));
const extraKey = (extra: Extra) =>
  extra.kind === "file"
    ? `file ${extra.file.path}`
    : extra.kind === "answer"
      ? `answer ${extra.conversation.id}`
      : "ask";

const nextFrame = (callback: () => void) => {
  requestAnimationFrame(() => callback());
};

export function App(props: {
  call: Call;
  search: () => Promise<Search>;
  listen: Listen;
  hear: Hear;
  opens?: Opens;
  pulls?: PullsOpening;
  earlyMs?: number;
}): JSX.Element {
  let field: HTMLInputElement | undefined;
  let menuField: HTMLInputElement | undefined;
  let list: HTMLDivElement | undefined;
  let answerView: HTMLDivElement | undefined;
  let search: Search | undefined;
  let openWhenReady = false;
  const [apps, setApps] = createSignal<InstalledApp[]>([]);
  const [results, setResults] = createSignal<ArrayLike<number>>([]);
  const [extras, setExtras] = createSignal<Extra[]>([]);
  const [sections, setSections] = createSignal<Section[]>([]);
  const [files, setFiles] = createSignal<FileRow[]>([]);
  const [filesPending, setFilesPending] = createSignal(false);
  const [indexOff, setIndexOff] = createSignal(false);
  const [nothingFound, setNothingFound] = createSignal("");
  const [typed, setTyped] = createSignal("");
  const [draft, setDraft] = createSignal("");
  const [selected, setSelected] = createSignal(0);
  const [icons, setIcons] = createStore<Record<string, string>>({});
  const [menu, setMenu] = createSignal<Menu | null>(null);
  const [menuQuery, setMenuQuery] = createSignal("");
  const [menuSelected, setMenuSelected] = createSignal(0);
  const [pressed, setPressed] = createSignal<Press | null>(null);
  const [answering, setAnswering] = createSignal(false);
  const [saved, setSaved] = createSignal<Conversation[]>([]);
  const [vimOn, setVimOn] = createSignal(false);
  const [mode, setMode] = createSignal<"insert" | "normal">("insert");
  const [waitingG, setWaitingG] = createSignal(false);
  const [modeSaid, setModeSaid] = createSignal("");
  const [searchNote, setSearchNote] = createSignal("");
  let noteFor = 0;
  const forgetNote = () => {
    noteFor++;
    setSearchNote("");
  };
  const normal = () => vimOn() && mode() === "normal";
  const findables = createMemo(() => saved().map(findable));
  const shownActions = createMemo(() => filterActions(menu()?.actions ?? [], menuQuery()));
  const requested = new Set<string>();

  const asking = createAsking({
    call: props.call,
    frame: nextFrame,
    saved: (kept) =>
      setSaved((before) => [kept, ...before.filter((other) => other.id !== kept.id)]),
  });

  const prefs = createSettings({
    call: props.call,
    accountOf: (id) => asking.accountOf(id),
    refreshAccounts: () => asking.refreshAccounts(),
    refreshChoice: async () => {
      await asking.refreshChoice();
    },
    vimChanged: (on) => setVimOn(on),
    showMenu: (shown) => {
      batch(() => {
        setMenuQuery("");
        setMenuSelected(0);
        setMenu(shown);
      });
    },
  });
  const clip = createClipboard({ call: props.call });
  const pulls = createPulls({ call: props.call });
  const slk = createSlack({ call: props.call });
  // The token is typed into the shared search field; clear that field whenever
  // onboarding opens or closes so a pasted token is never left to be sent as a
  // search once the view becomes a search again.
  createEffect(
    on(
      slk.gate,
      (gate, was) => {
        if (field && was !== gate) {
          field.value = "";
        }
      },
      { defer: true },
    ),
  );
  let returnTo = "";
  let pullsReturnTo = "";
  const pullsShown = () => pulls.open() && !answering();
  const covered = () => prefs.open() || clip.open() || pullsShown() || slk.open();
  const chatting = () => answering() && !covered();
  const size = createWindowSize({ call: props.call });

  const layout = createMemo(() => {
    const shown = sections();
    return {
      size: (slot: number) => (headingOf(slotAt(shown, slot)) ? HEADING_HEIGHT : ROW_HEIGHT),
      key: (slot: number) => {
        const at = slotAt(shown, slot);
        return "heading" in at ? `heading ${at.heading}` : at.row;
      },
    };
  });

  const virtualizer = createVirtualizer({
    get count() {
      return slotCount(sections(), results().length);
    },
    getScrollElement: () => list ?? null,
    get estimateSize() {
      return layout().size;
    },
    get getItemKey() {
      return layout().key;
    },
    overscan: 8,
  });

  const icon = (path: string, key = path) => {
    if (path && !path.startsWith(COMMAND) && !requested.has(key)) {
      requested.add(key);
      void props.call<string | null>("icon", { path }).then((url) => {
        if (url) {
          setIcons(key, url);
        }
      });
    }
    return icons[key];
  };

  const frames = createFrameWatcher(
    (ms) => void props.call("frame", { ms }),
    (callback) => requestAnimationFrame(callback),
  );

  const query = () => field?.value ?? "";

  const foundAnswers = (text: string) => {
    const found: Conversation[] = [];
    const all = saved();
    const words = findables();
    for (let at = 0; at < all.length && found.length < MOST_FOUND_ANSWERS; at++) {
      const answer = all[at];
      const answerWords = words[at];
      if (answer && answerWords && matchesAnswer(answerWords, text)) {
        found.push(answer);
      }
    }
    return found;
  };

  let handPicked = false;
  let askedFiles = "";
  let filesAsked = Date.now();
  let rowsOutdated = false;
  let noneAfterFiles = false;

  const keyAt = (row: number) => {
    const index = results()[row];
    if (index === undefined) {
      return "";
    }
    const extra = extras()[index - apps().length];
    return index < apps().length ? `app ${index}` : extra ? extraKey(extra) : "";
  };

  const rowOfKey = (key: string) =>
    Array.from(results(), (_, row) => row).find((row) => keyAt(row) === key) ?? 0;

  const typedText = () => (query().trim() === "" ? "" : query());

  const build = (fresh: boolean) => {
    rowsOutdated = false;
    const text = typedText();
    const count = apps().length;
    const kept = handPicked && !fresh ? keyAt(selected()) : "";
    batch(() => {
      if (search && text) {
        const ranked = search.rank(text);
        const found = foundAnswers(text);
        const shownFiles = files();
        const asked = text.trim();
        const more: Extra[] = [
          ...shownFiles.map((file): Extra => ({ kind: "file", file })),
          ...found.map((conversation): Extra => ({ kind: "answer", conversation })),
          { kind: "ask", question: asked },
        ];
        const rows = new Uint32Array(ranked.length + more.length);
        rows.set(ranked);
        more.forEach((_, at) => {
          rows[ranked.length + at] = count + at;
        });
        setExtras(more);
        setResults(rows);
        setSections(
          sectionsOf([
            ["Results", ranked.length],
            ["Files", shownFiles.length],
            ["Recent answers", found.length],
            [`Use “${asked}” with`, 1],
          ]),
        );
        const before = nothingFound();
        const stillNone =
          noneAfterFiles && before !== "" && asked.toLowerCase().includes(before.toLowerCase());
        const none =
          ranked.length + shownFiles.length + found.length === 0 && (!filesPending() || stillNone);
        noneAfterFiles = none && ((searchesFiles(asked) && !filesPending()) || stillNone);
        setNothingFound(none ? asked : "");
      } else {
        const suggested = search?.suggestions(SUGGESTIONS) ?? NO_POSITIONS;
        const shownFirst = new Set(Array.from(suggested));
        const all = apps();
        const starting = startingRows(count, suggested).filter(
          (index) => shownFirst.has(index) || !isSetting(all[index]),
        );
        const recent = saved().slice(0, RECENT_SHOWN);
        const rows = new Uint32Array(starting.length + recent.length);
        rows.set(starting.subarray(0, suggested.length));
        recent.forEach((_, at) => {
          rows[suggested.length + at] = count + at;
        });
        rows.set(starting.subarray(suggested.length), suggested.length + recent.length);
        const commandsLeft = all.filter(
          (app, index) => isCommand(app) && !shownFirst.has(index),
        ).length;
        setExtras(recent.map((conversation): Extra => ({ kind: "answer", conversation })));
        setResults(rows);
        setSections(
          sectionsOf([
            ["Suggestions", suggested.length],
            ["Recent answers", recent.length],
            ["Applications", starting.length - suggested.length - commandsLeft],
            ["Commands", commandsLeft],
          ]),
        );
        setNothingFound("");
      }
      setTyped(text);
      setSelected(kept === "" ? 0 : rowOfKey(kept));
    });
    if (fresh && (virtualizer.scrollOffset ?? 0) > 0) {
      virtualizer.scrollToOffset(0);
    }
  };

  const lookForFiles = (text: string) => {
    const wanted = text.trim();
    if (!searchesFiles(wanted)) {
      if (askedFiles !== "") {
        void props.call("files", { query: "", asked: ++filesAsked }).catch(() => undefined);
      }
      askedFiles = "";
      filesAsked++;
      batch(() => {
        setFiles([]);
        setFilesPending(false);
      });
      return;
    }
    if (wanted === askedFiles) {
      return;
    }
    askedFiles = wanted;
    const asked = ++filesAsked;
    batch(() => {
      setFiles((before) => stillMatching(before, wanted));
      setFilesPending(true);
    });
    const settle = (answer: FileAnswer | null) => {
      if (asked !== filesAsked) {
        return;
      }
      batch(() => {
        if (answer) {
          setFiles(answer.files);
          setIndexOff(answer.indexOff);
        }
        setFilesPending(false);
      });
      if (answering() || covered()) {
        rowsOutdated = true;
      } else {
        build(false);
        select(selected());
      }
    };
    props
      .call<FileAnswer | null>("files", { query: wanted, asked })
      .then(settle, () => settle(null));
  };

  const refresh = () => {
    handPicked = false;
    forgetNote();
    lookForFiles(typedText());
    build(true);
  };

  const select = (row: number) => {
    const last = results().length - 1;
    if (last < 0) {
      return;
    }
    const next = Math.min(Math.max(row, 0), last);
    setSelected(next);
    const shown = sections();
    const slot = slotOf(shown, next);
    const heading = startsSection(shown, next) ? slot - 1 : slot;
    const headingAbove = virtualizer.getOffsetForIndex(heading)?.[1] === "start";
    virtualizer.scrollToIndex(headingAbove ? heading : slot);
  };

  const selectedKey = createMemo(() => keyAt(selected()));
  createEffect(on(selectedKey, forgetNote, { defer: true }));

  const moveInList = (step: 1 | -1) => {
    const count = results().length;
    if (count > 0) {
      handPicked = true;
      select(selected() + step);
    }
  };

  let pointer: string | undefined;

  const hover = (event: MouseEvent, row: number) => {
    const at = `${event.screenX},${event.screenY}`;
    const moved = pointer !== undefined && pointer !== at;
    pointer = at;
    if (moved && menu() === null && pressed() === null) {
      handPicked = true;
      setSelected(row);
    }
  };

  const appAt = (row: number) => {
    const index = results()[row];
    return index === undefined ? undefined : apps()[index];
  };

  const extraAt = (row: number) => {
    const index = results()[row];
    return index === undefined || index < apps().length
      ? undefined
      : extras()[index - apps().length];
  };

  let pressTimer: ReturnType<typeof setTimeout> | undefined;

  const cancelPress = () => {
    clearTimeout(pressTimer);
    pressTimer = undefined;
    setPressed(null);
  };

  let startOverOnHide = false;

  const leaveAnswer = () => {
    asking.leave();
    batch(() => {
      setAnswering(false);
      setDraft("");
      setMenu(null);
    });
  };

  const startOver = () => {
    startOverOnHide = false;
    leaveAnswer();
    if (field) {
      field.value = "";
      refresh();
    }
  };

  const press = (target: Press, action: Action) => {
    if (pressed()) {
      return;
    }
    setPressed(target);
    if (action.hides) {
      startOverOnHide = true;
      action.run();
    }
    pressTimer = setTimeout(() => {
      pressTimer = undefined;
      setPressed(null);
      if (target.where === "action") {
        closeMenu();
      }
      if (!action.hides) {
        action.run();
      }
    }, PRESS_MS);
  };

  const startAsking = (question: string) => {
    batch(() => {
      setMenu(null);
      setAnswering(true);
      setDraft("");
    });
    if (field) {
      field.value = "";
    }
    void (question ? asking.start(question) : asking.begin());
  };

  const showAnswer = (conversation: Conversation) => {
    batch(() => {
      setMenu(null);
      setAnswering(true);
      setDraft("");
    });
    if (field) {
      field.value = "";
    }
    void asking.show(conversation);
  };

  const leavePulls = () => {
    const back = pullsReturnTo;
    pullsReturnTo = "";
    pulls.hide();
    return back;
  };

  const openSettings = (picked?: { query: string; row: number }) => {
    setMenu(null);
    if (prefs.open()) {
      field?.focus();
      return;
    }
    if (clip.open()) {
      clip.hide();
    } else if (slk.open()) {
      slk.hide();
    } else if (pullsShown()) {
      returnTo = leavePulls();
    } else {
      returnTo = field?.value ?? "";
    }
    if (field) {
      field.value = "";
    }
    void prefs.show(picked);
    field?.focus();
  };

  const openClipboard = (picked?: { query: string; row: number }) => {
    setMenu(null);
    if (clip.open()) {
      field?.focus();
      return;
    }
    if (prefs.open()) {
      prefs.hide();
    } else if (slk.open()) {
      slk.hide();
    } else if (pullsShown()) {
      returnTo = leavePulls();
    } else {
      returnTo = field?.value ?? "";
    }
    if (field) {
      field.value = "";
    }
    void clip.show(picked);
    field?.focus();
  };

  const openSlack = () => {
    setMenu(null);
    if (slk.open()) {
      field?.focus();
      return;
    }
    if (clip.open()) {
      clip.hide();
    } else if (prefs.open()) {
      prefs.hide();
    } else if (pullsShown()) {
      returnTo = leavePulls();
    } else {
      returnTo = field?.value ?? "";
    }
    if (field) {
      field.value = "";
    }
    void slk.show();
    field?.focus();
  };

  const closeSlack = () => {
    slk.hide();
    setMenu(null);
    if (field) {
      field.value = returnTo;
    }
    returnTo = "";
    field?.focus();
    if (answering()) {
      setDraft(query());
    } else {
      catchUpRows();
    }
  };

  const openPulls = async (start?: PullsOpening, picked?: Picked) => {
    setMenu(null);
    if (pullsShown()) {
      field?.focus();
      return;
    }
    if (clip.open()) {
      clip.hide();
    } else if (slk.open()) {
      slk.hide();
    } else if (prefs.open()) {
      prefs.hide();
    } else {
      pullsReturnTo = field?.value ?? "";
    }
    if (field) {
      field.value = start?.query ?? "";
    }
    field?.focus();
    await pulls.show(start, picked);
    if (start?.menu && pullsShown()) {
      openMenu();
    }
  };

  const closePulls = () => {
    const back = leavePulls();
    setMenu(null);
    if (answering()) {
      return;
    }
    if (field) {
      field.value = back;
    }
    field?.focus();
    catchUpRows();
  };

  const openOrganizations = () => {
    batch(() => {
      setMenuQuery("");
      setMenuSelected(0);
      setMenu({
        title: "Show only an organization",
        noun: "organizations",
        actions: pulls.organizationActions(),
        tall: true,
        primary: "Show only",
      });
    });
  };

  const openRepositories = () => {
    batch(() => {
      setMenuQuery("");
      setMenuSelected(0);
      setMenu({
        title: "Show only a repository",
        noun: "repositories",
        actions: pulls.repositoryActions(),
        tall: true,
        primary: "Show only",
      });
    });
  };

  const askAboutPulls = () => {
    if (!pulls.loaded() || pulls.gate() !== null || !pulls.canAsk()) {
      return;
    }
    pulls.ask().then(
      (context) => {
        if (!pullsShown() || !context) {
          return;
        }
        batch(() => {
          setMenu(null);
          setAnswering(true);
          setDraft("");
        });
        if (field) {
          field.value = "";
        }
        field?.focus();
        void asking.beginWith([
          { kind: "pulls", label: context.label, text: context.text, icon: null },
        ]);
      },
      (error: unknown) => pulls.setNotice(problemOf(error)),
    );
  };

  const catchUpRows = () => {
    forgetNote();
    if (rowsOutdated) {
      build(false);
    }
    nextFrame(() => select(selected()));
  };

  const closeClipboard = () => {
    clip.hide();
    setMenu(null);
    if (field) {
      field.value = returnTo;
    }
    returnTo = "";
    field?.focus();
    if (answering()) {
      setDraft(query());
    } else {
      catchUpRows();
    }
  };

  const closeSettings = () => {
    prefs.hide();
    setMenu(null);
    if (field) {
      field.value = returnTo;
    }
    returnTo = "";
    field?.focus();
    if (!answering()) {
      catchUpRows();
      return;
    }
    setDraft(query());
    const unasked = asking.phase() === "waiting" || asking.phase() === "signin";
    if (unasked && prefs.chosen() !== asking.account().id) {
      void asking.begin();
    }
  };

  const actionsFor = (app: InstalledApp, row: number): Action[] => {
    const typed = query();
    if (isSetting(app)) {
      return [
        {
          title: "Open",
          keys: "↵",
          hides: true,
          run: () =>
            void props.call("open", { path: app.path, query: typed, row }).then(reload, reload),
        },
      ];
    }
    if (app.path === SETTINGS_VIEW) {
      return [
        {
          title: `Open ${app.name}`,
          keys: "↵",
          hides: false,
          run: () => openSettings({ query: typed, row }),
        },
      ];
    }
    if (app.path === CLIPBOARD_VIEW) {
      return [
        {
          title: `Open ${app.name}`,
          keys: "↵",
          hides: false,
          run: () => openClipboard({ query: typed, row }),
        },
      ];
    }
    if (app.path === PULLS_VIEW) {
      return [
        {
          title: `Open ${app.name}`,
          keys: "↵",
          hides: false,
          run: () => void openPulls(undefined, { query: typed, row }),
        },
      ];
    }
    if (app.path === SLACK_VIEW) {
      return [
        {
          title: `Open ${app.name}`,
          keys: "↵",
          hides: false,
          run: () => openSlack(),
        },
      ];
    }
    if (app.path === SETTINGS_FILE) {
      return [
        {
          title: app.name,
          keys: "↵",
          hides: true,
          run: () =>
            void props.call("open_settings_file", { query: typed, row }).then(reload, reload),
        },
      ];
    }
    if (isCommand(app)) {
      return [
        {
          title: app.name,
          keys: "↵",
          hides: true,
          run: () => void props.call("fix_grammar", { query: typed, row }).then(reload, reload),
        },
      ];
    }
    return appActions(app, {
      open: () => props.call("open", { path: app.path, query: typed, row }).then(reload, reload),
      showInFinder: (path) => void props.call("show_in_finder", { path }),
      copyPath: (path) => void props.call("copy_path", { path }),
    });
  };

  const lookAgain = () => {
    startOverOnHide = false;
    askedFiles = "";
    lookForFiles(typedText());
  };

  const openInTerminal = (path: string) => {
    const asked = ++noteFor;
    setSearchNote("");
    props.call("open_in_terminal", { path }).catch((error: unknown) => {
      if (asked === noteFor) {
        setSearchNote(problemOf(error));
      }
    });
  };

  const fileActions = (file: FileRow): Action[] => [
    ...appActions(file, {
      open: () => void props.call("open_file", { path: file.path }).catch(lookAgain),
      showInFinder: (path) => void props.call("show_in_finder", { path }).catch(lookAgain),
      copyPath: (path) => void props.call("copy_path", { path }),
    }),
    {
      title: "Open in Terminal",
      keys: OPEN_IN_TERMINAL_KEYS,
      hides: true,
      run: () => openInTerminal(file.path),
    },
  ];

  const extraActions = (extra: Extra): Action[] =>
    extra.kind === "file"
      ? fileActions(extra.file)
      : extra.kind === "answer"
        ? [
            {
              title: "Open answer",
              keys: "↵",
              hides: false,
              run: () => showAnswer(extra.conversation),
            },
          ]
        : [{ title: "Ask AI", keys: "↵", hides: false, run: () => startAsking(extra.question) }];

  const extraTitle = (extra: Extra) =>
    extra.kind === "file"
      ? extra.file.name
      : extra.kind === "answer"
        ? titleOf(extra.conversation)
        : "Ask AI";

  const fileAt = (row: number) => {
    const extra = extraAt(row);
    return extra?.kind === "file" ? extra.file : undefined;
  };

  const rowActions = (row: number): Action[] | undefined => {
    const app = appAt(row);
    if (app) {
      return actionsFor(app, row);
    }
    const extra = extraAt(row);
    return extra ? extraActions(extra) : undefined;
  };

  const answerText = () => plainOf(asking.text());

  const openLink = (url: string) => {
    props.call("open_link", { url }).catch((error: unknown) => asking.setNotice(problemOf(error)));
  };

  let pasting = false;

  const pasteBack = () => {
    if (pasting) {
      return;
    }
    pasting = true;
    asking.setNotice("");
    props.call("paste_back", { text: answerText() }).then(
      () => {
        pasting = false;
        startOver();
      },
      (error: unknown) => {
        pasting = false;
        asking.setNotice(problemOf(error));
      },
    );
  };

  const copyAnswer = () => {
    props.call("copy_text", { text: answerText() }).then(
      () => asking.setNotice("Copied the answer."),
      (error: unknown) => asking.setNotice(problemOf(error)),
    );
  };

  const askFollowUp = () => {
    const question = draft().trim();
    if (field) {
      field.value = "";
    }
    setDraft("");
    void (asking.phase() === "waiting" ? asking.askFirst(question) : asking.followUp(question));
  };

  const openHistory = () => {
    const now = Date.now() / 1000;
    const shown = asking.conversationId();
    const past = saved().filter((conversation) => conversation.id !== shown);
    batch(() => {
      setMenuQuery("");
      setMenuSelected(0);
      setMenu({
        title: "Conversation history",
        noun: "conversations",
        actions: past.map((conversation) => ({
          title: titleOf(conversation),
          sub: ageLabel(conversation.at, now),
          about: conversation.turns.map((turn) => turn.question).join(" "),
          keys: "",
          hides: false,
          run: () => showAnswer(conversation),
        })),
      });
    });
  };

  const conversationActions = (): Action[] => [
    { title: "New conversation", keys: "⌘ N", hides: false, run: () => startAsking("") },
    { title: "Conversation history", keys: "⌘ Y", hides: false, run: openHistory },
    {
      title: `Use ${asking.other().name}`,
      keys: "",
      hides: false,
      run: () => void asking.choose(asking.other().id),
    },
  ];

  const answerActions = (): Action[] => {
    if (draft().trim()) {
      return [{ title: "Ask", keys: "↵", hides: false, run: askFollowUp }];
    }
    switch (asking.phase()) {
      case "streaming":
        return [{ title: "Stop", keys: "⌘ .", hides: false, run: asking.stop }];
      case "signin":
        return [{ title: "Sign in", keys: "↵", hides: false, run: () => void asking.signIn() }];
      case "error":
        return [
          {
            title: "Try again",
            keys: "↵",
            hides: false,
            run: () => void asking.ask(asking.question()),
          },
        ];
      case "done":
      case "stopped": {
        if (!asking.text().trim()) {
          return [];
        }
        const copy: Action = {
          title: "Copy answer",
          keys: "⌘ ⇧ C",
          hides: false,
          run: copyAnswer,
        };
        const app = asking.front()?.app;
        return app
          ? [{ title: `Paste into ${app}`, keys: "↵", hides: false, run: pasteBack }, copy]
          : [copy];
      }
      default:
        return [];
    }
  };

  const openMenu = () => {
    if (pullsShown()) {
      const actions = pulls.actions({
        organizations: openOrganizations,
        repositories: openRepositories,
        ask: askAboutPulls,
      });
      const pull = pulls.selectedPull();
      setMenuQuery("");
      setMenuSelected(0);
      setMenu({
        title: pull ? `#${pull.number} ${pull.title}` : PULLS_TITLE,
        actions: [...actions, size.action()],
        tall: true,
      });
      return;
    }
    if (clip.open()) {
      setMenuQuery("");
      setMenuSelected(0);
      setMenu({
        title: clip.selectedClip()?.title ?? CLIPBOARD_TITLE,
        actions: [...clip.actions(), size.action()],
      });
      return;
    }
    if (slk.open()) {
      setMenuQuery("");
      setMenuSelected(0);
      setMenu({
        title: slk.selectedHit()?.title ?? SLACK_TITLE,
        actions: [...slk.actions(), size.action()],
      });
      return;
    }
    if (prefs.open()) {
      setMenuQuery("");
      setMenuSelected(0);
      setMenu({ title: "Settings", actions: [...prefs.menuActions(), size.action()] });
      return;
    }
    if (answering()) {
      setMenuQuery("");
      setMenuSelected(0);
      setMenu({
        title: "Ask AI",
        actions: [...answerActions(), ...conversationActions(), size.action()],
      });
      return;
    }
    const row = selected();
    handPicked = true;
    const actions = search ? rowActions(row) : undefined;
    const app = appAt(row);
    const extra = extraAt(row);
    const title = app ? app.name : extra ? extraTitle(extra) : "";
    if (actions) {
      setMenuQuery("");
      setMenuSelected(0);
      setMenu({ title, actions: [...actions, size.action()] });
    }
  };

  const clear = () => {
    if (menu()) {
      if (menuField) {
        menuField.value = "";
      }
      setMenuQuery("");
      setMenuSelected(0);
    } else if (field) {
      field.value = "";
      if (clip.open()) {
        clip.filter("");
      } else if (slk.open()) {
        slk.filter("");
      } else if (
        pullsShown() &&
        !pulls.lost() &&
        pulls.query().trim() !== "" &&
        pulls.shown().length === 0 &&
        pulls.outside() > 0
      ) {
        field.value = pulls.query();
        pulls.narrow(null);
      } else if (pullsShown()) {
        pulls.filter("");
      } else if (prefs.open()) {
        prefs.filter("");
      } else if (answering()) {
        setDraft("");
      } else {
        refresh();
      }
    }
  };

  const closeMenu = () => {
    setMenu(null);
    field?.focus();
  };

  const runAction = () => {
    const index = menuSelected();
    const action = shownActions()[index];
    if (action && !action.off) {
      press({ where: "action", index }, action);
    }
  };

  const runShortcutInMenu = (actions: Action[], keys: string) => {
    const action = actions.find((candidate) => candidate.keys === keys);
    if (action) {
      press({ where: "action", index: shownActions().indexOf(action) }, action);
    }
    return action !== undefined;
  };

  const runOnRow = (pick: (actions: Action[]) => Action | undefined) => {
    const row = selected();
    const actions = rowActions(row);
    const action = actions && pick(actions);
    if (action) {
      press({ where: "row", index: row }, action);
    }
    return action !== undefined;
  };

  const runOnAnswer = (pick: (actions: Action[]) => Action | undefined) => {
    const action = pick(answerActions());
    if (action?.hides) {
      startOverOnHide = true;
    }
    action?.run();
    return action !== undefined;
  };

  const actOnPulls = () => {
    if (pulls.primary()?.hides) {
      startOverOnHide = true;
    }
    pulls.act();
  };

  const pickPull = (at: number) => {
    setMenu(null);
    field?.focus();
    pulls.select(at);
    actOnPulls();
  };

  const open = () => {
    if (menu()) {
      runAction();
    } else if (pullsShown()) {
      actOnPulls();
    } else if (clip.open()) {
      if (clip.selectedClip()) {
        startOverOnHide = true;
      }
      clip.act();
    } else if (slk.open()) {
      const opening = slk.gate() === null && slk.selectedHit();
      slk.act();
      if (opening) {
        void props.call("hide");
      }
    } else if (prefs.open()) {
      prefs.act();
    } else if (answering()) {
      runOnAnswer((actions) => actions[0]);
    } else if (!search) {
      openWhenReady = true;
    } else {
      runOnRow((actions) => actions[0]);
    }
  };

  const moveInMenu = (step: 1 | -1) => {
    const count = shownActions().length;
    if (count > 0) {
      setMenuSelected(clamp(menuSelected() + step, count));
    }
  };

  const scrollAnswer = (share: number) => {
    if (answerView) {
      answerView.scrollTop += share * answerView.clientHeight;
    }
  };
  const gained = (rowHeight: number) =>
    rowsGained(document.getElementById("app")?.offsetHeight ?? 0, COMPACT_HEIGHT, rowHeight);

  const actions = {
    hide: () => void props.call("hide"),
    page: (direction: 1 | -1, half?: boolean) => {
      if (clip.open()) {
        clip.page(direction * (CLIPS_PER_PAGE + gained(CLIP_HEIGHT)));
        return;
      }
      if (slk.open()) {
        slk.page(direction * (CLIPS_PER_PAGE + gained(CLIP_HEIGHT)));
        return;
      }
      if (pullsShown()) {
        pulls.page(direction * (PULLS_PER_PAGE + gained(PULL_HEIGHT)));
        return;
      }
      if (prefs.open()) {
        return;
      }
      if (answering()) {
        scrollAnswer(direction * (half ? HALF_PAGE : ANSWER_PAGE));
        return;
      }
      const rows = Math.max(1, Math.floor((list?.clientHeight ?? 0) / ROW_HEIGHT));
      handPicked = true;
      select(selected() + direction * rows);
    },
    move: (step: 1 | -1) => {
      if (menu()) {
        moveInMenu(step);
      } else if (clip.open()) {
        clip.move(step);
      } else if (slk.open()) {
        slk.move(step);
      } else if (pullsShown()) {
        pulls.move(step);
      } else if (prefs.open()) {
        prefs.move(step);
      } else if (!answering()) {
        moveInList(step);
      }
    },
    open,
    cancel: cancelPress,
    clear,
    toggleMenu: () => (menu() ? closeMenu() : openMenu()),
    closeMenu,
    shortcut: (keys: string) => {
      if (keys === "⌘ ,") {
        openSettings();
        return true;
      }
      if (keys === SIZE_KEYS && !menu()) {
        size.toggle();
        return true;
      }
      if (prefs.open() && menu()?.chooser !== true && prefs.shortcut(keys)) {
        closeMenu();
        return true;
      }
      const current = menu();
      const editing = current ? menuQuery() : query();
      if ((clip.open() || pullsShown() || slk.open()) && keys === DELETE_KEYS && editing !== "") {
        return false;
      }
      if (current) {
        return runShortcutInMenu(current.actions, keys);
      }
      if (pullsShown()) {
        const ready = pulls.loaded() && pulls.gate() === null;
        if (ready && keys === ORGS_KEYS) {
          openOrganizations();
          return true;
        }
        if (ready && keys === REPOS_KEYS) {
          openRepositories();
          return true;
        }
        return pulls.shortcut(keys);
      }
      if (clip.open()) {
        const selecting = field && field.selectionStart !== field.selectionEnd;
        return !selecting && clip.shortcut(keys);
      }
      if (slk.open()) {
        const selecting = field && field.selectionStart !== field.selectionEnd;
        return !selecting && slk.shortcut(keys);
      }
      if (prefs.open()) {
        return false;
      }
      const matching = (actions: Action[]) => actions.find((action) => action.keys === keys);
      return answering()
        ? runOnAnswer((actions) => matching([...actions, ...conversationActions()]))
        : runOnRow(matching);
    },
    ask: () => startAsking(query().trim()),
    stop: () => asking.stop(),
    back: () => {
      if (clip.open()) {
        closeClipboard();
        return;
      }
      if (slk.open()) {
        closeSlack();
        return;
      }
      if (prefs.open()) {
        closeSettings();
        return;
      }
      if (pullsShown()) {
        if (pulls.only()) {
          pulls.narrow(null);
        } else {
          closePulls();
        }
        return;
      }
      leaveAnswer();
      if (pulls.open()) {
        if (field) {
          field.value = pulls.query();
        }
        field?.focus();
        return;
      }
      if (field) {
        field.value = "";
        refresh();
      }
    },
    removeTag: () => {
      if (!asking.removeTag() && asking.phase() === "waiting") {
        actions.back();
      }
    },
    normal: () => {
      batch(() => {
        setWaitingG(false);
        setMode("normal");
      });
    },
    insert: (select: boolean, later?: boolean) => {
      if (later) {
        setTimeout(() => document.hasFocus() && typeAgain(select), 0);
      } else {
        typeAgain(select);
      }
    },
    wait: (waiting: boolean) => setWaitingG(waiting),
    line: (step: 1 | -1) => {
      if (chatting()) {
        scrollAnswer((step * ANSWER_LINES * ANSWER_LINE) / (answerView?.clientHeight || 1));
      } else {
        actions.move(step);
      }
    },
    edge: (last: boolean) => {
      if (clip.open()) {
        clip.page(last ? clip.shown().length : -clip.shown().length);
      } else if (slk.open()) {
        slk.page(last ? slk.shown().length : -slk.shown().length);
      } else if (pullsShown()) {
        pulls.page(last ? pulls.shown().length : -pulls.shown().length);
      } else if (prefs.open()) {
        prefs.select(last ? Math.max(0, prefs.rows().length - 1) : 0);
      } else if (answering()) {
        if (answerView) {
          following = last;
          answerView.scrollTop = last ? answerView.scrollHeight : 0;
        }
      } else if (results().length > 0) {
        handPicked = true;
        select(last ? results().length - 1 : 0);
      }
    },
    tab: (step: 1 | -1) => {
      if (!pulls.loaded() || pulls.gate() !== null) {
        return;
      }
      const next = tabAfter(pulls.tab(), step);
      if (next !== pulls.tab()) {
        pulls.showTab(next);
      }
    },
  };

  const loadAnswers = () =>
    props
      .call<Conversation[] | null>("conversations")
      .then((found) => {
        if (Array.isArray(found)) {
          setSaved(found);
        }
      })
      .catch(() => undefined);

  const reload = () => {
    if (search) {
      void Promise.all([props.call<Listing>("apps"), loadAnswers()]).then(
        ([listing]) => search && start(listing, search),
      );
    }
  };

  const start = (listing: Listing, loaded: Search) => {
    setApps(listing.apps);
    loaded.load(listing.catalog);
    search = loaded;
    refresh();
    if (openWhenReady) {
      openWhenReady = false;
      open();
    }
  };

  const enterInsert = () => {
    batch(() => {
      setWaitingG(false);
      setMode("insert");
    });
    if (field) {
      field.readOnly = false;
    }
  };

  const typeAgain = (select: boolean) => {
    enterInsert();
    if (!field) {
      return;
    }
    field.focus();
    if (select) {
      field.select();
    } else {
      field.setSelectionRange(field.value.length, field.value.length);
    }
  };

  const movable = () => {
    if (clip.open()) {
      return clip.shown().length > 0;
    }
    if (slk.open()) {
      return slk.gate() === null && slk.shown().length > 0;
    }
    if (prefs.open()) {
      return prefs.rows().length > 0;
    }
    if (pullsShown()) {
      return pulls.loaded() && pulls.gate() === null && pulls.shown().length > 0;
    }
    if (answering()) {
      const phase = asking.phase();
      return phase !== null && phase !== "waiting" && phase !== "signin" && phase !== "signing";
    }
    return results().length > 0 && nothingFound() === "";
  };

  const screen = () =>
    clip.open()
      ? "clipboard"
      : slk.open()
        ? "slack"
        : prefs.open()
          ? "settings"
          : pullsShown()
            ? "pulls"
            : answering()
              ? "answer"
              : "search";

  createEffect(on([screen, vimOn], () => enterInsert(), { defer: true }));
  createEffect(
    on(
      normal,
      (now) => {
        setModeSaid(vimOn() ? (now ? NORMAL_SAID : INSERT_SAID) : "");
      },
      { defer: true },
    ),
  );

  let keyDownAt = 0;
  let earlyUntil = 0;
  const earlyMs = props.earlyMs ?? EARLY_MS;
  let held = "";
  const startEarly = () => {
    earlyUntil = performance.now() + earlyMs;
  };
  const view = () => {
    const current = menu();
    return {
      menu: current !== null,
      pressing: pressed() !== null,
      search: current ? menuQuery() : query(),
      held,
      answer: chatting(),
      streaming: chatting() && asking.phase() === "streaming",
      settings: prefs.open(),
      clipboard: clip.open(),
      pulls: pullsShown(),
      slack: slk.open(),
      vim: vimOn(),
      normal: normal(),
      waiting: waitingG(),
      movable: movable(),
    };
  };
  const onKeyDown = (event: KeyboardEvent) => {
    keyDownAt = event.timeStamp;
    if (event.repeat && event.key === "Enter" && covered()) {
      event.preventDefault();
      return;
    }
    if (prefs.recording()) {
      event.preventDefault();
      if (!event.repeat && !event.isComposing) {
        prefs.press(event as Pressed);
      }
      return;
    }
    const now = { ...view(), early: event.timeStamp < earlyUntil };
    if (!event.repeat) {
      held = stepsBack(event, now) || switchesToInsert(event, now) ? event.key : "";
    }
    if (handleKey(event, now, actions)) {
      event.preventDefault();
      if (now.normal && !now.menu) {
        void props.call("keystroke", { ms: performance.now() - event.timeStamp });
      }
    }
  };
  const fieldChanged = () => {
    frames.activity(performance.now());
    if (clip.open()) {
      clip.filter(query());
    } else if (slk.open()) {
      slk.filter(query());
    } else if (pullsShown()) {
      pulls.filter(query());
    } else if (prefs.open()) {
      prefs.filter(query());
    } else if (answering()) {
      setDraft(query());
    } else {
      refresh();
    }
  };
  const typedAhead = (typed: Typed) => {
    if (typed.text && normal()) {
      enterInsert();
    }
    if (field) {
      applyTyped(
        typed,
        field,
        (key) => handleKey({ key }, { ...view(), early: true }, actions),
        fieldChanged,
      );
    }
  };
  const readVim = () =>
    void props
      .call<boolean | null>("vim_keys")
      .then((on) => setVimOn(on === true))
      .catch(() => undefined);
  const takeTypedAhead = () =>
    void catchUp(() => props.call<Typed[] | null>("typed_ahead"), typedAhead);
  const onFocus = () => {
    if (!menu()) {
      enterInsert();
    }
    const active = menu() ? menuField : field;
    if (document.activeElement !== active) {
      active?.focus();
    }
    if (asking.account().signedIn === false) {
      void asking.refreshAccounts();
    }
    void asking.refreshChoice();
    startEarly();
    takeTypedAhead();
    readVim();
    afterNextPaint(
      () => {
        void props.call("shown");
        void size.load();
      },
      nextFrame,
      nextTask,
    );
  };

  const onBlur = () => {
    pointer = undefined;
    askedFiles = "";
    if (clip.open()) {
      closeClipboard();
    }
    if (slk.open()) {
      closeSlack();
    }
    if (prefs.open()) {
      closeSettings();
    }
    if (pulls.open()) {
      closePulls();
    }
    if (!pasting && (startOverOnHide || answering())) {
      startOver();
    }
    cancelPress();
    setMenu(null);
    enterInsert();
    frames.reset();
    field?.select();
    openWhenReady = false;
    reload();
  };

  let stopListening: (() => void) | undefined;
  let stopHearing: (() => void) | undefined;
  let stopOpening: (() => void) | undefined;
  onMount(() => {
    window.addEventListener("blur", onBlur);
    document.addEventListener("keydown", onKeyDown);
    window.addEventListener("focus", onFocus);
    field?.focus();
    void props
      .listen((event) => asking.receive(event))
      .then((stop) => {
        stopListening = stop;
      });
    void props
      .hear((keys) => prefs.heard(keys))
      .then((stop) => {
        stopHearing = stop;
      });
    void props
      .opens?.((shown) => {
        if (shown && clip.open() && !menu()) {
          actions.hide();
        } else {
          openClipboard();
          if (shown) {
            startEarly();
            takeTypedAhead();
          }
        }
      })
      .then((stop) => {
        stopOpening = stop;
      });
    void asking.refreshAccounts();
    void asking.refreshChoice();
    readVim();
    void size.load();
    if (props.pulls) {
      void openPulls(props.pulls);
    }
    void Promise.all([props.call<Listing>("apps"), props.search(), loadAnswers()]).then(
      ([listing, loaded]) => start(listing, loaded),
    );
  });
  onCleanup(() => {
    window.removeEventListener("blur", onBlur);
    document.removeEventListener("keydown", onKeyDown);
    window.removeEventListener("focus", onFocus);
    stopListening?.();
    stopHearing?.();
    stopOpening?.();
  });

  let following = true;
  const [faded, setFaded] = createSignal(false);

  createEffect(() => {
    answering();
    asking.turns();
    asking.question();
    following = true;
  });

  createEffect(() => {
    answering();
    asking.turns();
    asking.question();
    asking.text();
    asking.phase();
    covered();
    settle();
  });

  function settle() {
    if (!answerView) {
      return;
    }
    if (following && !covered()) {
      answerView.scrollTop = answerView.scrollHeight;
    }
    setFaded(answerView.scrollTop > 0);
  }

  const watchAnswerView = (element: HTMLDivElement) => {
    answerView = element;
    if (typeof ResizeObserver === "function") {
      const watch = new ResizeObserver(() => settle());
      watch.observe(element);
      onCleanup(() => watch.disconnect());
    }
  };

  const followScroll = () => {
    if (answerView) {
      const below = answerView.scrollHeight - answerView.scrollTop - answerView.clientHeight;
      following = below < FOLLOW_SLACK;
      setFaded(answerView.scrollTop > 0);
    }
  };

  const troubled = () => {
    const phase = asking.phase();
    return phase === "signin" || phase === "signing" || phase === "error";
  };

  const onInput = (event: InputEvent) => {
    fieldChanged();
    const started = event.timeStamp - keyDownAt < 1000 ? keyDownAt : event.timeStamp;
    void props.call("keystroke", { ms: performance.now() - started });
  };

  const primary = () => {
    const current = menu();
    if (current?.primary) {
      const title = current.primary;
      return { title, keys: "↵", hides: false, run: runAction };
    }
    if (pullsShown()) {
      return pulls.primary();
    }
    if (clip.open()) {
      return clip.primary();
    }
    if (slk.open()) {
      return slk.primary();
    }
    if (prefs.open()) {
      return prefs.primary();
    }
    if (answering()) {
      return answerActions()[0];
    }
    return rowActions(selected())?.[0];
  };
  const hiddenTag = () => hiddenLabel(asking.hidden());
  const account = () => asking.account();

  const searchField = (
    <input
      ref={(element) => (field = element)}
      id="q"
      type="text"
      aria-describedby={!answering() && !covered() ? "hint" : undefined}
      placeholder={
        clip.open()
          ? CLIPBOARD_PLACEHOLDER
          : slk.open()
            ? slk.gate()
              ? TOKEN_PLACEHOLDER
              : SLACK_PLACEHOLDER
            : pullsShown()
              ? PULLS_PLACEHOLDER
              : prefs.open()
                ? SETTINGS_PLACEHOLDER
                : answering()
                  ? FOLLOW_UP_PLACEHOLDER
                  : SEARCH_PLACEHOLDER
      }
      autocomplete="off"
      spellcheck={false}
      readOnly={normal()}
      classList={{ dim: normal() }}
      onMouseDown={() => normal() && enterInsert()}
      aria-label={
        clip.open()
          ? CLIPBOARD_PLACEHOLDER
          : slk.open()
            ? slk.gate()
              ? TOKEN_PLACEHOLDER
              : SLACK_PLACEHOLDER
            : pullsShown()
              ? PULLS_PLACEHOLDER
              : prefs.open()
                ? SETTINGS_PLACEHOLDER
                : answering()
                  ? "Ask a follow-up"
                  : "Search"
      }
      onInput={onInput}
    />
  ) as HTMLInputElement;

  let fieldSlot: HTMLSpanElement | undefined;
  let composer: HTMLDivElement | undefined;
  createEffect(
    on(chatting, (below) => {
      const place = below ? composer : fieldSlot;
      if (!field || !place || field.parentNode === place) {
        return;
      }
      const active = document.activeElement;
      const refocus =
        active === field || ((active === null || active === document.body) && !menu());
      place.appendChild(field);
      if (refocus && document.activeElement !== field) {
        field.focus();
      }
    }),
  );

  return (
    <>
      <header
        classList={{
          chat: chatting(),
          loading:
            (chatting() && asking.phase() === "streaming") ||
            (pullsShown() && !prefs.open() && !clip.open() && pulls.reading()),
        }}
      >
        <Show when={menu() || answering() || covered()}>
          <button
            id="back"
            type="button"
            aria-label="Back"
            title="Back (Esc)"
            onMouseDown={(event) => event.preventDefault()}
            onClick={() => (menu() ? closeMenu() : actions.back())}
          >
            <svg viewBox="0 0 24 24" aria-hidden="true">
              <path d="M15 6l-6 6 6 6" />
            </svg>
          </button>
        </Show>
        <Switch>
          <Match when={clip.open()}>
            <span id="chip">
              <Glyph name="clipboard" />
              {CLIPBOARD_TITLE}
            </span>
          </Match>
          <Match when={slk.open()}>
            <span id="chip">
              <SlackMark />
              {SLACK_TITLE}
            </span>
          </Match>
          <Match when={prefs.open()}>
            <span id="chip">
              <Glyph name="gear" />
              Settings
            </span>
          </Match>
          <Match when={pullsShown()}>
            <span id="chip">
              <GitHubMark />
              {PULLS_TITLE}
            </span>
            <Show when={pulls.onlyLabel()}>
              {(label) => (
                <span class="fchip">
                  {label()}
                  <button
                    class="x"
                    type="button"
                    aria-label="Clear the filter"
                    onMouseDown={(event) => event.preventDefault()}
                    onClick={() => pulls.narrow(null)}
                  >
                    ×
                  </button>
                </span>
              )}
            </Show>
          </Match>
          <Match when={answering()}>
            <span id="chip">
              <Brand id={account().id} />
              Ask AI
            </span>
          </Match>
        </Switch>
        <span class="field-slot" ref={(element) => (fieldSlot = element)}>
          {searchField}
        </span>
        <Show when={!answering() && !covered()}>
          <span id="hint">
            <kbd>Tab</kbd>
            <Brand id={asking.chosenAccount().id} />
            <span class="sr">asks {asking.chosenAccount().name}</span>
          </span>
        </Show>
      </header>
      <Show when={prefs.open()}>
        <SettingsList settings={prefs} icon={icon} />
      </Show>
      <Show when={clip.open()}>
        <ClipboardList clipboard={clip} />
      </Show>
      <Show when={slk.open()}>
        <SlackList slack={slk} />
      </Show>
      <Show when={pullsShown()}>
        <PullsList pulls={pulls} menuOpen={menu() !== null} onPick={pickPull} />
      </Show>
      <Show when={answering()}>
        <div
          ref={watchAnswerView}
          id="ai"
          hidden={covered()}
          classList={{ faded: faded() }}
          data-phase={asking.phase() ?? ""}
          onScroll={followScroll}
          aria-live="polite"
        >
          <For each={asking.turns()}>
            {(turn) => (
              <div class="ai-turn">
                <div class="ai-q">{turn.question}</div>
                <div class="ai-a">
                  <Reply text={turn.text} streaming={false} open={openLink} />
                  <Show when={turn.stopped}>
                    <p class="muted">Stopped.</p>
                  </Show>
                </div>
              </div>
            )}
          </For>
          <Show when={asking.phase() !== "waiting"}>
            <div class="ai-q">{asking.question()}</div>
          </Show>
          <Switch
            fallback={
              <div class="ai-a">
                <Reply
                  open={openLink}
                  text={asking.text()}
                  streaming={asking.phase() === "streaming" && asking.searching() === null}
                />
                <Show when={asking.phase() === "streaming" && asking.searching() !== null}>
                  <p class="searching" role="status">
                    {asking.searching()
                      ? `Searching the web for “${asking.searching()}”`
                      : "Searching the web"}
                  </p>
                </Show>
                <Show when={asking.phase() === "stopped"}>
                  <p class="muted">Stopped.</p>
                </Show>
                <Show when={asking.phase() === "error"}>
                  <p class="muted">{asking.problem()}</p>
                </Show>
                <Show when={asking.notice()}>
                  <p class="muted">{asking.notice()}</p>
                </Show>
              </div>
            }
          >
            <Match when={asking.phase() === "waiting"}>
              <div class="ai-empty">
                Ask anything about what you are working on.
                <span>
                  {asking.tags().length > 0
                    ? "What is shown below goes with your question. Backspace removes it."
                    : "Nothing from your screen goes with your question."}
                </span>
                <Show when={asking.notice()}>
                  <span>{asking.notice()}</span>
                </Show>
              </div>
            </Match>
            <Match when={asking.phase() === "signin" || asking.phase() === "signing"}>
              <div class="ai-empty">
                Sign in to {account().through} to ask
                <span>
                  {asking.phase() === "signing"
                    ? `Finish signing in to ${account().through}, then your question is sent.`
                    : `Raiden answers on your ${account().name} subscription through ${account().through}. Enter opens its sign-in.`}
                </span>
                <Show when={asking.notice()}>
                  <span>{asking.notice()}</span>
                </Show>
              </div>
            </Match>
          </Switch>
        </div>
        <Show when={!covered() && (asking.tags().length > 0 || hiddenTag())}>
          <div class="cx-row">
            <Index each={asking.tags()}>
              {(tag, index) => (
                <span class="cx" title={tag().label}>
                  <Show when={tag().kind === "pulls"}>
                    <GitHubMark />
                  </Show>
                  <Show when={icon(tag().icon ?? "")}>
                    {(url) => <img class="cx-icon" alt="" src={url()} />}
                  </Show>
                  <span class="cx-label">{tag().title ?? tag().label}</span>
                  <Show when={tag().place}>
                    {(place) => <span class="cx-place">{place()}</span>}
                  </Show>
                  <Show when={!asking.tagsFixed()}>
                    <button
                      class="cx-x"
                      type="button"
                      aria-label={`Remove ${tag().label}`}
                      onMouseDown={(event) => event.preventDefault()}
                      onClick={() => {
                        asking.removeTag(index);
                        field?.focus();
                      }}
                    >
                      <svg viewBox="0 0 24 24" aria-hidden="true">
                        <path d="M6 6l12 12M18 6L6 18" />
                      </svg>
                    </button>
                  </Show>
                </span>
              )}
            </Index>
            <Show when={hiddenTag()}>
              {(label) => (
                <span class="cx shield">
                  <svg viewBox="0 0 24 24" aria-hidden="true">
                    <path d="M12 3l7 3v5c0 4.5-3 8-7 10-4-2-7-5.5-7-10V6z" />
                    <path d="M9 12l2 2 4-4" />
                  </svg>
                  {label()}
                </span>
              )}
            </Show>
          </div>
        </Show>
        <Show when={!covered()}>
          <div class="composer" ref={(element) => (composer = element)} />
        </Show>
      </Show>
      <div
        ref={(element) => (list = element)}
        class="list"
        role="listbox"
        aria-label="Apps"
        hidden={answering() || covered()}
        onScroll={() => frames.activity(performance.now())}
      >
        <Show when={nothingFound()}>
          {(asked) => (
            <div class="none">
              {`Nothing matches “${asked()}”`}
              <span>Press Tab to ask AI about it, or Esc to clear the search.</span>
              <Show when={indexOff()}>
                <span>Files are not searched while Spotlight's index is off.</span>
              </Show>
            </div>
          )}
        </Show>
        <div class="rows" style={{ height: `${virtualizer.getTotalSize()}px` }}>
          <Index each={virtualizer.getVirtualItems()}>
            {(item) => {
              const slot = createMemo(() => slotAt(sections(), item().index));
              const row = () => rowOf(slot());
              const index = () => results()[row()];
              const app = () => apps()[index() ?? -1];
              const extra = () => {
                const at = index();
                return at === undefined || at < apps().length
                  ? undefined
                  : extras()[at - apps().length];
              };
              const marked = createMemo(() => {
                const text = typed();
                const at = index();
                const positions =
                  text && search && at !== undefined && at < apps().length
                    ? search.positions(at, text)
                    : NO_POSITIONS;
                return pieces(app()?.name ?? "", positions);
              });
              return (
                <Show
                  when={headingOf(slot())}
                  fallback={
                    <div
                      class="row"
                      classList={{
                        selected: selected() === row(),
                        press: pressed()?.where === "row" && pressed()?.index === row(),
                      }}
                      role="option"
                      aria-selected={selected() === row()}
                      style={{ transform: `translateY(${item().start}px)` }}
                      onMouseMove={(event) => hover(event, row())}
                      onClick={() => {
                        setMenu(null);
                        field?.focus();
                        setSelected(row());
                        open();
                      }}
                    >
                      <Show
                        when={extra()}
                        fallback={
                          <>
                            <Switch
                              fallback={
                                <Show when={icon(iconPath(app()))} fallback={<span class="icon" />}>
                                  {(url) => <img class="icon" alt="" src={url()} />}
                                </Show>
                              }
                            >
                              <Match when={isAiCommand(app())}>
                                <AiIcon />
                              </Match>
                              <Match when={app()?.path === SETTINGS_VIEW}>
                                <Glyph name="gear" />
                              </Match>
                              <Match when={app()?.path === CLIPBOARD_VIEW}>
                                <Glyph name="clipboard" />
                              </Match>
                              <Match when={app()?.path === PULLS_VIEW}>
                                <GitHubMark />
                              </Match>
                              <Match when={app()?.path === SLACK_VIEW}>
                                <SlackMark />
                              </Match>
                              <Match when={app()?.path === SETTINGS_FILE}>
                                <Glyph name="file" />
                              </Match>
                            </Switch>
                            <span class="title">
                              <Index each={marked()}>
                                {(piece) => (
                                  <Show when={piece().matched} fallback={piece().text}>
                                    <b>{piece().text}</b>
                                  </Show>
                                )}
                              </Index>
                            </span>
                            <Show when={app()?.under}>
                              {(under) => <span class="sub">{under()}</span>}
                            </Show>
                            <Switch>
                              <Match when={app()?.accessory}>
                                {(accessory) => <span class="acc">{accessory()}</span>}
                              </Match>
                              <Match when={isAiCommand(app())}>
                                <span class="sub">Selected text</span>
                                <span class="acc">AI</span>
                              </Match>
                              <Match when={app()?.path === SETTINGS_VIEW}>
                                <span class="acc">⌘ ,</span>
                              </Match>
                              <Match when={isCommand(app())}>
                                <span class="acc">Command</span>
                              </Match>
                            </Switch>
                          </>
                        }
                      >
                        {(shown) => (
                          <Show
                            when={(() => {
                              const current = shown();
                              return current.kind === "file" ? current.file : undefined;
                            })()}
                            fallback={
                              <>
                                <AiIcon />
                                <span class="title">{extraTitle(shown())}</span>
                                <span class="sub">
                                  {(() => {
                                    const current = shown();
                                    return current.kind === "ask"
                                      ? `“${current.question}”`
                                      : current.kind === "answer"
                                        ? ageLabel(current.conversation.at, Date.now() / 1000)
                                        : "";
                                  })()}
                                </span>
                                <Show when={shown().kind === "ask"}>
                                  <span class="acc">Built in</span>
                                </Show>
                              </>
                            }
                          >
                            {(file) => (
                              <>
                                <Show
                                  when={icon(file().path, file().icon)}
                                  fallback={<span class="icon" />}
                                >
                                  {(url) => <img class="icon" alt="" src={url()} />}
                                </Show>
                                <span class="title">
                                  <Index
                                    each={pieces(
                                      file().name.normalize("NFC"),
                                      wordPositions(file().name, typed()),
                                    )}
                                  >
                                    {(piece) => (
                                      <Show when={piece().matched} fallback={piece().text}>
                                        <b>{piece().text}</b>
                                      </Show>
                                    )}
                                  </Index>
                                </span>
                                <span class="sub">{file().folder}</span>
                              </>
                            )}
                          </Show>
                        )}
                      </Show>
                    </div>
                  }
                >
                  {(heading) => (
                    <div
                      class="sec"
                      role="presentation"
                      style={{ transform: `translateY(${item().start}px)` }}
                    >
                      {heading()}
                    </div>
                  )}
                </Show>
              );
            }}
          </Index>
        </div>
      </div>
      <footer>
        <span id="ctx">
          <Show when={vimOn()}>
            <span class="nm" classList={{ ins: !normal() }}>
              {normal() ? "NORMAL" : "INSERT"}
            </span>
          </Show>
          <Switch>
            <Match when={clip.open()}>{clip.context()}</Match>
            <Match when={slk.open()}>{slk.context()}</Match>
            <Match when={pullsShown()}>{pulls.context()}</Match>
            <Match when={prefs.open()}>{prefs.context()}</Match>
            <Match when={!answering() && searchNote()}>{searchNote()}</Match>
            <Match when={answering()}>
              <Brand id={account().id} />
              {modelLabel(asking.model(), account().name)}
              <Show
                when={troubled()}
                fallback={<span class="sr">, on your {account().name} subscription</span>}
              >
                <span class="why">on your {account().name} subscription</span>
              </Show>
            </Match>
            <Match when={fileAt(selected())}>Files</Match>
            <Match when={extraAt(selected()) || isAiCommand(appAt(selected()))}>
              <AiIcon />
              AI
            </Match>
            <Match when={isCommand(appAt(selected()))}>Commands</Match>
            <Match when={isSetting(appAt(selected()))}>System Settings</Match>
            <Match when={appAt(selected())}>Applications</Match>
          </Switch>
        </span>
        <div class="right">
          <Show when={prefs.recording()}>
            <span
              class="btn"
              id="cancel"
              onMouseDown={(event) => event.preventDefault()}
              onClick={prefs.cancel}
            >
              Cancel <Keys keys="esc" />
            </span>
          </Show>
          <Show when={!prefs.recording() && primary()}>
            {(action) => (
              <>
                <span
                  class="btn"
                  id="primary"
                  onMouseDown={(event) => event.preventDefault()}
                  onClick={open}
                >
                  {action().title} <Keys keys={action().keys} />
                </span>
                <span class="vsep" />
              </>
            )}
          </Show>
          <Show when={!prefs.recording()}>
            <Show
              when={pullsShown() && pulls.gate() === "allow"}
              fallback={
                <span
                  class="btn"
                  id="openActions"
                  onMouseDown={(event) => event.preventDefault()}
                  onClick={() => (menu() ? closeMenu() : openMenu())}
                >
                  Actions <Keys keys="⌘ K" />
                </span>
              }
            >
              <span
                class="btn"
                id="notNow"
                onMouseDown={(event) => event.preventDefault()}
                onClick={() => actions.back()}
              >
                Not now <Keys keys="esc" />
              </span>
            </Show>
          </Show>
        </div>
      </footer>
      <span class="said" role="status" aria-live="polite">
        {modeSaid()}
      </span>
      <Show when={menu()}>
        {(current) => (
          <div
            id="actions"
            classList={{ tall: current().tall === true }}
            role="menu"
            aria-label="Actions"
            onMouseDown={(event) => {
              if (event.target !== menuField) {
                event.preventDefault();
              }
            }}
          >
            <div class="head">{current().title}</div>
            <Show
              when={shownActions().length > 0}
              fallback={
                <Show
                  when={menuQuery().trim()}
                  fallback={
                    <div class="none">
                      {current().noun ? `No ${current().noun} yet` : "No actions"}
                    </div>
                  }
                >
                  {(typed) => <NoMatch things={current().noun ?? "actions"} typed={typed()} />}
                </Show>
              }
            >
              <div class="acts">
                <Index each={shownActions()}>
                  {(action, index) => (
                    <>
                      <Show
                        when={
                          action().section !== undefined &&
                          shownActions()[index - 1]?.section !== action().section
                        }
                      >
                        <div class="mh" role="presentation">
                          {action().section}
                        </div>
                      </Show>
                      <div
                        ref={(element) =>
                          createEffect(() => {
                            if (menuSelected() === index) {
                              element.scrollIntoView({ block: "nearest" });
                            }
                          })
                        }
                        class="act"
                        classList={{
                          off: action().off === true,
                          sel: menuSelected() === index,
                          press: pressed()?.where === "action" && pressed()?.index === index,
                        }}
                        role="menuitem"
                        aria-disabled={action().off === true ? true : undefined}
                        onClick={() => {
                          setMenuSelected(index);
                          runAction();
                        }}
                      >
                        <Show when={action().app !== undefined || action().image !== undefined}>
                          <MenuIcon
                            src={action().image || (action().app ? icon(action().app ?? "") : "")}
                          />
                        </Show>
                        <span class="act-t">{action().title}</span>
                        <Show when={action().checked}>
                          <Check />
                        </Show>
                        <Show when={action().note}>
                          {(note) => <span class="asub">{note()}</span>}
                        </Show>
                        <Show when={action().sub}>{(sub) => <span class="sub">{sub()}</span>}</Show>
                        <Show when={action().keys}>
                          <Keys keys={action().keys} />
                        </Show>
                      </div>
                    </>
                  )}
                </Index>
              </div>
            </Show>
            <input
              ref={(element) => {
                menuField = element;
                queueMicrotask(() => element.focus());
              }}
              id="aq"
              type="text"
              placeholder={`Search ${current().noun ?? "actions"}`}
              autocomplete="off"
              spellcheck={false}
              aria-label={`Search ${current().noun ?? "actions"}`}
              onInput={(event) => {
                setMenuQuery(event.currentTarget.value);
                setMenuSelected(0);
              }}
            />
          </div>
        )}
      </Show>
    </>
  );
}

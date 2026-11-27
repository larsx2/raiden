import { render } from "solid-js/web";
import tauri from "../../tauri.conf.json";
import recorded from "../scenes/answer.json";
import pullsFixture from "./pulls.fixture.json";
import {
  FIXTURE_ANSWERS,
  FIXTURE_APPS,
  FIXTURE_CLIPS,
  FIXTURE_FILES,
  FIXTURE_HISTORY,
  FIXTURE_TERMINALS,
  QUICK_MESSAGE,
  SPOTLIGHT_REFUSAL,
  type Fixture,
} from "../scenes/scenes";
import {
  App,
  type Call,
  type Hear,
  type InstalledApp,
  type Listen,
  type Listing,
  type Opens,
  type Previews,
  type PullsOpening,
} from "./App";
import { Hud } from "./Hud";
import type { Account, AnswerEvent, Conversation, Front } from "./ask";
import type { Problem, ProblemKind, Pull, PullsView, Tab } from "./pulls";
import type { AtLogin, Effort, SettingsView } from "./settings";
import type { TerminalId, TerminalRow } from "./terminals";
import { fetchSearch } from "./search";
import { EXPANDED } from "./window";

const params = new URLSearchParams(location.search);
const fixture = (params.get("fixture") ?? "empty") as Fixture;
const query = params.get("query") ?? "";
const moment = params.get("moment");

const shown = fixture === "empty" ? [] : FIXTURE_APPS;
const HOME = "/Users/me";
const fixtureFiles =
  fixture === "files"
    ? FIXTURE_FILES.map(([path]) => {
        const name = path.slice(path.lastIndexOf("/") + 1);
        const folder = path.slice(0, path.lastIndexOf("/")).replace(HOME, "~");
        return {
          name,
          path,
          folder,
          icon: name.includes(".") ? name.slice(name.lastIndexOf(".")) : path,
        };
      })
    : [];
const commands: InstalledApp[] =
  params.get("commands") === "1"
    ? [
        { name: "Fix grammar", path: "raiden:fix-grammar" },
        { name: "Clipboard History", path: "raiden:clipboard", under: "Raiden" },
        {
          name: "Pull Requests",
          path: "raiden:pull-requests",
          under: "GitHub",
          accessory: "1 needs your review",
        },
        { name: "Settings", path: "raiden:settings", under: "Raiden" },
        { name: "Open Settings File", path: "raiden:settings-file", under: "raiden.toml" },
      ]
    : [];
const settings: InstalledApp[] =
  params.get("settings") === "1"
    ? [
        {
          name: "Display",
          path: "x-apple.systempreferences:com.apple.Accessibility-Settings.extension?display",
          under: "Accessibility",
        },
        {
          name: "Displays",
          path: "x-apple.systempreferences:com.apple.Displays-Settings.extension",
          under: "System Settings",
        },
      ]
    : [];
const apps: InstalledApp[] = [
  ...shown.map(([name]) => ({ name, path: `/Applications/${name}.app` })),
  ...settings,
  ...commands,
];
const pickedSetting = params.get("picked") === "setting" ? `${shown.length}\t1\t\n`.repeat(9) : "";
const picks =
  shown
    .flatMap(([, , uses], index) => Array.from({ length: uses }, () => `${index}\t1\t\n`))
    .join("") + pickedSetting;
const fixtureTerminals: TerminalRow[] = FIXTURE_TERMINALS.map(([id, name, app]) => ({
  id,
  name,
  app,
}));
const iconByPath = new Map([
  ...shown.map(([name, icon]): [string, string] => [`/Applications/${name}.app`, icon]),
  ...fixtureTerminals.flatMap(({ id, app }): Array<[string, string]> => (app ? [[app, id]] : [])),
  ...(fixture === "files" ? FIXTURE_FILES : []),
]);

const now = Date.now() / 1000;
const fixtureConversation = (
  turns: string[],
  hours: number,
  at: number,
  name: string,
): Conversation => ({
  id: `${name}-${at}`,
  at: now - hours * 3600,
  model: params.get("historymodel") ?? recorded.events.model,
  turns: turns.map((question) => ({ question, answer: `About ${question}` })),
});
const answers: Conversation[] = [
  ...(fixture === "recent-answers"
    ? FIXTURE_ANSWERS.map(([question, hours], at) =>
        fixtureConversation([question], hours, at, "fixture"),
      )
    : []),
  ...(params.get("history") === "1"
    ? FIXTURE_HISTORY.map(([turns, hours], at) => fixtureConversation(turns, hours, at, "history"))
    : []),
];
const asked = new Set<string>();
const clips =
  fixture === "clipboard" || params.get("clips") === "1"
    ? FIXTURE_CLIPS.map((clip, at) => ({ ...clip, id: `clip-${at}` }))
    : [];
const front: Front | null =
  fixture === "answer" || fixture === "clipboard"
    ? {
        ...recorded.front,
        window: params.get("window") ?? recorded.front.window,
        selection: params.get("selection") ?? recorded.front.selection,
      }
    : null;
let signedIn = params.get("signedout") !== "1";
let chatgptSignedIn = params.get("chatgptsignedout") !== "1";
let chosenAi = params.get("ai") ?? "claude";
let chosenEffort = (params.get("effort") ?? "low") as Effort;
let vimKeys = params.get("vim") !== null && params.get("vim") !== "0";
let terminalInUse = (params.get("terminal") ?? "ghostty") as TerminalId;
const CHATGPT_MODEL = "gpt-6-luna";
const settingsFile = {
  hotkey: params.get("hotkey") ?? "⌥Space",
  clipboardHotkey: params.get("clipboardhotkey") ?? "⌃⌥V",
  atLogin: { on: params.get("login") !== "off", canChange: true, note: null } as AtLogin,
};
window.editSettings = (edit) => {
  if (edit.hotkey) {
    settingsFile.hotkey = edit.hotkey;
  }
  if (edit.ai) {
    chosenAi = edit.ai;
  }
  if (edit.effort) {
    chosenEffort = edit.effort as Effort;
  }
  if (edit.vim !== undefined) {
    vimKeys = edit.vim;
  }
};

window.sent = [];
const waitingFiles: Array<{ query: string; resolve: () => void }> = [];
window.finishFiles = (query) => {
  const released = waitingFiles.filter((waiting) => query === undefined || waiting.query === query);
  for (const waiting of released) {
    waitingFiles.splice(waitingFiles.indexOf(waiting), 1);
    waiting.resolve();
  }
};

const pullsNow = Date.now() / 1000;
let pullsSaved = params.get("fetched") !== "none";
const fetchedAt =
  pullsNow - (pullsSaved ? Number(params.get("fetched") ?? pullsFixture.fetchedSecondsAgo) : 0);
const waitingRefreshes: Array<() => void> = [];
window.finishRefresh = () => {
  for (const resolve of waitingRefreshes.splice(0)) {
    resolve();
  }
};
const PROBLEMS: Record<ProblemKind, Problem> = {
  missing: {
    kind: "missing",
    text: "Raiden could not find gh, GitHub's command-line tool. Install it with brew install gh, then run gh auth login.",
  },
  signedOut: {
    kind: "signedOut",
    text: "gh is not signed in to GitHub. Run gh auth login in Terminal.",
  },
  offline: { kind: "offline", text: "Raiden could not reach GitHub." },
  limited: {
    kind: "limited",
    text: "GitHub's limit on requests is used up for now.",
    retryAt: pullsNow + 300,
  },
  failed: { kind: "failed", text: "gh could not list your pull requests." },
};
let pullsProblem: Problem | null = PROBLEMS[params.get("problem") as ProblemKind] ?? null;
let ghSignedIn = false;
window.signInToGh = () => {
  ghSignedIn = true;
};
let readFails = params.get("readfails") === "1";
const lostRepo = params.get("lost");
let pullsAllowed = params.get("allowed") !== "0";
let pinnedRepos = [...pullsFixture.pinned, ...(lostRepo ? [lostRepo] : [])];
const fixturePulls: Pull[] = pullsFixture.pulls.map(({ hoursAgo, ...pull }) => ({
  ...pull,
  checks: pull.checks as Pull["checks"],
  look: pull.look as Pull["look"],
  updatedAt: pullsNow - hoursAgo * 3600,
}));
const pullsView = (): PullsView => {
  const problem = pullsProblem;
  const signedOut = problem?.kind === "missing" || problem?.kind === "signedOut";
  return {
    allowed: pullsAllowed,
    login: signedOut || params.get("nologin") === "1" ? null : pullsFixture.login,
    pulls: pullsSaved ? fixturePulls : [],
    fetchedAt: pullsSaved ? fetchedAt : null,
    reviewRequested: pullsFixture.reviewRequested,
    owners: pullsFixture.owners,
    pinned: [...pinnedRepos],
    lost: lostRepo ? [lostRepo] : [],
    refreshing: false,
    problem,
  };
};
const PULLS_LABELS: Record<Tab, string> = {
  all: "Open pull requests",
  failing: "Failing pull requests",
  ready: "Pull requests ready to merge",
  drafts: "Draft pull requests",
};
const pullsContext = (tab: Tab, only: string | null, typed: string, urls: string[] | null) => {
  const inside = fixturePulls.filter(
    (pull) =>
      (!urls || urls.includes(pull.url)) &&
      (tab === "all" ||
        (tab === "failing" && pull.failing) ||
        (tab === "ready" && pull.ready) ||
        (tab === "drafts" && pull.draft)) &&
      (!only || `${pull.owner}/${pull.repo}` === only || pull.owner === only),
  );
  const shown = only
    ? (fixturePulls.find((pull) => `${pull.owner}/${pull.repo}` === only)?.shown ?? only)
    : "";
  return {
    label: `${PULLS_LABELS[tab]}${shown ? ` in ${shown}` : ""}${typed ? ` matching “${typed}”` : ""}`,
    text: inside
      .map(
        (pull) =>
          `${pull.owner}/${pull.repo} #${pull.number} ${pull.title} (${pull.draft ? "draft" : "open"}, checks ${pull.checks}${pull.conflicts ? ", conflicts" : ""})`,
      )
      .join("\n"),
  };
};
const pullsOpening: PullsOpening | undefined =
  fixture === "pulls"
    ? {
        tab: (params.get("tab") ?? "all") as Tab,
        only: params.get("only"),
        query,
        menu: params.get("actions") === "1",
      }
    : undefined;

let looking = false;
let expanded = params.get("size") === "expanded";
const fitRoot = () => {
  const box = document.querySelector<HTMLElement>("#app");
  const shown = expanded ? EXPANDED : tauri.app.windows[0];
  if (box && shown && fixture !== "quick") {
    box.style.width = `${shown.width}px`;
    box.style.height = `${shown.height}px`;
  }
};

let listener: ((event: AnswerEvent) => void) | undefined;
const emit = (event: AnswerEvent) => listener?.(event);
window.emitAnswer = emit;

const replay = (id: string, followingUp: boolean, model: string) => {
  const { hidden } = recorded.events;
  const told = followingUp ? recorded.followUp.deltas : recorded.events.deltas;
  const deltas = Array.from({ length: Number(params.get("long") ?? 1) }, () => told).flat();
  const shownDeltas = moment === "streaming" ? deltas.slice(0, recorded.streaming) : deltas;
  for (const text of shownDeltas) {
    emit({ id, kind: "delta", text, model, hidden });
  }
  if (moment !== "streaming" && params.get("hold") !== "1") {
    emit({ id, kind: "done", text: deltas.join(""), model, hidden });
  }
};

const accounts = (): Account[] => [
  { id: "claude", name: "Claude", through: "Claude Code", signedIn },
  { id: "chatgpt", name: "ChatGPT", through: "Codex", signedIn: chatgptSignedIn },
];

const call: Call = async <T,>(command: string, args?: Record<string, unknown>) => {
  window.sent.push(args === undefined ? { command } : { command, args });
  switch (command) {
    case "window_size":
      return expanded as T;
    case "expand_window":
      expanded = args?.["expanded"] === true;
      fitRoot();
      return expanded as T;
    case "apps": {
      const listing: Listing = {
        apps,
        catalog: `${apps.map((app) => app.name).join("\n")}\n\n${picks}`,
      };
      return listing as T;
    }
    case "icon":
      return `/scenes/icons/${iconByPath.get(String(args?.["path"])) ?? ""}.png` as T;
    case "conversations":
      return [...answers] as T;
    case "clipboard_items":
      return clips.map(({ minutesAgo, ...clip }) => ({
        ...clip,
        at: Math.floor(Date.now() / 1000) - minutesAgo * 60,
        width: 0,
        height: 0,
        thumb: clip.thumb === true,
      })) as T;
    case "files": {
      const wanted = String(args?.["query"]);
      if (params.get("fileswait") === "1" && wanted !== params.get("filesnowait")) {
        await new Promise<void>((resolve) => {
          waitingFiles.push({ query: wanted, resolve });
        });
      }
      const words = wanted.toLowerCase().split(/\s+/).filter(Boolean);
      const found = fixtureFiles.filter((file) =>
        words.every((word) => file.name.toLowerCase().includes(word)),
      );
      return {
        query: wanted,
        files: words.some((word) => word.length >= 3) ? found : [],
        indexOff: params.get("indexoff") === "1",
      } as T;
    }
    case "typed_ahead":
      return (window.typedAhead?.shift() ?? null) as T;
    case "quick_look":
      looking = args?.["follow"] === true ? looking : !looking;
      return looking as T;
    case "close_quick_look": {
      const was = looking;
      looking = false;
      return was as T;
    }
    case "pull_requests":
      if (readFails) {
        readFails = false;
        throw "Raiden could not read the saved pull requests.";
      }
      return pullsView() as T;
    case "pull_requests_refresh":
      if (!pullsAllowed) {
        return { ...pullsView(), problem: { kind: "failed", text: "Not allowed yet." } } as T;
      }
      if (params.get("refreshwaits") === "1") {
        await new Promise<void>((resolve) => {
          waitingRefreshes.push(resolve);
        });
      }
      if (params.get("refreshfails") === "1") {
        return pullsView() as T;
      }
      if (pullsProblem?.kind === "missing" || pullsProblem?.kind === "signedOut") {
        if (!ghSignedIn) {
          return pullsView() as T;
        }
        pullsProblem = null;
      }
      pullsSaved = true;
      return pullsView() as T;
    case "pull_requests_allow":
      pullsAllowed = true;
      return pullsView() as T;
    case "pull_requests_avatar":
      return (params.get("avatar") === "none" ? null : pullsFixture.avatar) as T;
    case "pull_requests_pin": {
      const repo = String(args?.["repo"]);
      pinnedRepos = pinnedRepos.filter((pinned) => pinned !== repo);
      if (args?.["pin"] === true) {
        pinnedRepos.push(repo);
      }
      return [...pinnedRepos] as T;
    }
    case "pull_requests_context":
      return pullsContext(
        (args?.["tab"] ?? "all") as Tab,
        (args?.["only"] ?? null) as string | null,
        String(args?.["query"] ?? ""),
        Array.isArray(args?.["urls"]) ? (args["urls"] as string[]) : null,
      ) as T;
    case "clipboard_thumb":
      return "/scenes/icons/shot.png" as T;
    case "clipboard_delete": {
      const at = clips.findIndex((clip) => clip.id === args?.["id"]);
      if (at >= 0) {
        clips.splice(at, 1);
      }
      return null as T;
    }
    case "front_context":
      if (params.get("frontwaits") === "1") {
        await new Promise<void>((resolve) => {
          window.finishFront = resolve;
        });
      }
      return front as T;
    case "ai_accounts":
      return accounts() as T;
    case "ai_sign_in":
      if (params.get("signinwaits") === "1") {
        await new Promise<void>((resolve) => {
          window.finishSignIn = resolve;
        });
      }
      if (args?.["id"] === "chatgpt") {
        chatgptSignedIn = true;
      } else {
        signedIn = true;
      }
      return null as T;
    case "chosen_ai":
      return chosenAi as T;
    case "settings_view": {
      const view: SettingsView = {
        hotkey: settingsFile.hotkey,
        clipboardHotkey: settingsFile.clipboardHotkey,
        ai: chosenAi === "chatgpt" ? "chatgpt" : "claude",
        effort: chosenEffort,
        vim: vimKeys,
        terminal: terminalInUse,
        terminals: fixtureTerminals,
        file: "~/.config/raiden/raiden.toml",
        atLogin: settingsFile.atLogin,
      };
      return view as T;
    }
    case "set_hotkey": {
      const hotkey = String(args?.["hotkey"]);
      if (hotkey === "⌘Space") {
        throw SPOTLIGHT_REFUSAL;
      }
      if (!/[⌃⌥⌘]/.test(hotkey)) {
        throw `"${hotkey}" needs Control, Option or Command, or it would take that key from every app`;
      }
      const taken =
        args?.["which"] === "clipboard" ? settingsFile.hotkey : settingsFile.clipboardHotkey;
      if (hotkey === taken) {
        throw `${hotkey} already opens ${args?.["which"] === "clipboard" ? "Raiden" : "Clipboard history"}`;
      }
      if (args?.["which"] === "clipboard") {
        settingsFile.clipboardHotkey = hotkey;
      } else {
        settingsFile.hotkey = hotkey;
      }
      return hotkey as T;
    }
    case "vim_keys":
      return vimKeys as T;
    case "choose_terminal": {
      const wanted = String(args?.["id"]) as TerminalId;
      if (!fixtureTerminals.some((terminal) => terminal.id === wanted && terminal.app)) {
        throw `Raiden has no installed terminal called ${wanted}`;
      }
      terminalInUse = wanted;
      return wanted as T;
    }
    case "open_in_terminal":
      if (params.get("terminalfails") === "1") {
        throw "Ghostty did not open: no such folder.";
      }
      return null as T;
    case "set_vim_keys":
      vimKeys = args?.["on"] === true;
      return vimKeys as T;
    case "set_open_at_login":
      settingsFile.atLogin = { ...settingsFile.atLogin, on: args?.["on"] === true };
      return settingsFile.atLogin as T;
    case "choose_ai":
      if (params.get("choosefails") === "1") {
        throw "raiden.toml is not valid TOML, so Raiden left it as it is";
      }
      chosenAi = String(args?.["id"]);
      return null as T;
    case "choose_effort":
      if (params.get("choosefails") === "1") {
        throw "raiden.toml is not valid TOML, so Raiden left it as it is";
      }
      if (params.get("effortwaits") === "1") {
        await new Promise((resolve) => setTimeout(resolve, 200));
      }
      chosenEffort = String(args?.["id"]) as Effort;
      return null as T;
    case "ask": {
      const id = String(args?.["answer"]);
      const conversation = String(args?.["conversation"]);
      const followingUp = asked.has(conversation);
      asked.add(conversation);
      replay(id, followingUp, args?.["id"] === "chatgpt" ? CHATGPT_MODEL : recorded.events.model);
      return id as T;
    }
    case "paste_back":
    case "clipboard_paste": {
      const failure = params.get("pastefails");
      if (failure) {
        await new Promise((resolve) => setTimeout(resolve, 50));
        throw failure;
      }
      return null as T;
    }
    case "save_turn": {
      const id = String(args?.["conversation"]);
      const before = answers.find((kept) => kept.id === id);
      const kept: Conversation = {
        id,
        at: Date.now() / 1000,
        model: String(args?.["model"]),
        turns: [
          ...(before?.turns ?? []),
          { question: String(args?.["question"]), answer: String(args?.["answer"]) },
        ],
      };
      answers.splice(0, answers.length, kept, ...answers.filter((other) => other.id !== id));
      return kept as T;
    }
    default:
      return null as T;
  }
};

const listen: Listen = async (handler) => {
  listener = handler;
  return () => {
    listener = undefined;
  };
};

const hear: Hear = async (handler) => {
  window.pressHotkey = handler;
  return () => {
    window.pressHotkey = undefined;
  };
};

const opens: Opens = async (handler) => {
  window.openClipboard = handler;
  return () => {
    window.openClipboard = undefined;
  };
};

const previews: Previews = async (handler) => {
  window.previewEvent = (what: string) => {
    looking = false;
    handler(what);
  };
  return () => {
    window.previewEvent = undefined;
  };
};

const earlyMs = params.get("early") === null ? undefined : Number(params.get("early"));
const root = document.querySelector<HTMLElement>("#app");
const size = tauri.app.windows[0];
if (root && fixture === "quick") {
  root.style.background = "transparent";
  render(
    () => <Hud notice={{ look: "done", text: QUICK_MESSAGE, undo: true, shown: true }} />,
    root,
  );
} else if (root && size) {
  fitRoot();
  render(
    () => (
      <App
        call={call}
        search={fetchSearch}
        listen={listen}
        hear={hear}
        opens={opens}
        previews={previews}
        pulls={pullsOpening}
        earlyMs={earlyMs}
      />
    ),
    root,
  );
  const field = root.querySelector("input");
  const typed = moment && moment !== "waiting" ? recorded.question : query;
  if (field && typed && fixture !== "clipboard" && fixture !== "pulls") {
    field.value = typed;
    field.dispatchEvent(new InputEvent("input", { bubbles: true }));
  }
  if (field && moment) {
    field.dispatchEvent(new KeyboardEvent("keydown", { key: "Tab", bubbles: true }));
  }
  if (fixture === "clipboard") {
    window.openClipboard?.(false);
    if (field && query) {
      field.value = query;
      field.dispatchEvent(new InputEvent("input", { bubbles: true }));
    }
  }
  if (field && fixture === "settings") {
    field.dispatchEvent(new KeyboardEvent("keydown", { key: ",", metaKey: true, bubbles: true }));
  }
  if (field && moment === "thread") {
    const followUp = () => {
      if (!root.querySelector('#ai[data-phase="done"]')) {
        requestAnimationFrame(followUp);
        return;
      }
      field.value = recorded.followUp.question;
      field.dispatchEvent(new InputEvent("input", { bubbles: true }));
      field.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
    };
    followUp();
  }
}

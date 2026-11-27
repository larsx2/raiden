import type { AnswerEvent } from "../src/ask";
import type { TerminalId } from "../src/terminals";
import type { Typed } from "../src/typeahead";

export type Fixture =
  | "empty"
  | "apps"
  | "answer"
  | "recent-answers"
  | "quick"
  | "settings"
  | "clipboard"
  | "files"
  | "contents"
  | "pulls";

export type SentCall = { command: string; args?: Record<string, unknown> };

declare global {
  interface Window {
    sent: SentCall[];
    emitAnswer: (event: AnswerEvent) => void;
    finishSignIn?: () => void;
    finishFront?: () => void;
    finishFiles?: (query?: string) => void;
    finishContents?: (query?: string) => void;
    finishBuilding?: () => void;
    contentsReady?: () => void;
    finishRefresh?: () => void;
    signInToGh?: () => void;
    pressHotkey?: (keys: string) => void;
    editSettings: (edit: { hotkey?: string; ai?: string; effort?: string; vim?: boolean }) => void;
    openClipboard?: (shown: boolean) => void;
    typedAhead?: Typed[][];
  }
}

export const EXPANDED_VIEWPORT = { width: 1000, height: 760 };

export const SCENE_TIME = "2026-11-25T15:00:00";

export type FixtureClip = {
  kind: "text" | "link" | "file" | "image";
  text: string;
  title: string;
  app: string;
  minutesAgo: number;
  thumb?: boolean;
};

export const FIXTURE_CLIPS: FixtureClip[] = [
  {
    kind: "text",
    text: "git worktree add .cache/launcher -b launcher origin/main",
    title: "git worktree add .cache/launcher -b launcher origin/main",
    app: "Terminal",
    minutesAgo: 2,
  },
  {
    kind: "link",
    text: "https://gpui.rs/",
    title: "https://gpui.rs/",
    app: "Safari",
    minutesAgo: 9,
  },
  {
    kind: "image",
    text: "",
    title: "Image, 1280 × 720",
    app: "Preview",
    minutesAgo: 25,
    thumb: true,
  },
  {
    kind: "file",
    text: "/Users/me/Code/lantern/AGENTS.md",
    title: "~/Code/lantern/AGENTS.md",
    app: "Finder",
    minutesAgo: 31,
  },
  { kind: "text", text: "just land", title: "just land", app: "Terminal", minutesAgo: 26 * 60 },
  {
    kind: "text",
    text: "NSWindowCollectionBehavior.fullScreenAuxiliary",
    title: "NSWindowCollectionBehavior.fullScreenAuxiliary",
    app: "Zed",
    minutesAgo: 28 * 60,
  },
];

export const FIXTURE_FILES: Array<[path: string, icon: string]> = [
  ["/Users/me/Code/lantern/docs/notes.md", "file-md"],
  ["/Users/me/Documents/Release notes.txt", "file-txt"],
  ["/Users/me/Documents/Meeting notes", "file-folder"],
  ["/Users/me/Code/lantern/AGENTS.md", "file-md"],
];

export const FIXTURE_CONTENTS: Array<[path: string, icon: string, lines: string[]]> = [
  [
    "/Users/me/Documents/notes.md",
    "file-md",
    ["# Planning", "", "The quarterly plan is due Friday.", "Ask Dana about the budget."],
  ],
  [
    "/Users/me/Code/lantern/src/main.rs",
    "file-txt",
    ["use lantern::run;", "", "fn main() {", "    run();", "}"],
  ],
  [
    "/Users/me/Code/lantern/ui/src/search.ts",
    "file-txt",
    ["export function rank(text: string) {", "  // TODO: rank recent files first", "}"],
  ],
  [
    "/Users/me/Documents/Release notes.txt",
    "file-txt",
    ["Raiden 0.4", "The quarterly release moves to Monday."],
  ],
];

export const FIXTURE_TERMINALS: Array<[id: TerminalId, name: string, app: string | null]> = [
  ["ghostty", "Ghostty", "/Applications/Ghostty.app"],
  ["iterm", "iTerm2", "/Applications/iTerm.app"],
  ["rex", "Rex", "/Applications/Rex.app"],
  ["tern", "Tern", "/Applications/Tern.app"],
  ["terminal", "Terminal", "/System/Applications/Utilities/Terminal.app"],
  ["kitty", "Kitty", null],
  ["alacritty", "Alacritty", null],
  ["warp", "Warp", null],
];

export const QUICK_MESSAGE = "Fixed 1 sentence in TextEdit";

export const SPOTLIGHT_REFUSAL =
  "Spotlight holds ⌘Space. Turn off its shortcut in System Settings, Keyboard, Keyboard Shortcuts, Spotlight.";

export const FIXTURE_APPS: Array<[name: string, icon: string, uses: number]> = [
  ["Calendar", "calapp", 0],
  ["Finder", "finder", 0],
  ["Mail", "mail", 3],
  ["Messages", "msgs", 5],
  ["Music", "music", 0],
  ["Notes", "notes", 0],
  ["Safari", "safari", 9],
  ["System Settings", "settings", 0],
  ["Terminal", "term", 4],
  ["Zed", "zed", 7],
];

export const FIXTURE_ANSWERS: Array<[question: string, hoursAgo: number]> = [
  ["How do I land a change?", 2],
  ["Summarize #22", 26],
];

export const FIXTURE_HISTORY: Array<[questions: string[], hoursAgo: number]> = [
  [["Summarize #22", "Which files does it touch?"], 26],
  [["What does just sync do?"], 50],
];

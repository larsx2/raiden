export type AccountId = "claude" | "chatgpt";
export type Account = { id: AccountId; name: string; through: string; signedIn: boolean };
export type Front = {
  app: string;
  path: string | null;
  window: string | null;
  selection: string | null;
};
export type Tag = {
  kind: "window" | "selection" | "pulls";
  label: string;
  text: string;
  icon: string | null;
  title?: string;
  place?: string;
};
export type Sent = Pick<Tag, "kind" | "label" | "text">;
export type AnswerEvent = {
  id: string;
  kind: "delta" | "searching" | "done" | "error";
  text: string;
  model: string;
  hidden: Record<string, number>;
};
export type SavedTurn = { question: string; answer: string };
export type Conversation = { id: string; at: number; model: string; turns: SavedTurn[] };

export const DEFAULT_ACCOUNT: AccountId = "claude";

export const isAccountId = (value: unknown): value is AccountId =>
  value === "claude" || value === "chatgpt";

const SEPARATORS = [" - ", " — ", " – "];

export function windowTitle(window: string, app: string): string {
  let cut = -1;
  for (const separator of SEPARATORS) {
    const ending = `${separator}${app}`;
    for (let at = window.indexOf(ending); at > 0; at = window.indexOf(ending, at + 1)) {
      const after = window.slice(at + ending.length);
      if (after === "" || SEPARATORS.some((next) => after.startsWith(next))) {
        cut = Math.max(cut, at);
      }
    }
  }
  return cut > 0 ? window.slice(0, cut) : window;
}

export function tagsFor(front: Front | null): Tag[] {
  if (!front?.app) {
    return [];
  }
  const tags: Tag[] = [];
  if (front.window) {
    const title = windowTitle(front.window, front.app);
    const place = title === front.app ? "" : ` in ${front.app}`;
    const text = front.window === front.app ? front.app : `${front.window} in ${front.app}`;
    tags.push({ kind: "window", label: `${title}${place}`, text, icon: front.path, title, place });
  }
  if (front.selection) {
    tags.push({
      kind: "selection",
      label: `Selection from ${front.app}`,
      text: front.selection,
      icon: front.path,
    });
  }
  return tags;
}

export function sent(tags: Tag[]): Sent[] {
  return tags.map(({ kind, label, text }) => ({ kind, label, text }));
}

const SECRET_NOUNS: Record<string, string> = {
  anthropic_api_key: "API key",
  openai_api_key: "API key",
  stripe_secret_key: "API key",
  aws_access_key_id: "AWS key",
  aws_secret_access_key: "AWS key",
  github_token: "token",
  github_fine_grained_token: "token",
  slack_bot_token: "token",
  json_web_token: "token",
  postgres_password: "connection string",
};

export function hiddenLabel(hidden: Record<string, number>): string | null {
  const counted = Object.entries(hidden).filter(([, count]) => count > 0);
  const total = counted.reduce((sum, [, count]) => sum + count, 0);
  if (total === 0) {
    return null;
  }
  const nouns = new Set(counted.map(([kind]) => SECRET_NOUNS[kind] ?? "secret"));
  const noun = nouns.size === 1 ? [...nouns][0] : "secret";
  return `${total} ${noun}${total === 1 ? "" : "s"} hidden`;
}

const CLAUDE_FAMILIES = new Set(["haiku", "sonnet", "opus"]);
const capital = (word: string) => word.charAt(0).toUpperCase() + word.slice(1);

export function modelLabel(model: string, account: string): string {
  const words = model
    .trim()
    .toLowerCase()
    .split(/[-_\s]+/)
    .filter((word) => word && !/^\d{8}$/.test(word));
  if (words.length === 0) {
    return account;
  }
  if (words[0] === "claude" || CLAUDE_FAMILIES.has(words[0] ?? "")) {
    const rest = words[0] === "claude" ? words.slice(1) : words;
    const names = rest.filter((word) => !/^\d+$/.test(word)).map(capital);
    const version = rest.filter((word) => /^\d+$/.test(word)).join(".");
    return ["Claude", ...names, version].filter(Boolean).join(" ");
  }
  if (words[0]?.startsWith("gpt")) {
    return model
      .trim()
      .split("-")
      .map((part, at) => (at === 0 ? part.toUpperCase() : capital(part)))
      .join("-");
  }
  return model.trim();
}

export function madeBy(model: string): AccountId | null {
  const label = modelLabel(model, "");
  if (label.startsWith("Claude")) {
    return "claude";
  }
  return label.startsWith("GPT") ? "chatgpt" : null;
}

const wordsOf = (text: string) =>
  text
    .toLowerCase()
    .split(/[^\p{L}\p{N}]+/u)
    .filter(Boolean);

export type Findable = { words: string[] };

export function findable(conversation: Conversation): Findable {
  return { words: conversation.turns.flatMap((turn) => wordsOf(turn.question)) };
}

export function titleOf(conversation: Conversation): string {
  return conversation.turns[0]?.question ?? "";
}

export function matchesAnswer(found: Findable, query: string): boolean {
  const wanted = wordsOf(query);
  return (
    wanted.length > 0 && wanted.every((part) => found.words.some((word) => word.startsWith(part)))
  );
}

const HOUR = 3600;

export function ageLabel(at: number, now: number): string {
  const seconds = Math.max(0, now - at);
  if (seconds < 60) {
    return "Just now";
  }
  if (seconds < HOUR) {
    return `${Math.floor(seconds / 60)} min ago`;
  }
  if (seconds < 24 * HOUR) {
    return `${Math.floor(seconds / HOUR)} h ago`;
  }
  if (seconds < 48 * HOUR) {
    return "Yesterday";
  }
  return new Date(at * 1000).toLocaleDateString("en-GB", { day: "numeric", month: "short" });
}

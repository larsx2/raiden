export type Span =
  | { kind: "text"; text: string }
  | { kind: "code"; text: string }
  | { kind: "strong"; spans: Span[] }
  | { kind: "em"; spans: Span[] }
  | { kind: "link"; url: string; href: string; spans: Span[] };

export type Item = { spans: Span[]; blocks: Block[] };
export type List = { kind: "list"; ordered: boolean; start: number; items: Item[] };
export type Block =
  | { kind: "paragraph"; spans: Span[] }
  | { kind: "heading"; spans: Span[] }
  | { kind: "code"; text: string; lang: string; open: boolean }
  | { kind: "quote"; blocks: Block[] }
  | List;

type Found = { spans: Span[]; end: number };

const SPECIAL = /[`[!<*_\\hH]/g;
const ESCAPABLE = /[!-/:-@[-`{-~]/;
const SPACE = /\s/;
const WORD = /[\p{L}\p{N}]/u;
const FENCE = /^( *)(`{3,}(?=[^`]*$)|~{3,})[ \t]*([^\s{]*)/;
const RULE = /^ {0,3}([-*_])(?: *\1){2,} *$/;
const HEADING = /^ {0,3}#{1,6}(?: |$)/;
const QUOTE = /^ {0,3}> ?/;
const ITEM = /^( *)([-*+]|\d{1,9}[.)]) +(.*)$/;
const NESTED = 2;
const LOOK = 4000;
const LABEL = 500;
const URL_LONGEST = 2048;
const DEEPEST_PARENTHESES = 32;
const AUTOLINK = new RegExp(`<(https?://[^\\s<>]{1,${URL_LONGEST}})>`, "iy");
const BARE_LINK = new RegExp(`https?://[^\\s<>"\`]{0,${URL_LONGEST}}`, "iy");
const DEEPEST = 4;

export function webLink(raw: string): string | null {
  try {
    const url = new URL(raw.trim());
    return url.protocol === "http:" || url.protocol === "https:" ? url.href : null;
  } catch {
    return null;
  }
}

function pushText(spans: Span[], text: string) {
  const last = spans.at(-1);
  if (last?.kind === "text") {
    last.text += text;
  } else if (text) {
    spans.push({ kind: "text", text });
  }
}

function pushAll(spans: Span[], more: Span[]) {
  for (const span of more) {
    if (span.kind === "text") {
      pushText(spans, span.text);
    } else {
      spans.push(span);
    }
  }
}

const runAt = (text: string, at: number, mark: string) => {
  let end = at;
  while (text[end] === mark) {
    end++;
  }
  return end - at;
};

type Scanned = Map<string, { from: number; to: number }>;
type Context = { codes: Map<number, Found>; scanned: Scanned; unpaired: Scanned };

function codeAt(text: string, at: number, context: Context): Found | null {
  if (text[at] !== "`") {
    return null;
  }
  const known = context.codes.get(at);
  if (known) {
    return known;
  }
  const found = codeFrom(text, at, context);
  context.codes.set(at, found);
  return found;
}

function codeFrom(text: string, at: number, context: Context): Found {
  const size = runAt(text, at, "`");
  const limit = at + LOOK;
  const start = at + size;
  const known = context.unpaired.get(String(size));
  let from = known && known.from <= start && start < known.to ? known.to : start;
  while (from < text.length) {
    const next = text.indexOf("`", from);
    if (next < 0 || next > limit) {
      from = next < 0 ? text.length : next;
      break;
    }
    const run = runAt(text, next, "`");
    if (run === size) {
      const inner = text.slice(at + size, next).replace(/\n/g, " ");
      const padded = inner.startsWith(" ") && inner.endsWith(" ") && inner.trim() !== "";
      const trimmed = padded ? inner.slice(1, -1) : inner;
      return { spans: [{ kind: "code", text: trimmed }], end: next + run };
    }
    from = next + run;
  }
  const joins = known && from >= known.from && start <= known.to;
  context.unpaired.set(
    String(size),
    joins
      ? { from: Math.min(known.from, start), to: Math.max(known.to, from) }
      : { from: start, to: from },
  );
  return { spans: [{ kind: "text", text: "`".repeat(size) }], end: at + size };
}

function closingBracket(text: string, open: number, context: Context): number {
  let depth = 0;
  for (let at = open; at < Math.min(text.length, open + LABEL);) {
    const char = text[at];
    if (char === "\\") {
      at += 2;
      continue;
    }
    if (char === "`") {
      at = codeAt(text, at, context)?.end ?? at + 1;
      continue;
    }
    if (char === "[") {
      depth++;
    } else if (char === "]") {
      depth--;
      if (depth === 0) {
        return at;
      }
    }
    at++;
  }
  return -1;
}

function destinationAt(text: string, start: number): { url: string; end: number } | null {
  const limit = Math.min(text.length, start + URL_LONGEST);
  const find = (wanted: string, from: number) => {
    const found = text.slice(from, limit).indexOf(wanted);
    return found < 0 ? -1 : from + found;
  };
  let at = start;
  while (text[at] === " ") {
    at++;
  }
  let url: string;
  if (text[at] === "<") {
    const end = find(">", at);
    if (end < 0 || text.slice(at, end).includes("\n")) {
      return null;
    }
    url = text.slice(at + 1, end);
    at = end + 1;
  } else {
    const from = at;
    let depth = 0;
    while (at < limit) {
      const char = text[at] ?? "";
      if (char === "\\" && at + 1 < limit) {
        at += 2;
        continue;
      }
      if (char <= " " || (char === ")" && depth === 0)) {
        break;
      }
      depth += char === "(" ? 1 : char === ")" ? -1 : 0;
      if (depth > DEEPEST_PARENTHESES) {
        return null;
      }
      at++;
    }
    url = text.slice(from, at).replace(/\\([!-/:-@[-`{-~])/g, "$1");
  }
  while (text[at] === " " || text[at] === "\n") {
    at++;
  }
  const quote = text[at];
  if (quote === '"' || quote === "'") {
    const end = find(quote, at + 1);
    if (end < 0) {
      return null;
    }
    at = end + 1;
    while (text[at] === " ") {
      at++;
    }
  }
  return text[at] === ")" ? { url, end: at + 1 } : null;
}

function linkAt(text: string, at: number, context: Context): Found | null {
  const open = text[at] === "!" && text[at + 1] === "[" ? at + 1 : at;
  if (text[open] !== "[") {
    return null;
  }
  const close = closingBracket(text, open, context);
  if (close < 0 || text[close + 1] !== "(") {
    return null;
  }
  const target = destinationAt(text, close + 2);
  if (!target) {
    return null;
  }
  const label = text.slice(open + 1, close);
  const spans = label.trim()
    ? spansOf(label, false)
    : [{ kind: "text" as const, text: target.url }];
  const href = webLink(target.url);
  return {
    spans: href ? [{ kind: "link", url: target.url.trim(), href, spans }] : spans,
    end: target.end,
  };
}

function autolinkAt(text: string, at: number): Found | null {
  if (text[at] !== "<") {
    return null;
  }
  AUTOLINK.lastIndex = at;
  const found = AUTOLINK.exec(text);
  const url = found?.[1];
  const href = url ? webLink(url) : null;
  if (!found || !url || !href) {
    return null;
  }
  return {
    spans: [{ kind: "link", url, href, spans: [{ kind: "text", text: url }] }],
    end: at + found[0].length,
  };
}

const TRAILING = ".,;:!?*_~'\"。、，；：！？";

function balanceOf(url: string, open: string, close: string): number {
  let sum = 0;
  for (let at = 0; at < url.length; at++) {
    const char = url[at];
    sum += char === open ? 1 : char === close ? -1 : 0;
  }
  return sum;
}

function trimUrl(url: string): string {
  let parentheses: number | undefined;
  let brackets: number | undefined;
  let end = url.length;
  while (end > 0) {
    const last = url[end - 1] ?? "";
    if (last === ")") {
      parentheses ??= balanceOf(url, "(", ")");
      if (parentheses >= 0) {
        break;
      }
      parentheses++;
    } else if (last === "]") {
      brackets ??= balanceOf(url, "[", "]");
      if (brackets >= 0) {
        break;
      }
      brackets++;
    } else if (!TRAILING.includes(last)) {
      break;
    }
    end--;
  }
  return url.slice(0, end);
}

function bareLinkAt(text: string, at: number): Found | null {
  const start = text[at];
  if (
    (start !== "h" && start !== "H") ||
    WORD.test(text[at - 1] ?? "") ||
    text[at - 1] === "<" ||
    text.slice(at - 2, at) === "](" ||
    !/^https?:\/\//i.test(text.slice(at, at + 8))
  ) {
    return null;
  }
  BARE_LINK.lastIndex = at;
  const found = BARE_LINK.exec(text)?.[0] ?? "";
  const url = trimUrl(found);
  const href = webLink(url);
  if (!href) {
    return null;
  }
  return {
    spans: [{ kind: "link", url, href, spans: [{ kind: "text", text: url }] }],
    end: at + url.length,
  };
}

function skip(text: string, at: number, context: Context): number {
  const char = text[at];
  if (char === "\\") {
    return at + 2;
  }
  if (char === "`") {
    return codeAt(text, at, context)?.end ?? at + 1;
  }
  return at + 1;
}

function emphasisAt(text: string, at: number, linking: boolean, context: Context): Found | null {
  const mark = text[at];
  if (mark !== "*" && mark !== "_") {
    return null;
  }
  const run = runAt(text, at, mark);
  const literal = { spans: [{ kind: "text" as const, text: mark.repeat(run) }], end: at + run };
  const after = text[at + run] ?? "";
  if (!after || SPACE.test(after) || (mark === "_" && WORD.test(text[at - 1] ?? ""))) {
    return literal;
  }
  const size = Math.min(run, 2);
  const key = mark.repeat(size);
  const known = context.scanned.get(key);
  const limit = Math.min(text.length, at + LOOK);
  let close = at + run;
  if (known && known.from <= close && close < known.to) {
    close = known.to;
  }
  while (close < limit) {
    if (text[close] !== mark) {
      close = skip(text, close, context);
      continue;
    }
    const length = runAt(text, close, mark);
    const before = text[close - 1] ?? "";
    const next = text[close + length] ?? "";
    const fits = length === size || length >= 3;
    if (fits && !SPACE.test(before) && (mark === "*" || !WORD.test(next))) {
      const end = close + length - size;
      const spans = spansOf(text.slice(at + size, end), linking);
      return {
        spans: [{ kind: size === 2 ? "strong" : "em", spans }],
        end: close + length,
      };
    }
    if (length === size && SPACE.test(before) && next && !SPACE.test(next)) {
      return literal;
    }
    close += length;
  }
  context.scanned.set(key, { from: at + run, to: close });
  return literal;
}

export function spansOf(text: string, linking = true): Span[] {
  const spans: Span[] = [];
  const context: Context = { codes: new Map(), scanned: new Map(), unpaired: new Map() };
  let at = 0;
  while (at < text.length) {
    const linked = linking
      ? (linkAt(text, at, context) ?? autolinkAt(text, at) ?? bareLinkAt(text, at))
      : null;
    if (linked && /[`\\]/.test(text.slice(at, linked.end))) {
      context.scanned.clear();
    }
    const found = codeAt(text, at, context) ?? linked ?? emphasisAt(text, at, linking, context);
    if (found) {
      pushAll(spans, found.spans);
      at = found.end;
      continue;
    }
    const char = text[at] ?? "";
    if (char === "\\" && ESCAPABLE.test(text[at + 1] ?? "")) {
      pushText(spans, text[at + 1] ?? "");
      at += 2;
      continue;
    }
    SPECIAL.lastIndex = at + 1;
    const next = SPECIAL.exec(text)?.index ?? text.length;
    pushText(spans, text.slice(at, next));
    at = next;
  }
  return spans;
}

const indentOf = (line: string) => line.length - line.trimStart().length;

const dedent = (line: string, width: number) => line.slice(Math.min(width, indentOf(line)));

const headingOf = (line: string) =>
  HEADING.test(line)
    ? line
        .trim()
        .replace(/^#+/, "")
        .trim()
        .replace(/(?:^| )#+$/, "")
        .trim()
    : null;

const isOrdered = (marker: string) => /\d/.test(marker);

const interrupts = (line: string) => {
  const item = ITEM.exec(line);
  const later = item && isOrdered(item[2] ?? "") && Number.parseInt(item[2] ?? "", 10) !== 1;
  return !later && startsBlock(line);
};

const startsBlock = (line: string) =>
  FENCE.test(line) || RULE.test(line) || HEADING.test(line) || QUOTE.test(line) || ITEM.test(line);

function fenceEnd(lines: string[], start: number, marker: string): number {
  const closing = new RegExp(`^ *${marker[0] === "`" ? "`" : "~"}{${marker.length},} *$`);
  for (let at = start + 1; at < lines.length; at++) {
    if (closing.test(lines[at] ?? "")) {
      return at;
    }
  }
  return lines.length;
}

function listFrom(lines: string[], start: number, depth: number): { list: List; end: number } {
  const first = ITEM.exec(lines[start] ?? "");
  const base = first?.[1]?.length ?? 0;
  const ordered = isOrdered(first?.[2] ?? "");
  const list: List = {
    kind: "list",
    ordered,
    start: ordered ? Number.parseInt(first?.[2] ?? "1", 10) : 1,
    items: [],
  };
  let text: string[] = [];
  let children: string[] = [];
  let blank = false;
  const finish = () => {
    if (text.length > 0 || children.length > 0) {
      const width = children
        .filter((line) => line.trim())
        .reduce((least, line) => Math.min(least, indentOf(line)), Infinity);
      list.items.push({
        spans: spansOf(text.join("\n")),
        blocks: blocksFrom(
          children.map((line) => dedent(line, width)),
          depth + 1,
        ),
      });
    }
    text = [];
    children = [];
  };
  let at = start;
  for (; at < lines.length; at++) {
    const line = lines[at] ?? "";
    if (!line.trim()) {
      blank = true;
      if (children.length > 0) {
        children.push("");
      }
      continue;
    }
    const item = ITEM.exec(line);
    const indent = indentOf(line);
    if (indent < base + NESTED && item) {
      if (isOrdered(item[2] ?? "") !== ordered) {
        break;
      }
      finish();
      text = [item[3] ?? ""];
      blank = false;
      continue;
    }
    if (indent >= base + NESTED) {
      if (children.length === 0 && !blank && !item && !FENCE.test(line)) {
        text.push(line.trim());
      } else {
        children.push(line);
      }
      blank = false;
      continue;
    }
    if (blank || startsBlock(line)) {
      break;
    }
    if (children.length > 0) {
      children.push(line);
    } else {
      text.push(line.trim());
    }
  }
  finish();
  while (at > start && !lines[at - 1]?.trim()) {
    at--;
  }
  return { list, end: at };
}

function blocksFrom(lines: string[], depth = 0): Block[] {
  if (depth > DEEPEST) {
    const text = lines
      .map((line) => line.trim())
      .filter(Boolean)
      .join("\n");
    return text ? [{ kind: "paragraph", spans: spansOf(text) }] : [];
  }
  const blocks: Block[] = [];
  let at = 0;
  while (at < lines.length) {
    const line = lines[at] ?? "";
    const fence = FENCE.exec(line);
    const heading = headingOf(line);
    if (!line.trim() || (!fence && RULE.test(line))) {
      at++;
    } else if (fence) {
      const end = fenceEnd(lines, at, fence[2] ?? "```");
      const indent = fence[1]?.length ?? 0;
      const code = lines.slice(at + 1, end).map((kept) => dedent(kept, indent));
      const lang = (fence[3] ?? "").toLowerCase();
      blocks.push({ kind: "code", text: code.join("\n"), lang, open: end === lines.length });
      at = end + 1;
    } else if (heading !== null) {
      blocks.push({ kind: "heading", spans: spansOf(heading) });
      at++;
    } else if (QUOTE.test(line)) {
      let end = at;
      while (end < lines.length && QUOTE.test(lines[end] ?? "")) {
        end++;
      }
      const quoted = lines.slice(at, end).map((kept) => kept.replace(QUOTE, ""));
      blocks.push({ kind: "quote", blocks: blocksFrom(quoted, depth + 1) });
      at = end;
    } else if (ITEM.test(line)) {
      const { list, end } = listFrom(lines, at, depth);
      blocks.push(list);
      at = end;
    } else {
      let end = at + 1;
      while (end < lines.length && lines[end]?.trim() && !interrupts(lines[end] ?? "")) {
        end++;
      }
      const text = lines
        .slice(at, end)
        .map((kept) => kept.trim())
        .join("\n");
      blocks.push({ kind: "paragraph", spans: spansOf(text) });
      at = end;
    }
  }
  return blocks;
}

export function blocksOf(text: string): Block[] {
  const lines = text
    .replace(/\r\n?/g, "\n")
    .split("\n")
    .map((line) => line.replace(/^\t+/, (tabs) => "    ".repeat(tabs.length)));
  return blocksFrom(lines);
}

function shared<T>(old: T, next: T): T {
  if (old === next || typeof old !== "object" || typeof next !== "object" || !old || !next) {
    return next;
  }
  if (Array.isArray(old) !== Array.isArray(next)) {
    return next;
  }
  const before = old as Record<string, unknown>;
  const after = next as Record<string, unknown>;
  const keys = Object.keys(after);
  let same = keys.length === Object.keys(before).length;
  const kept = (Array.isArray(next) ? [] : {}) as Record<string, unknown>;
  for (const key of keys) {
    kept[key] = shared(before[key], after[key]);
    same &&= kept[key] === before[key];
  }
  return same ? old : (kept as T);
}

export function blocksAfter(previous: Block[], text: string): Block[] {
  return shared(previous, blocksOf(text));
}

function plainSpans(spans: Span[]): string {
  return spans
    .map((span) => {
      switch (span.kind) {
        case "text":
          return span.text;
        case "code":
          return `\`${span.text}\``;
        case "link": {
          const label = plainSpans(span.spans);
          return label === span.url ? span.url : `${label} (${span.url})`;
        }
        default:
          return plainSpans(span.spans);
      }
    })
    .join("");
}

const hang = (text: string, width: number) => text.replace(/\n(?=.)/g, `\n${" ".repeat(width)}`);

function plainBlock(block: Block): string {
  switch (block.kind) {
    case "code":
      return block.text;
    case "quote":
      return plainBlocks(block.blocks).replace(/^/gm, "> ").replace(/^> $/gm, ">");
    case "list":
      return block.items
        .map((item, at) => {
          const marker = block.ordered ? `${block.start + at}. ` : "- ";
          const body = [plainSpans(item.spans), plainBlocks(item.blocks)].filter(Boolean);
          return marker + hang(body.join("\n"), marker.length);
        })
        .join("\n");
    default:
      return plainSpans(block.spans);
  }
}

function plainBlocks(blocks: Block[]): string {
  return blocks.map(plainBlock).join("\n\n");
}

export function plainOf(text: string): string {
  return plainBlocks(blocksOf(text));
}

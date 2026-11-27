export type ContentRow = {
  name: string;
  path: string;
  folder: string;
  icon: string;
  line: number;
  text: string;
};
export type ContentState = "ready" | "building" | "waiting" | "off";
export type ContentAnswer = { query: string; rows: ContentRow[]; state: ContentState };

export const CONTENTS_TITLE = "Text in files";

const SHORTEST = 3;
const OPENING = /^["“”]/u;
const CLOSING = /["“”]$/u;

export function startsContent(text: string): boolean {
  return OPENING.test(text.trimStart());
}

export function contentQuery(text: string): string | null {
  if (!startsContent(text)) {
    return null;
  }
  const wanted = text.trim().slice(1).replace(CLOSING, "").trim();
  return [...wanted].length >= SHORTEST ? wanted : null;
}

export function contentKey(row: ContentRow): string {
  return `${row.path}:${row.line}`;
}

export function contentPlace(row: ContentRow): string {
  return `${row.folder}:${row.line}`;
}

const asciiLower = (text: string) => text.replace(/[A-Z]/g, (letter) => letter.toLowerCase());

export function stillContaining(rows: ContentRow[], wanted: string): ContentRow[] {
  const phrase = asciiLower(wanted);
  return rows.filter((row) => asciiLower(row.text).includes(phrase));
}

export function contentNote(state: ContentState): string {
  switch (state) {
    case "building":
      return "Raiden is reading your files to search their text. Results will come in as it finishes.";
    case "waiting":
      return "Raiden reads your files' text while your Mac is plugged in.";
    case "off":
      return "Text in files is not searched right now.";
    case "ready":
      return "";
  }
}

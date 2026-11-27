export type Typed = { key: string; text: string };

export type Field = Pick<HTMLInputElement, "value" | "selectionStart" | "selectionEnd"> & {
  setSelectionRange: (start: number, end: number) => void;
};

const MOST_PULLS = 50;
const STOPS = new Set(["Enter", "Escape"]);

const splice = (field: Field, start: number, end: number, text: string) => {
  field.value = field.value.slice(0, start) + text + field.value.slice(end);
  field.setSelectionRange(start + text.length, start + text.length);
};

const isLowSurrogate = (code: number) => code >= 0xdc00 && code <= 0xdfff;

const deleteBack = (field: Field, start: number, end: number) => {
  if (start !== end) {
    splice(field, start, end, "");
  } else if (start > 0) {
    const width = start > 1 && isLowSurrogate(field.value.charCodeAt(start - 1)) ? 2 : 1;
    splice(field, start - width, start, "");
  }
};

export function applyTyped(
  typed: Typed,
  field: Field,
  runKey: (key: string) => boolean,
  changed: () => void,
): void {
  const end = field.selectionEnd ?? field.value.length;
  const start = field.selectionStart ?? end;
  if (typed.text) {
    splice(field, start, end, typed.text);
    changed();
  } else if (!runKey(typed.key) && typed.key === "Backspace") {
    deleteBack(field, start, end);
    changed();
  }
}

export async function catchUp(
  pull: () => Promise<Typed[] | null>,
  apply: (typed: Typed) => void,
): Promise<void> {
  for (let pulls = 0; pulls < MOST_PULLS; pulls += 1) {
    const batch = await pull().catch(() => null);
    if (!batch) {
      return;
    }
    for (const typed of batch) {
      apply(typed);
      if (STOPS.has(typed.key)) {
        return;
      }
    }
  }
}

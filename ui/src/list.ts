export type Section = { name: string; start: number };
export type Slot = { heading: string } | { row: number };
export type Piece = { text: string; matched: boolean };

export function sectionsOf(groups: Array<[name: string, rows: number]>): Section[] {
  const sections: Section[] = [];
  let start = 0;
  for (const [name, rows] of groups) {
    if (rows > 0) {
      sections.push({ name, start });
      start += rows;
    }
  }
  return sections;
}

export function slotCount(sections: Section[], rows: number): number {
  return rows + sections.length;
}

export function slotAt(sections: Section[], slot: number): Slot {
  for (let at = sections.length - 1; at >= 0; at--) {
    const heading = (sections[at]?.start ?? 0) + at;
    if (slot === heading) {
      return { heading: sections[at]?.name ?? "" };
    }
    if (slot > heading) {
      return { row: slot - at - 1 };
    }
  }
  return { row: slot };
}

export function slotOf(sections: Section[], row: number): number {
  return row + sections.filter((section) => section.start <= row).length;
}

export function startsSection(sections: Section[], row: number): boolean {
  return sections.some((section) => section.start === row);
}

export function clamp(row: number, count: number): number {
  return Math.max(0, Math.min(row, count - 1));
}

export function pieces(name: string, positions: ArrayLike<number>): Piece[] {
  const matched = new Set(Array.from(positions));
  const found: Piece[] = [];
  [...name].forEach((letter, at) => {
    const isMatched = matched.has(at);
    const last = found[found.length - 1];
    if (last && last.matched === isMatched) {
      last.text += letter;
    } else {
      found.push({ text: letter, matched: isMatched });
    }
  });
  return found;
}

export function startingRows(count: number, suggested: ArrayLike<number>): Uint32Array {
  const rows = new Uint32Array(count);
  rows.set(Array.from(suggested));
  const taken = new Set(Array.from(suggested));
  let at = suggested.length;
  for (let index = 0; index < count; index++) {
    if (!taken.has(index)) {
      rows[at++] = index;
    }
  }
  return rows;
}

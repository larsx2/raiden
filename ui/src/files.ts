export type FileRow = { name: string; path: string; folder: string; icon: string };
export type FileAnswer = { query: string; files: FileRow[]; indexOff: boolean };

const SHORTEST = 3;

const fold = (text: string) => text.normalize("NFD").replace(/\p{M}/gu, "").toLowerCase();
const lettersOf = (text: string) => [...text.normalize("NFC")].map(fold);
const wordsOf = (text: string) => fold(text).split(/\s+/).filter(Boolean);

export function searchesFiles(text: string): boolean {
  return text.split(/\s+/).some((word) => [...word].length >= SHORTEST);
}

export function wordPositions(name: string, text: string): number[] {
  const letters = lettersOf(name);
  const marked = new Set<number>();
  for (const word of wordsOf(text)) {
    const wanted = lettersOf(word);
    const at = letters.findIndex((_, start) =>
      wanted.every((letter, offset) => letters[start + offset] === letter),
    );
    wanted.forEach((_, offset) => at >= 0 && marked.add(at + offset));
  }
  return [...marked].toSorted((a, b) => a - b);
}

export function stillMatching(files: FileRow[], text: string): FileRow[] {
  const words = wordsOf(text);
  return files.filter((file) => {
    const name = lettersOf(file.name).join("");
    return words.every((word) => name.includes(word));
  });
}

import wasmUrl from "./generated/search.wasm?url";

export type Search = {
  load: (catalog: string) => number;
  rank: (query: string) => Uint32Array;
  positions: (index: number, query: string) => Uint32Array;
  suggestions: (limit: number) => Uint32Array;
};

type Exports = {
  memory: WebAssembly.Memory;
  input: (length: number) => number;
  load: () => number;
  rank: () => number;
  positions: (index: number) => number;
  suggestions: (limit: number) => number;
  output: () => number;
};

export async function createSearch(bytes: BufferSource): Promise<Search> {
  const { instance } = await WebAssembly.instantiate(bytes);
  const core = instance.exports as unknown as Exports;
  const encoder = new TextEncoder();
  const write = (text: string) => {
    const data = encoder.encode(text);
    const at = core.input(data.length);
    new Uint8Array(core.memory.buffer, at, data.length).set(data);
  };
  const read = (length: number) =>
    new Uint32Array(core.memory.buffer, core.output(), length).slice();
  return {
    load: (catalog) => {
      write(catalog);
      return core.load();
    },
    rank: (query) => {
      write(query);
      return read(core.rank());
    },
    positions: (index, query) => {
      write(query);
      return read(core.positions(index));
    },
    suggestions: (limit) => read(core.suggestions(limit)),
  };
}

export async function fetchSearch(): Promise<Search> {
  const response = await fetch(wasmUrl);
  return createSearch(await response.arrayBuffer());
}

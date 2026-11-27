import { For, Show, type JSX } from "solid-js";
import { BRANDS, SPARKLE } from "./brands";

export type GlyphName =
  | "gear"
  | "keyboard"
  | "power"
  | "gauge"
  | "prompt"
  | "terminal"
  | "pen"
  | "file"
  | "sparkle"
  | "clipboard"
  | "text"
  | "link"
  | "image";

const GLYPHS: Record<GlyphName, () => JSX.Element> = {
  gear: () => (
    <>
      <circle cx="12" cy="12" r="3" />
      <path d="M12 3v3M12 18v3M3 12h3M18 12h3M5.6 5.6l2.1 2.1M16.3 16.3l2.1 2.1M5.6 18.4l2.1-2.1M16.3 7.7l2.1-2.1" />
    </>
  ),
  terminal: () => (
    <>
      <rect x="3" y="5" width="18" height="14" rx="2.5" />
      <path d="M7 10l2.5 2L7 14M12 14h4.5" />
    </>
  ),
  keyboard: () => (
    <>
      <rect x="3" y="6" width="18" height="12" rx="2" />
      <path d="M7 10h1M11 10h2M16 10h1M8 14h8" />
    </>
  ),
  power: () => <path d="M12 3v9M6.3 7.3a8 8 0 1 0 11.4 0" />,
  gauge: () => <path d="M4.5 17a8 8 0 1 1 15 0M12 14l3.5-4.5" />,
  prompt: () => <path d="M5 7l5 5-5 5M12 17h7" />,
  pen: () => <path d="M4 20h4L19 9l-4-4L4 16z" />,
  file: () => <path d="M7 4h7l4 4v12H7zM14 4v4h4" />,
  sparkle: () => <path d={SPARKLE} fill="currentColor" stroke="none" />,
  clipboard: () => (
    <>
      <rect x="6" y="5" width="12" height="15" rx="2" />
      <path d="M9 5V4h6v1M9 11h6M9 15h4" />
    </>
  ),
  text: () => <path d="M6 7h12M6 12h12M6 17h8" />,
  link: () => (
    <>
      <path d="M10 14a4 4 0 0 0 5.7 0l3-3a4 4 0 0 0-5.7-5.7l-1 1" />
      <path d="M14 10a4 4 0 0 0-5.7 0l-3 3a4 4 0 0 0 5.7 5.7l1-1" />
    </>
  ),
  image: () => (
    <>
      <rect x="4" y="5" width="16" height="14" rx="2" />
      <circle cx="9" cy="10" r="1.5" />
      <path d="M20 16l-5-5-9 8" />
    </>
  ),
};

export function Glyph(props: { name: GlyphName; plain?: boolean }): JSX.Element {
  return (
    <span class={props.plain ? "ico plain" : `ico tint ${props.name}`}>
      <svg viewBox="0 0 24 24" aria-hidden="true">
        {GLYPHS[props.name]()}
      </svg>
    </span>
  );
}

export const GITHUB_MARK =
  "M12 .297c-6.63 0-12 5.373-12 12 0 5.303 3.438 9.8 8.205 11.385.6.113.82-.258.82-.577 0-.285-.01-1.04-.015-2.04-3.338.724-4.042-1.61-4.042-1.61C4.422 18.07 3.633 17.7 3.633 17.7c-1.087-.744.084-.729.084-.729 1.205.084 1.838 1.236 1.838 1.236 1.07 1.835 2.809 1.305 3.495.998.108-.776.417-1.305.76-1.605-2.665-.3-5.466-1.332-5.466-5.93 0-1.31.465-2.38 1.235-3.22-.135-.303-.54-1.523.105-3.176 0 0 1.005-.322 3.3 1.23.96-.267 1.98-.399 3-.405 1.02.006 2.04.138 3 .405 2.28-1.552 3.285-1.23 3.285-1.23.645 1.653.24 2.873.12 3.176.765.84 1.23 1.91 1.23 3.22 0 4.61-2.805 5.625-5.475 5.92.42.36.81 1.096.81 2.22 0 1.606-.015 2.896-.015 3.286 0 .315.21.69.825.57C20.565 22.092 24 17.592 24 12.297c0-6.627-5.373-12-12-12";

export function GitHubMark(): JSX.Element {
  return (
    <span class="ico gh">
      <svg viewBox="0 0 24 24" aria-hidden="true">
        <path d={GITHUB_MARK} />
      </svg>
    </span>
  );
}

export function AiIcon(): JSX.Element {
  return (
    <span class="ico tint ai">
      <svg viewBox="0 0 24 24" aria-hidden="true">
        <path d={SPARKLE} fill="currentColor" />
      </svg>
    </span>
  );
}

export function Brand(props: { id: keyof typeof BRANDS }): JSX.Element {
  const brand = () => BRANDS[props.id];
  return (
    <span class="ico brand" style={{ "--bg": brand().background, "--fg": brand().color }}>
      <svg viewBox="0 0 24 24" aria-hidden="true">
        <path d={brand().path} />
      </svg>
    </span>
  );
}

export function NoMatch(props: { things: string; typed: string }): JSX.Element {
  return (
    <div class="none">
      {`No ${props.things} match “${props.typed}”`}
      <span>Press Esc to clear the search.</span>
    </div>
  );
}

export function Keys(props: { keys: string }): JSX.Element {
  return (
    <span class="keys">
      <For each={props.keys.split(" ")}>{(key) => <kbd>{key}</kbd>}</For>
    </span>
  );
}

export function MenuIcon(props: { src: string | undefined }): JSX.Element {
  return (
    <span class="aico">
      <Show when={props.src}>{(src) => <img src={src()} alt="" />}</Show>
    </span>
  );
}

export function Check(): JSX.Element {
  return (
    <svg class="check" viewBox="0 0 16 16" role="img" aria-label="In use">
      <path d="M3.5 8.5l3 3 6-7" />
    </svg>
  );
}

export type Action = {
  title: string;
  keys: string;
  hides: boolean;
  run: () => void;
  sub?: string;
  about?: string;
  section?: string;
  note?: string;
  app?: string;
  image?: string;
  checked?: boolean;
  off?: boolean;
};

export type AppTarget = { path: string };

export const QUICK_LOOK_KEYS = "⌘ Y";

export type AppHandlers = {
  open: () => void;
  showInFinder: (path: string) => void;
  copyPath: (path: string) => void;
  quickLook?: (path: string) => void;
};

export function appActions(app: AppTarget, handlers: AppHandlers): Action[] {
  return [
    { title: "Open", keys: "↵", hides: true, run: handlers.open },
    {
      title: "Show in Finder",
      keys: "⌘ ↵",
      hides: true,
      run: () => handlers.showInFinder(app.path),
    },
    ...(handlers.quickLook
      ? [
          {
            title: "Quick Look",
            keys: QUICK_LOOK_KEYS,
            hides: false,
            run: () => handlers.quickLook?.(app.path),
          },
        ]
      : []),
    { title: "Copy path", keys: "⌘ ⇧ C", hides: false, run: () => handlers.copyPath(app.path) },
  ];
}

export function matchesInOrder(query: string, title: string): boolean {
  const wanted = query.trim().toLowerCase();
  const text = title.toLowerCase();
  let found = 0;
  for (let at = 0; at < text.length && found < wanted.length; at++) {
    if (text[at] === wanted[found]) {
      found++;
    }
  }
  return found === wanted.length;
}

export function filterActions(actions: Action[], query: string): Action[] {
  return actions.filter((action) => matchesInOrder(query, action.about ?? action.title));
}

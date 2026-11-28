export type Keystroke = Pick<KeyboardEvent, "key"> &
  Partial<
    Pick<
      KeyboardEvent,
      "code" | "keyCode" | "isComposing" | "repeat" | "metaKey" | "ctrlKey" | "altKey" | "shiftKey"
    >
  >;

export type PanelView = {
  menu: boolean;
  pressing: boolean;
  search: string;
  held: string;
  answer?: boolean;
  streaming?: boolean;
  settings?: boolean;
  clipboard?: boolean;
  pulls?: boolean;
  slack?: boolean;
  vim?: boolean;
  normal?: boolean;
  waiting?: boolean;
  movable?: boolean;
  early?: boolean;
};

export type PanelActions = {
  hide: () => void;
  page: (direction: 1 | -1, half?: boolean) => void;
  move: (step: 1 | -1) => void;
  open: () => void;
  cancel: () => void;
  clear: () => void;
  toggleMenu: () => void;
  closeMenu: () => void;
  shortcut: (keys: string) => boolean;
  ask: () => void;
  stop: () => void;
  back: () => void;
  removeTag: () => void;
  normal: () => void;
  insert: (select: boolean, later?: boolean) => void;
  wait: (waiting: boolean) => void;
  line: (step: 1 | -1) => void;
  edge: (last: boolean) => void;
  tab: (step: 1 | -1) => void;
  edit: (event: Keystroke) => boolean;
};

export function shortcutOf(event: Keystroke): string {
  return [
    event.ctrlKey === true && "⌃",
    event.altKey === true && "⌥",
    event.metaKey === true && "⌘",
    event.shiftKey === true && "⇧",
    event.key === "Enter" ? "↵" : event.key === "Backspace" ? "⌫" : event.key.toUpperCase(),
  ]
    .filter(Boolean)
    .join(" ");
}

const inside = (view: PanelView) =>
  view.answer === true ||
  view.settings === true ||
  view.clipboard === true ||
  view.pulls === true ||
  view.slack === true;

export const COMPOSING = 229;
const MODIFIERS = new Set(["Shift", "Control", "Alt", "Meta", "CapsLock", "Fn", "OS"]);
const TO_INSERT = new Set(["i", "a", "/"]);
const ONE_SHOT = new Set([...TO_INSERT, "g"]);

const plainControl = (event: Keystroke) =>
  event.ctrlKey === true && event.metaKey !== true && event.altKey !== true;

const vimEscape = (event: Keystroke, view: PanelView) =>
  view.vim === true && plainControl(event) && event.key === "[";

const vimNormal = (view: PanelView) => view.vim === true && view.normal === true && !view.menu;

export function vimLetter(event: Keystroke): string {
  const position = /^Key([A-Z])$/.exec(event.code ?? "")?.[1];
  const latin = /^[a-z]$/i.test(event.key);
  const letter = latin ? event.key : [...event.key].length === 1 ? position : undefined;
  if (letter === undefined) {
    return event.key;
  }
  return event.shiftKey === true ? letter.toUpperCase() : letter.toLowerCase();
}

export function stepsBack(event: Keystroke, view: PanelView): boolean {
  return (
    event.key === "Escape" ||
    vimEscape(event, view) ||
    (event.key === "Backspace" && (view.menu || inside(view)) && view.search === "")
  );
}

export function switchesToInsert(event: Keystroke, view: PanelView): boolean {
  return (
    vimNormal(view) &&
    event.metaKey !== true &&
    event.ctrlKey !== true &&
    event.altKey !== true &&
    TO_INSERT.has(vimLetter(event))
  );
}

const CONTROL_STEPS: Record<string, 1 | -1> = { j: 1, n: 1, k: -1, p: -1 };

function controlStep(event: Keystroke): 1 | -1 | undefined {
  if (!plainControl(event)) {
    return undefined;
  }
  return CONTROL_STEPS[event.key.toLowerCase()];
}

const VIM_PAGES: Record<string, 1 | -1> = { d: 1, u: -1 };

function vimPage(event: Keystroke, view: PanelView): 1 | -1 | undefined {
  if (view.vim !== true || view.menu || !plainControl(event)) {
    return undefined;
  }
  return VIM_PAGES[event.key.toLowerCase()];
}

function stepBackInMenu(view: PanelView, actions: PanelActions): void {
  if (view.search !== "") {
    actions.clear();
  } else {
    actions.closeMenu();
  }
}

function stepBack(view: PanelView, actions: PanelActions): void {
  if (view.search !== "") {
    actions.clear();
  } else if (inside(view)) {
    actions.back();
  } else {
    actions.hide();
  }
}

function escape(view: PanelView, actions: PanelActions): void {
  if (view.pressing) {
    actions.cancel();
  } else if (view.menu) {
    stepBackInMenu(view, actions);
  } else if (view.streaming === true) {
    actions.stop();
  } else if (view.waiting === true) {
    return;
  } else if (view.vim !== true || view.normal === true) {
    stepBack(view, actions);
  } else if (!inside(view) && view.search === "") {
    actions.hide();
  } else if (view.movable !== true || view.early === true) {
    stepBack(view, actions);
  } else {
    actions.normal();
  }
}

function afterG(letter: string, view: PanelView, actions: PanelActions): boolean {
  if (letter === "g") {
    actions.edge(false);
    return true;
  }
  if (view.pulls === true && (letter === "t" || letter === "T")) {
    actions.tab(letter === "t" ? 1 : -1);
    return true;
  }
  return false;
}

function normalKey(event: Keystroke, view: PanelView, actions: PanelActions): boolean | undefined {
  const typesText = [...event.key].length === 1 || event.key === "Dead";
  if (event.altKey === true) {
    return typesText ? true : undefined;
  }
  const letter = vimLetter(event);
  if (view.waiting === true && (afterG(letter, view, actions) || typesText)) {
    return true;
  }
  switch (letter) {
    case "j":
    case "k":
      actions.line(letter === "j" ? 1 : -1);
      return true;
    case "G":
      actions.edge(true);
      return true;
    case "g":
      actions.wait(true);
      return true;
    case "i":
    case "a":
    case "/":
      actions.insert(letter === "/", event.keyCode === COMPOSING);
      return true;
    default:
      if (event.key === "Backspace" && view.search !== "") {
        return true;
      }
      return typesText ? true : undefined;
  }
}

export function handleKey(event: Keystroke, view: PanelView, actions: PanelActions): boolean {
  if (view.vim === true && event.repeat === true && event.key === view.held) {
    return true;
  }
  if (
    event.isComposing === true ||
    (view.vim === true && !vimNormal(view) && event.keyCode === COMPOSING)
  ) {
    return false;
  }
  if (event.repeat === true && (event.key === view.held || stepsBack(event, view))) {
    return true;
  }
  if (event.repeat === true && vimNormal(view) && ONE_SHOT.has(vimLetter(event))) {
    return true;
  }
  if (view.vim === true && MODIFIERS.has(event.key)) {
    return false;
  }
  const waiting = view.waiting === true;
  if (waiting) {
    actions.wait(false);
  }
  if (view.answer === true && vimNormal(view) && !waiting && actions.edit(event)) {
    return true;
  }
  if (event.metaKey === true && event.key.toLowerCase() === "k") {
    actions.toggleMenu();
    return true;
  }
  const paging = vimPage(event, view);
  if (paging !== undefined) {
    actions.page(paging, true);
    return true;
  }
  if (vimEscape(event, view)) {
    escape(view, actions);
    return true;
  }
  const step = controlStep(event);
  if (step !== undefined) {
    actions.move(step);
    return true;
  }
  if (event.metaKey === true || event.ctrlKey === true) {
    if (vimNormal(view) && event.metaKey === true && event.key.toLowerCase() === "v") {
      actions.insert(false);
    }
    return actions.shortcut(shortcutOf(event));
  }
  if (vimNormal(view)) {
    const handled = normalKey(event, view, actions);
    if (handled !== undefined) {
      return handled;
    }
  }
  switch (event.key) {
    case "Escape":
      escape(view, actions);
      return true;
    case "Backspace":
      if (view.search !== "") {
        return false;
      }
      if (view.menu) {
        actions.closeMenu();
        return true;
      }
      if (view.answer === true) {
        actions.removeTag();
        return true;
      }
      if (
        view.settings === true ||
        view.clipboard === true ||
        view.pulls === true ||
        view.slack === true
      ) {
        actions.back();
        return true;
      }
      return false;
    case "Tab":
      if (view.menu) {
        return true;
      }
      if (view.pulls === true) {
        actions.tab(event.shiftKey === true ? -1 : 1);
      } else if (!inside(view)) {
        actions.ask();
      }
      return true;
    case "PageDown":
    case "PageUp":
      if (view.menu) {
        return false;
      }
      actions.page(event.key === "PageDown" ? 1 : -1);
      return true;
    case "ArrowDown":
      actions.move(1);
      return true;
    case "ArrowUp":
      actions.move(-1);
      return true;
    case "Enter":
      actions.open();
      return true;
    default:
      return false;
  }
}

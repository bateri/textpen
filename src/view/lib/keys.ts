/**
 * Keys as textpen names them — `Mod-k`, `Shift-Mod-k`, `Ctrl-w`, `F1` — matched against
 * the browser's events and shown as the platform shows them. `Mod` is ⌘ on a Mac and
 * Ctrl elsewhere: the view runs on the person's machine, whatever the program's is.
 *
 * A letter or a digit is also matched by where it sits on the keyboard when ⌘ or ⌃ is
 * held, since a layout may give the key another character. ⌥ alone is left to type:
 * on a Mac it is how many layouts write @, #, [ or { (⌥Q is @ on a Turkish one), so
 * textpen binds no ⌥-letter key.
 */

export const isMac =
  typeof navigator !== "undefined" &&
  /Mac|iPhone|iPad/.test(navigator.platform || navigator.userAgent);

interface Chord {
  ctrl: boolean;
  alt: boolean;
  shift: boolean;
  meta: boolean;
  key: string;
}

const cache = new Map<string, Chord>();

function parse(spec: string): Chord {
  const known = cache.get(spec);
  if (known) return known;
  const parts = spec.split("-");
  // `Mod--` is ⌘ and the minus key.
  let key = parts.pop()!;
  if (key === "" && parts.at(-1) === "") {
    parts.pop();
    key = "-";
  }
  const chord: Chord = {
    ctrl: false,
    alt: false,
    shift: false,
    meta: false,
    key: key.toLowerCase(),
  };
  for (const part of parts) {
    if (part === "Mod") chord[isMac ? "meta" : "ctrl"] = true;
    else if (part === "Ctrl") chord.ctrl = true;
    else if (part === "Alt") chord.alt = true;
    else if (part === "Shift") chord.shift = true;
    else if (part === "Cmd" || part === "Meta") chord.meta = true;
  }
  cache.set(spec, chord);
  return chord;
}

/** Keys that are typed with ⇧ on most layouts, so that ⇧ does not count against them. */
const SHIFTED = new Set(["+", "?", "`", "~"]);

function keyName(event: KeyboardEvent): string {
  const key = event.key.toLowerCase();
  return key === " " ? "space" : key;
}

function codeName(event: KeyboardEvent): string | undefined {
  const code = event.code;
  if (code.startsWith("Key")) return code.slice(3).toLowerCase();
  if (code.startsWith("Digit")) return code.slice(5);
  if (code === "Slash") return "/";
  if (code === "Backquote") return "`";
  if (code === "Equal") return "=";
  if (code === "Minus") return "-";
  if (code === "Comma") return ",";
  if (code === "Period") return ".";
  return undefined;
}

export function matches(event: KeyboardEvent, spec: string): boolean {
  const chord = parse(spec);
  if (event.ctrlKey !== chord.ctrl || event.altKey !== chord.alt || event.metaKey !== chord.meta) {
    return false;
  }
  const key = keyName(event);
  if (chord.key === "=" && (key === "=" || key === "+")) return true;
  if (event.shiftKey !== chord.shift && !SHIFTED.has(chord.key)) return false;
  if (key === chord.key) return true;
  // ⌥ alone types a character on a Mac (CodeMirror reads it the same way); with ⌘ or ⌃
  // it does not, and the key is read by where it sits.
  const typed = isMac && event.altKey && !event.metaKey && !event.ctrlKey;
  return (
    (event.metaKey || event.ctrlKey || event.altKey) && !typed && codeName(event) === chord.key
  );
}

const MAC_GLYPHS: Record<string, string> = {
  Mod: "⌘",
  Cmd: "⌘",
  Ctrl: "⌃",
  Alt: "⌥",
  Shift: "⇧",
};

const OTHER_NAMES: Record<string, string> = {
  Mod: "Ctrl",
  Ctrl: "Ctrl",
  Alt: "Alt",
  Shift: "Shift",
  Cmd: "Super",
};

const KEY_NAMES: Record<string, string> = {
  arrowup: "↑",
  arrowdown: "↓",
  arrowleft: "←",
  arrowright: "→",
  enter: "⏎",
  escape: "Esc",
  backspace: "⌫",
  delete: "⌦",
  tab: "⇥",
  space: "Space",
  pageup: "PgUp",
  pagedown: "PgDn",
  home: "Home",
  end: "End",
  "=": "+",
};

/** The parts of a key as the platform writes them: `["⌃", "W"]`, `["Ctrl", "W"]`. */
export function glyphs(spec: string): string[] {
  const parts = spec.split("-");
  let key = parts.pop()!;
  if (key === "" && parts.at(-1) === "") {
    parts.pop();
    key = "-";
  }
  // The order a Mac writes them in: ⌃ ⌥ ⇧ ⌘.
  const order = isMac
    ? ["Ctrl", "Alt", "Shift", "Mod", "Cmd"]
    : ["Mod", "Ctrl", "Alt", "Shift", "Cmd"];
  const mods = order
    .filter((m) => parts.includes(m))
    .map((m) => (isMac ? MAC_GLYPHS[m]! : OTHER_NAMES[m]!));
  const name = KEY_NAMES[key.toLowerCase()] ?? (key.length === 1 ? key.toUpperCase() : key);
  return [...mods, name];
}

/** A key as one string: `⌃W` on a Mac, `Ctrl+W` elsewhere. */
export function label(spec: string): string {
  return glyphs(spec).join(isMac ? "" : "+");
}

/** The element the keys go to is one that types text. */
export function typing(target: EventTarget | null): boolean {
  return (
    target instanceof HTMLInputElement ||
    target instanceof HTMLTextAreaElement ||
    (target instanceof HTMLElement && target.isContentEditable)
  );
}

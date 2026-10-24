/**
 * The page's side of pencere's one protocol: what a web view says to its program and to
 * the host that shows it, and nothing of textpen. The page asks the program with `call`,
 * hears a topic with `subscribe`, is told about the terminal in `context`, and asks the
 * host to `close` it or to open a link. The host passes the page's `call`, `subscribe` and
 * `cancel` to the program, unread, and the program's `response` and `push` back.
 *
 * This is the module `@pencere/sdk/page` will be: when the SDK ships it, the import is all
 * that changes.
 */

/** The roles of the terminal's theme, each a colour as `#rrggbb` (specification 11.2). */
export interface Colors {
  background: string;
  foreground: string;
  dim: string;
  accent: string;
  border: string;
  success: string;
  error: string;
  warning: string;
  info: string;
}

/** What the host tells the page about the terminal (specification 9.5). */
export interface Context {
  /** `dark` when the pane's background is darker than its foreground. */
  mode: "dark" | "light";
  colors: Colors;
  /** The terminal's monospace font as a CSS `font-family` list; left out when the host does not know it. */
  font?: string;
  /** The pane's content area in CSS pixels. */
  size: { width: number; height: number };
  locale?: string;
  timeZone?: string;
  reduceMotion: boolean;
}

/** An answer of the program that is an error: a short `code` to act on, and words for the person. */
export class CallError extends Error {
  override name = "CallError";
  readonly code: string;

  constructor(code: string, message?: string) {
    super(message ?? code);
    this.code = code;
  }
}

interface Pending {
  resolve: (value: unknown) => void;
  reject: (error: CallError) => void;
}

/** The host: whatever embeds the page. A page on its own has none, and nothing to ask. */
const host: Window | null = window.parent !== window ? window.parent : null;

let nextId = 1;
const calls = new Map<number, Pending>();
const subscriptions = new Map<number, (value: unknown) => void>();
const listeners = new Set<(context: Context) => void>();
let current: Context | undefined;

function post(message: object): void {
  host?.postMessage(message, "*");
}

/** Asks the program to do something and answers once: with its result, or rejects with its error. */
export function call<T = unknown>(method: string, params?: unknown): Promise<T> {
  if (host === null) {
    return Promise.reject(new CallError("no-host", "This page is not shown by a terminal"));
  }
  return new Promise<T>((resolve, reject) => {
    const id = nextId++;
    calls.set(id, { resolve: resolve as (value: unknown) => void, reject });
    post({ call: params === undefined ? { id, method } : { id, method, params } });
  });
}

/**
 * Hears a topic of the program as it changes, until the returned function ends it. A
 * program that refuses the subscription ends it with an error, which is logged: nothing
 * more is pushed.
 */
export function subscribe<T = unknown>(
  topic: string,
  params: unknown,
  onPush: (value: T) => void,
): () => void {
  const id = nextId++;
  subscriptions.set(id, onPush as (value: unknown) => void);
  post({ subscribe: params === undefined ? { id, topic } : { id, topic, params } });
  return () => {
    if (subscriptions.delete(id)) post({ cancel: { id } });
  };
}

/**
 * Calls `listener` with the terminal's context now, if the host has told it, and each time
 * it changes: the pane is resized, the theme changes. Returns the function that stops it.
 */
export function onContext(listener: (context: Context) => void): () => void {
  listeners.add(listener);
  if (current !== undefined) listener(current);
  return () => {
    listeners.delete(listener);
  };
}

/** The terminal's context as the host last told it; `undefined` before it has. */
export function context(): Context | undefined {
  return current;
}

/** Asks the host to close the view. The program is told and decides what it does. */
export function close(): void {
  post({ close: {} });
}

/**
 * Asks the host to open a link in the person's browser. Only `http:` and `https:` links
 * are opened, and the person may be asked first; the host does not say whether they were.
 */
export function openLink(url: string): void {
  post({ link: { url } });
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function understood(body: Record<string, unknown>): Context | undefined {
  const { mode, colors, font, size, locale, timeZone, reduceMotion } = body;
  if ((mode !== "dark" && mode !== "light") || !isRecord(colors) || !isRecord(size)) {
    return undefined;
  }
  const text = (value: unknown) => (typeof value === "string" ? value : undefined);
  return {
    mode,
    colors: colors as unknown as Colors,
    ...(text(font) !== undefined ? { font: text(font)! } : {}),
    size: {
      width: typeof size.width === "number" ? size.width : 0,
      height: typeof size.height === "number" ? size.height : 0,
    },
    ...(text(locale) !== undefined ? { locale: text(locale)! } : {}),
    ...(text(timeZone) !== undefined ? { timeZone: text(timeZone)! } : {}),
    reduceMotion: reduceMotion === true,
  };
}

function received(data: unknown): void {
  if (!isRecord(data)) return;
  const members = Object.keys(data);
  const [name] = members;
  const body = name === undefined ? undefined : data[name];
  if (members.length !== 1 || !isRecord(body)) return;
  switch (name) {
    case "context": {
      const next = understood(body);
      if (next === undefined) return;
      current = next;
      for (const listener of [...listeners]) listener(next);
      return;
    }
    case "response": {
      const id = body.id;
      if (typeof id !== "number") return;
      const waiting = calls.get(id);
      if (waiting !== undefined) {
        calls.delete(id);
        const error = body.error;
        if (isRecord(error)) {
          waiting.reject(
            new CallError(
              typeof error.code === "string" ? error.code : "failed",
              typeof error.message === "string" ? error.message : undefined,
            ),
          );
        } else {
          waiting.resolve(body.result);
        }
      } else if (subscriptions.delete(id)) {
        const error = isRecord(body.error) ? body.error : {};
        console.error(`The program ended a subscription: ${String(error.message ?? error.code)}`);
      }
      return;
    }
    case "push": {
      const id = body.id;
      if (typeof id === "number") subscriptions.get(id)?.(body.value);
      return;
    }
    // `progress` is how far a slow call has got; no call of this page is slow enough to report.
  }
}

window.addEventListener("message", (event) => {
  if (host !== null && event.source === host) received(event.data);
});

// The host sends `context` once the page has loaded, but a page cannot know it was listening
// in time, so it asks.
post({ context: {} });

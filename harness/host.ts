/**
 * A stand-in terminal for working on the editor in a browser: it frames the view, tells it
 * the terminal's `context` the way a pencere terminal does (the theme from a terminal
 * palette, the pane's size), handles the page's `close` and `link`, and passes its `call`,
 * `subscribe` and `cancel` to the real program, which the harness's Vite plugin runs, and
 * the program's `response`, `push` and `progress` back, unread.
 */

type Rgb = [number, number, number];

interface Palette {
  name: string;
  background: string;
  foreground: string;
  dim?: string;
  accent?: string;
  success?: string;
  error?: string;
  warning?: string;
  info?: string;
  font?: string;
}

const PALETTES: Palette[] = [
  {
    name: "bateri",
    background: "#000000",
    foreground: "#d8d9dd",
    accent: "#7a9cc6",
    success: "#8bb58b",
    error: "#d16d6a",
    warning: "#d6b16a",
    info: "#79b3b3",
  },
  {
    name: "bateri light",
    background: "#f5f6f8",
    foreground: "#24262c",
    dim: "#696b70",
    accent: "#3d6aa8",
    success: "#3b7a3b",
    error: "#b5423d",
    warning: "#8f6a00",
    info: "#23787f",
  },
  {
    name: "Tokyo Night",
    background: "#1a1b26",
    foreground: "#c0caf5",
    dim: "#787c99",
    accent: "#7aa2f7",
    success: "#9ece6a",
    error: "#f7768e",
    warning: "#e0af68",
    info: "#7dcfff",
  },
  {
    name: "Gruvbox",
    background: "#282828",
    foreground: "#ebdbb2",
    dim: "#a89984",
    accent: "#83a598",
    success: "#b8bb26",
    error: "#fb4934",
    warning: "#fabd2f",
    info: "#8ec07c",
  },
  {
    name: "Dracula",
    background: "#282a36",
    foreground: "#f8f8f2",
    dim: "#9ea8c7",
    accent: "#bd93f9",
    success: "#50fa7b",
    error: "#ff5555",
    warning: "#f1fa8c",
    info: "#8be9fd",
  },
  {
    name: "Solarized Light",
    background: "#fdf6e3",
    foreground: "#586e75",
    dim: "#93a1a1",
    accent: "#268bd2",
    success: "#859900",
    error: "#dc322f",
    warning: "#b58900",
    info: "#2aa198",
  },
  {
    name: "Catppuccin Mocha",
    background: "#1e1e2e",
    foreground: "#cdd6f4",
    dim: "#9399b2",
    accent: "#cba6f7",
    success: "#a6e3a1",
    error: "#f38ba8",
    warning: "#f9e2af",
    info: "#89dceb",
  },
];

const SIZES: { name: string; width: number; height: number }[] = [
  { name: "120×36", width: 980, height: 600 },
  { name: "80×24", width: 660, height: 410 },
  { name: "160×48", width: 1300, height: 800 },
  { name: "200×56", width: 1600, height: 930 },
];

function parse(hex: string): Rgb {
  const h = hex.replace("#", "");
  return [0, 2, 4].map((i) => parseInt(h.slice(i, i + 2), 16)) as Rgb;
}

function css([r, g, b]: Rgb): string {
  return `#${[r, g, b].map((c) => c.toString(16).padStart(2, "0")).join("")}`;
}

function toward(a: Rgb, b: Rgb, amount: number): Rgb {
  return a.map((c, i) => Math.round(c + (b[i]! - c) * amount)) as Rgb;
}

function luminance([r, g, b]: Rgb): number {
  const lin = (c: number) => {
    const s = c / 255;
    return s <= 0.04045 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b);
}

/**
 * The nine roles a terminal tells a page (specification 11.2): what the palette lacks is made
 * up as the specification says.
 */
function colors(p: Palette): Record<string, string> {
  const bg = parse(p.background);
  const fg = parse(p.foreground);
  return {
    background: p.background,
    foreground: p.foreground,
    dim: p.dim ?? css(toward(fg, bg, 1 / 3)),
    accent: p.accent ?? p.info ?? "#58a6ff",
    border: css(toward(bg, fg, 0.2)),
    success: p.success ?? "#3fb950",
    error: p.error ?? "#f85149",
    warning: p.warning ?? "#d29922",
    info: p.info ?? "#58a6ff",
  };
}

function themeOf(p: Palette): "dark" | "light" {
  return luminance(parse(p.background)) < luminance(parse(p.foreground)) ? "dark" : "light";
}

const frame = document.getElementById("view") as HTMLIFrameElement;
const themeSelect = document.getElementById("theme") as HTMLSelectElement;
const sizeSelect = document.getElementById("size") as HTMLSelectElement;
const log = document.getElementById("log")!;
const strip = document.getElementById("strip")!;
const params = new URLSearchParams(location.search);

for (const p of PALETTES) themeSelect.append(new Option(p.name, p.name));
for (const s of SIZES) sizeSelect.append(new Option(s.name, s.name));
sizeSelect.append(new Option("fit the window", "fit"));
window.addEventListener("resize", () => {
  if (sizeSelect.value === "fit" || bare) changed();
});
themeSelect.value = params.get("theme") ?? PALETTES[0]!.name;
sizeSelect.value = params.get("size") ?? SIZES[0]!.name;

const palette = () => PALETTES.find((p) => p.name === themeSelect.value) ?? PALETTES[0]!;
/** The pane's size: one of the terminal sizes, or what the window leaves (`fit`). */
// `?bare` shows the pane alone, filling the window: for screenshots.
const bare = params.has("bare");
if (bare) document.body.classList.add("bare");

const size = () => {
  if (bare) return { name: "bare", width: window.innerWidth, height: window.innerHeight - 24 };
  if (sizeSelect.value === "fit") {
    return {
      name: "fit",
      width: window.innerWidth - 32,
      height: window.innerHeight - 36 - 32 - 24,
    };
  }
  return SIZES.find((s) => s.name === sizeSelect.value) ?? SIZES[0]!;
};

function place(): void {
  const s = size();
  frame.style.width = `${s.width}px`;
  frame.style.height = `${s.height}px`;
  const p = palette();
  strip.style.background = css(toward(parse(p.background), parse(p.foreground), 0.08));
  strip.style.color = p.dim ?? css(toward(parse(p.foreground), parse(p.background), 1 / 3));
}

/** What a terminal tells a page (specification 9.5). */
function context() {
  const p = palette();
  const s = size();
  return {
    mode: themeOf(p),
    colors: colors(p),
    font: p.font ?? '"JetBrains Mono", "SF Mono", Menlo, monospace',
    size: { width: s.width, height: s.height },
    locale: navigator.language,
    timeZone: Intl.DateTimeFormat().resolvedOptions().timeZone,
    reduceMotion: false,
  };
}

function post(message: unknown): void {
  frame.contentWindow?.postMessage(message, "*");
}

function note(text: string): void {
  log.textContent = text;
}

/** The program's words for the page, from Vite's socket: `response`, `push` and `progress`. */
const socket = import.meta.hot;
socket?.on("textpen:message", (message: unknown) => post(message));

/** A call of this page's own to the program, answered once: for the state a screenshot starts in. */
async function callProgram(method: string, params: unknown): Promise<unknown> {
  const response = await fetch("/__harness/call", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ method, params: params ?? {} }),
  });
  if (!response.ok) throw new Error(await response.text());
  return response.json();
}

/**
 * What the page says. A message of one member named `context`, `close` or `link` is the
 * host's own; any other is the program's, passed on whole (specification 9.4).
 */
window.addEventListener("message", (event) => {
  if (event.source !== frame.contentWindow) return;
  const message = event.data as Record<string, unknown> | null;
  if (typeof message !== "object" || message === null || Array.isArray(message)) return;
  const members = Object.keys(message);
  const name = members.length === 1 ? members[0] : undefined;
  if (name === "context") {
    post({ context: context() });
  } else if (name === "close") {
    // The program would be told `closed` and end; the stand-in leaves the pane empty.
    note("the page asked to be closed");
    frame.src = "about:blank";
  } else if (name === "link") {
    const url = String((message.link as { url?: unknown } | null)?.url ?? "");
    if (/^https?:\/\/[\x21-\x7e]+$/.test(url)) window.open(url, "_blank", "noopener");
    else note(`link refused: ${url.slice(0, 60)}`);
  } else {
    socket?.send("textpen:message", message);
  }
});

function changed(): void {
  place();
  post({ context: context() });
  const url = new URL(location.href);
  url.searchParams.set("theme", themeSelect.value);
  url.searchParams.set("size", sizeSelect.value);
  history.replaceState(null, "", url);
}

themeSelect.addEventListener("change", changed);
sizeSelect.addEventListener("change", changed);
// `?built` shows the view as `npm run build` wrote it, the files a terminal is sent.
const page = params.has("built") ? "/dist/view/index.html" : "/index.html";

/** Starts the view: what the last one had asked of the program is over with it. */
function start(): void {
  socket?.send("textpen:reset", {});
  frame.src = `${page}?${Date.now()}`;
  note("view started");
}
document.getElementById("reload")!.addEventListener("click", start);

// A page that has loaded is told the context, as a terminal does.
frame.addEventListener("load", () => {
  frame.focus();
  if (frame.src.endsWith("about:blank")) return;
  post({ context: context() });
});
place();
// `?ui=<json>` starts the view in that state, as the program would keep it: for screenshots.
const ui = params.get("ui");
void (
  ui
    ? callProgram("ui", {
        ui: {
          tabs: [],
          active: null,
          expanded: [],
          places: {},
          recent: [],
          preview: [],
          ...JSON.parse(ui),
        },
      })
    : Promise.resolve()
).then(start);

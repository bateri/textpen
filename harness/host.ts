/**
 * A stand-in MCP Apps host for working on the editor in a browser: it frames the view,
 * answers its `ui/` messages the way a pencere terminal does (the theme from a terminal
 * palette, the pane's size, `pencere/pane`), and passes its tool calls to the real
 * server, which the harness's Vite plugin runs over stdio.
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

/** The variables a pencere host sends for a terminal theme: the crate's `Theme::variables`, in TypeScript. */
function variables(p: Palette): Record<string, string> {
  const bg = parse(p.background);
  const fg = parse(p.foreground);
  const info = parse(p.info ?? "#58a6ff");
  const roles: [string, Rgb][] = [
    ["info", info],
    ["danger", parse(p.error ?? "#f85149")],
    ["success", parse(p.success ?? "#3fb950")],
    ["warning", parse(p.warning ?? "#d29922")],
  ];
  const dim = p.dim ? parse(p.dim) : toward(fg, bg, 1 / 3);
  const accent = p.accent ? parse(p.accent) : info;
  const border = toward(bg, fg, 0.2);
  const v: Record<string, string> = {
    "--color-background-primary": css(bg),
    "--color-background-secondary": css(toward(bg, fg, 0.06)),
    "--color-background-tertiary": css(toward(bg, fg, 0.12)),
    "--color-background-inverse": css(fg),
    "--color-background-ghost": "transparent",
    "--color-background-disabled": css(toward(bg, fg, 0.03)),
    "--color-text-primary": css(fg),
    "--color-text-secondary": css(dim),
    "--color-text-tertiary": css(toward(dim, bg, 0.35)),
    "--color-text-inverse": css(bg),
    "--color-text-ghost": css(dim),
    "--color-text-disabled": css(toward(fg, bg, 0.55)),
    "--color-border-primary": css(border),
    "--color-border-secondary": css(toward(border, bg, 0.4)),
    "--color-border-tertiary": css(toward(border, bg, 0.7)),
    "--color-border-inverse": css(fg),
    "--color-border-ghost": "transparent",
    "--color-border-disabled": css(toward(border, bg, 0.5)),
    "--color-ring-primary": css(accent),
    "--color-ring-secondary": css(border),
    "--color-ring-inverse": css(fg),
    "--font-mono": p.font ?? '"JetBrains Mono", "SF Mono", Menlo, monospace',
  };
  for (const [name, role] of roles) {
    v[`--color-background-${name}`] = css(toward(bg, role, 0.18));
    v[`--color-text-${name}`] = css(role);
    v[`--color-border-${name}`] = css(role);
    v[`--color-ring-${name}`] = css(role);
  }
  return v;
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

let tool: unknown;

function place(): void {
  const s = size();
  frame.style.width = `${s.width}px`;
  frame.style.height = `${s.height}px`;
  const p = palette();
  strip.style.background = css(toward(parse(p.background), parse(p.foreground), 0.08));
  strip.style.color = p.dim ?? css(toward(parse(p.foreground), parse(p.background), 1 / 3));
}

function context() {
  const p = palette();
  const s = size();
  return {
    theme: themeOf(p),
    styles: { variables: variables(p) },
    containerDimensions: { width: s.width, height: s.height },
  };
}

function post(message: unknown): void {
  frame.contentWindow?.postMessage(message, "*");
}

function note(text: string): void {
  log.textContent = text;
}

async function callTool(name: string, args: unknown): Promise<unknown> {
  const response = await fetch("/__harness/call", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ name, arguments: args ?? {} }),
  });
  if (!response.ok) throw new Error(await response.text());
  return response.json();
}

window.addEventListener("message", async (event) => {
  if (event.source !== frame.contentWindow) return;
  const message = event.data as {
    jsonrpc?: string;
    id?: number;
    method?: string;
    params?: Record<string, unknown>;
  };
  if (message?.jsonrpc !== "2.0" || !message.method) return;
  const answer = (result: unknown) => post({ jsonrpc: "2.0", id: message.id, result });
  const fail = (code: number, text: string) =>
    post({ jsonrpc: "2.0", id: message.id, error: { code, message: text } });
  switch (message.method) {
    case "ui/initialize": {
      const listed = (await fetch("/__harness/tools").then((r) => r.json())) as {
        tools: { name: string }[];
      };
      tool = listed.tools.find((t) => t.name === "show_editor");
      answer({
        protocolVersion: "2026-01-26",
        hostInfo: { name: "textpen-harness", version: "0.0.0" },
        hostCapabilities: {
          experimental: { "pencere/pane": {} },
          openLinks: {},
          serverTools: {},
          serverResources: {},
          logging: {},
          sandbox: {},
        },
        hostContext: {
          toolInfo: { tool },
          ...context(),
          displayMode: "inline",
          availableDisplayModes: ["inline"],
          locale: navigator.language,
          timeZone: Intl.DateTimeFormat().resolvedOptions().timeZone,
          userAgent: "textpen-harness/0.0.0",
          platform: "desktop",
        },
      });
      return;
    }
    case "ui/notifications/initialized": {
      post({ jsonrpc: "2.0", method: "ui/notifications/tool-input", params: { arguments: {} } });
      const result = await callTool("show_editor", {});
      post({ jsonrpc: "2.0", method: "ui/notifications/tool-result", params: result });
      note("view started");
      return;
    }
    case "tools/call": {
      const params = message.params as { name: string; arguments?: unknown };
      const started = performance.now();
      try {
        const result = await callTool(params.name, params.arguments);
        answer(result);
        note(`${params.name} ${Math.round(performance.now() - started)}ms`);
      } catch (error) {
        fail(-32603, String(error));
      }
      return;
    }
    case "ui/open-link": {
      window.open(String(message.params?.url ?? ""), "_blank");
      answer({});
      return;
    }
    case "ui/request-display-mode":
      answer({ mode: "inline" });
      return;
    case "ui/message":
    case "ui/update-model-context":
      fail(-32000, "A terminal has no conversation");
      return;
    case "ping":
      answer({});
      return;
    default:
      if (message.id !== undefined) fail(-32601, `${message.method} is not a method of this host`);
  }
});

function changed(): void {
  place();
  post({ jsonrpc: "2.0", method: "ui/notifications/host-context-changed", params: context() });
  const url = new URL(location.href);
  url.searchParams.set("theme", themeSelect.value);
  url.searchParams.set("size", sizeSelect.value);
  history.replaceState(null, "", url);
}

themeSelect.addEventListener("change", changed);
sizeSelect.addEventListener("change", changed);
// `?built` shows the view as `npm run build` wrote it, the files a terminal is sent.
const page = params.has("built") ? "/dist/view/index.html" : "/index.html";
document.getElementById("reload")!.addEventListener("click", () => {
  frame.src = `${page}?${Date.now()}`;
});

frame.addEventListener("load", () => frame.focus());
place();
// `?ui=<json>` starts the view in that state, as the program would keep it: for screenshots.
const ui = params.get("ui");
void (
  ui
    ? callTool("textpen_poll", {
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
).then(() => {
  frame.src = page;
});

/**
 * What textpen remembers between runs, where the XDG directories say: the settings
 * (`~/.config/textpen/settings.json`) and, for each project, the files that were open
 * and where (`~/.local/state/textpen/projects/`). The view's own storage does not
 * last: a terminal gives every view a new origin.
 */

import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";

import { keyOf } from "./project.js";
import { DEFAULT_SETTINGS, EMPTY_UI } from "./types.js";
import type { Settings, UiState } from "./types.js";

export function configDirectory(): string {
  return join(process.env.XDG_CONFIG_HOME || join(homedir(), ".config"), "textpen");
}

export function stateDirectory(): string {
  return join(process.env.XDG_STATE_HOME || join(homedir(), ".local", "state"), "textpen");
}

export class Store {
  readonly config = configDirectory();
  readonly state = stateDirectory();
  #uiTimer: ReturnType<typeof setTimeout> | undefined;
  #pendingUi: { root: string; ui: UiState } | undefined;

  settings(): Settings {
    const read = readJson<Partial<Settings>>(join(this.config, "settings.json"));
    return sanitize({ ...DEFAULT_SETTINGS, ...read });
  }

  saveSettings(settings: Settings): Settings {
    const clean = sanitize(settings);
    writeJson(join(this.config, "settings.json"), clean);
    return clean;
  }

  ui(root: string): UiState {
    const read = readJson<Partial<UiState> & { root?: string }>(this.#uiFile(root));
    if (read?.root !== root) return { ...EMPTY_UI };
    return {
      tabs: strings(read.tabs),
      active: typeof read.active === "string" ? read.active : null,
      expanded: strings(read.expanded),
      places: typeof read.places === "object" && read.places !== null ? read.places : {},
      recent: strings(read.recent).slice(0, 50),
      preview: strings(read.preview),
    };
  }

  /** Keeps the view's state for the next run, a moment after it stops changing. */
  saveUi(root: string, ui: UiState): void {
    this.#pendingUi = { root, ui };
    clearTimeout(this.#uiTimer);
    this.#uiTimer = setTimeout(() => this.flush(), 1000);
    this.#uiTimer.unref();
  }

  flush(): void {
    clearTimeout(this.#uiTimer);
    const pending = this.#pendingUi;
    this.#pendingUi = undefined;
    if (pending) writeJson(this.#uiFile(pending.root), { root: pending.root, ...pending.ui });
  }

  #uiFile(root: string): string {
    return join(this.state, "projects", `${keyOf(root)}.json`);
  }
}

function sanitize(settings: Settings): Settings {
  const number = (value: unknown, min: number, max: number, fallback: number) =>
    typeof value === "number" && Number.isFinite(value)
      ? Math.min(max, Math.max(min, Math.round(value)))
      : fallback;
  const flag = (value: unknown, fallback: boolean) =>
    typeof value === "boolean" ? value : fallback;
  return {
    vim: flag(settings.vim, DEFAULT_SETTINGS.vim),
    wrap: flag(settings.wrap, DEFAULT_SETTINGS.wrap),
    fontSize: number(settings.fontSize, 9, 28, DEFAULT_SETTINGS.fontSize),
    lineNumbers: ["on", "relative", "off"].includes(settings.lineNumbers)
      ? settings.lineNumbers
      : DEFAULT_SETTINGS.lineNumbers,
    tabSize: number(settings.tabSize, 1, 8, DEFAULT_SETTINGS.tabSize),
    insertSpaces: flag(settings.insertSpaces, DEFAULT_SETTINGS.insertSpaces),
    whitespace: flag(settings.whitespace, DEFAULT_SETTINGS.whitespace),
    autoSave: flag(settings.autoSave, DEFAULT_SETTINGS.autoSave),
    sidebar: flag(settings.sidebar, DEFAULT_SETTINGS.sidebar),
    sidebarWidth: number(settings.sidebarWidth, 160, 520, DEFAULT_SETTINGS.sidebarWidth),
  };
}

function strings(value: unknown): string[] {
  return Array.isArray(value) ? value.filter((v): v is string => typeof v === "string") : [];
}

function readJson<T>(path: string): T | undefined {
  try {
    return JSON.parse(readFileSync(path, "utf8")) as T;
  } catch {
    return undefined;
  }
}

function writeJson(path: string, value: unknown): void {
  try {
    mkdirSync(join(path, ".."), { recursive: true });
    writeFileSync(path, `${JSON.stringify(value, null, 2)}\n`);
  } catch {
    // A home that cannot be written to: textpen works, and forgets.
  }
}

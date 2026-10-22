/**
 * The view's state, in one store. The text of the files is not here: it lives in the
 * editor (one CodeMirror state per open file) and, as the source of truth, in the
 * program. What is here is what the rest of the screen draws.
 */

import { create } from "zustand";

import type {
  Boot,
  BufferInfo,
  FileIndex,
  GitInfo,
  Listing,
  Place,
  SearchResult,
  Settings,
  UiState,
  Unopenable,
} from "../../server/types";
import { DEFAULT_SETTINGS } from "../../server/types";

export type Layout = "wide" | "medium" | "narrow";

export interface Tab {
  path: string;
  status: "loading" | "ready" | "unopenable" | "error";
  info?: BufferInfo;
  unopenable?: Unopenable;
  error?: string;
}

export type PaletteMode = "files" | "commands" | "line" | "tabs" | "language" | "indent";

export interface DialogAction {
  id: string;
  label: string;
  primary?: boolean;
  danger?: boolean;
}

export interface Dialog {
  title: string;
  message?: string;
  /** A field to type into: the dialog asks for a name or a path. */
  input?: { value: string; placeholder?: string; select?: [number, number]; hint?: string };
  actions: DialogAction[];
  resolve: (result: { action: string; value: string } | null) => void;
}

export type MenuItem =
  | { separator: true }
  | { label: string; keys?: string; danger?: boolean; disabled?: boolean; run: () => void };

export type Overlay =
  | { kind: "palette"; mode: PaletteMode; query: string }
  | { kind: "menu"; at: { x: number; y: number }; items: MenuItem[] }
  | { kind: "dialog"; dialog: Dialog }
  | { kind: "help" }
  | { kind: "settings" }
  | null;

export interface Toast {
  id: number;
  tone: "info" | "ok" | "warn" | "bad";
  text: string;
  action?: { label: string; run: () => void };
}

export interface Cursor {
  line: number;
  column: number;
  selections: number;
  selected: number;
  lines: number;
}

export interface SearchState {
  query: string;
  regex: boolean;
  caseSensitive: boolean;
  wholeWord: boolean;
  running: boolean;
  result: SearchResult | null;
  error: string | null;
  /** The match the list has chosen: a file, and the index of a match in it. */
  chosen: { file: number; match: number } | null;
  collapsed: string[];
}

export interface State {
  boot: Boot | null;
  bootError: string | null;
  settings: Settings;
  git: GitInfo | null;
  listings: Record<string, Listing>;
  listingErrors: Record<string, string>;
  expanded: string[];
  /** The row of the file tree that the keys act on. */
  treeSelection: string | null;
  sidebar: "files" | "search";
  /** A narrow pane shows the sidebar over the editor, while this is set. */
  drawer: boolean;
  focus: "editor" | "sidebar";
  tabs: Tab[];
  active: string | null;
  recent: string[];
  places: Record<string, Place>;
  preview: string[];
  overlay: Overlay;
  toasts: Toast[];
  cursor: Cursor;
  vimMode: string | null;
  language: string;
  search: SearchState;
  layout: Layout;
  short: boolean;
  handingOff: "shell" | "quit" | null;
  fileIndex: FileIndex | null;
  /** Something went to the program and has not come back yet: shown as a quiet spinner. */
  busy: number;
  savedAt: Record<string, number>;
}

export const useStore = create<State>(() => ({
  boot: null,
  bootError: null,
  settings: DEFAULT_SETTINGS,
  git: null,
  listings: {},
  listingErrors: {},
  expanded: [],
  treeSelection: null,
  sidebar: "files",
  drawer: false,
  focus: "editor",
  tabs: [],
  active: null,
  recent: [],
  places: {},
  preview: [],
  overlay: null,
  toasts: [],
  cursor: { line: 1, column: 1, selections: 1, selected: 0, lines: 1 },
  vimMode: null,
  language: "Plain Text",
  search: {
    query: "",
    regex: false,
    caseSensitive: false,
    wholeWord: false,
    running: false,
    result: null,
    error: null,
    chosen: null,
    collapsed: [],
  },
  layout: "wide",
  short: false,
  handingOff: null,
  fileIndex: null,
  busy: 0,
  savedAt: {},
}));

export const get = useStore.getState;
export const set = useStore.setState;

export function tabOf(path: string | null): Tab | undefined {
  if (path === null) return undefined;
  return get().tabs.find((t) => t.path === path);
}

export function updateTab(path: string, change: Partial<Tab>): void {
  set((s) => ({ tabs: s.tabs.map((t) => (t.path === path ? { ...t, ...change } : t)) }));
}

export function updateInfo(info: BufferInfo): void {
  set((s) => ({ tabs: s.tabs.map((t) => (t.path === info.path ? { ...t, info } : t)) }));
}

/** The sidebar is beside the editor (a wide enough pane, and shown) or over it (a drawer). */
export function sidebarDocked(state: State = get()): boolean {
  return state.layout !== "narrow" && state.settings.sidebar;
}

export function sidebarVisible(state: State = get()): boolean {
  return sidebarDocked(state) || (state.layout === "narrow" && state.drawer);
}

let toastId = 0;

export function toast(
  text: string,
  tone: Toast["tone"] = "info",
  action?: Toast["action"],
  ms = 4200,
): void {
  const id = ++toastId;
  set((s) => ({
    toasts: [...s.toasts.slice(-2), { id, tone, text, ...(action ? { action } : {}) }],
  }));
  setTimeout(() => dismissToast(id), action ? ms + 2500 : ms);
}

export function dismissToast(id: number): void {
  set((s) => ({ toasts: s.toasts.filter((t) => t.id !== id) }));
}

/** Asks with a dialog; resolves with the action chosen (and what was typed), or null. */
export function ask(
  dialog: Omit<Dialog, "resolve">,
): Promise<{ action: string; value: string } | null> {
  return new Promise((resolve) => {
    set({
      overlay: {
        kind: "dialog",
        dialog: {
          ...dialog,
          resolve: (result) => {
            set((s) => (s.overlay?.kind === "dialog" ? { overlay: null } : {}));
            resolve(result);
          },
        },
      },
    });
  });
}

/** The view's state as the program keeps it for the next view and the next run. */
export function uiState(state: State = get()): UiState {
  const tabs = state.tabs.map((t) => t.path);
  const recent = state.recent.slice(0, 50);
  // Where the cursor was is kept for the files that may be opened again soon, not every one ever.
  const kept = new Set([...tabs, ...recent]);
  return {
    tabs,
    active: state.active,
    expanded: state.expanded,
    places: Object.fromEntries(Object.entries(state.places).filter(([path]) => kept.has(path))),
    recent,
    preview: state.preview.filter((path) => kept.has(path)),
  };
}

export function busy<T>(promise: Promise<T>): Promise<T> {
  set((s) => ({ busy: s.busy + 1 }));
  return promise.finally(() => set((s) => ({ busy: Math.max(0, s.busy - 1) })));
}

/**
 * What the view does: open, show, save and close files, keep the tree, search, and
 * stay in step with the program. Each function is what one command or one click asks
 * for; the commands, the keys and the mouse all come here.
 */

import { EditorSelection } from "@codemirror/state";
import { EditorView } from "@codemirror/view";

import type {
  Boot,
  BufferInfo,
  Changes,
  Entry,
  FileIndex,
  FilesPage,
  Listing,
  OpenedBuffer,
  SearchResult,
  Settings,
  Unopenable,
} from "../../server/types";
import { editor } from "../editor/controller";
import { copy } from "../lib/clipboard";
import { CallError, call, close, subscribe } from "../lib/page";
import { callText } from "../lib/text";
import { ancestors, basename, dirname, join, within } from "../lib/files";
import {
  ask,
  busy,
  get,
  set,
  sidebarDocked,
  tabOf,
  toast,
  uiState,
  updateInfo,
  updateTab,
} from "./store";

function messageOf(error: unknown): string {
  return error instanceof CallError || error instanceof Error ? error.message : String(error);
}

function infoOf(opened: OpenedBuffer): BufferInfo {
  const { text: _text, ...info } = opened;
  return info;
}

// ---- starting --------------------------------------------------------------------------------

export async function start(boot: Boot): Promise<void> {
  const ui = boot.ui;
  set({
    boot,
    settings: boot.settings,
    git: boot.git,
    expanded: ui.expanded,
    recent: ui.recent,
    places: ui.places,
    preview: ui.preview,
    // A file the program still has open (the view comes back from a shell) shows whether
    // it has changes before its text is fetched.
    tabs: ui.tabs.map((path) => {
      const info = boot.buffers.find((b) => b.path === path);
      return { path, status: "loading" as const, ...(info ? { info } : {}) };
    }),
    active: null,
  });
  editor.configure(boot.settings, boot.settings);
  void listDir("");
  for (const dir of ui.expanded) void listDir(dir);

  // The tabs open in the program are opened again; the others come up as they are shown.
  const active = boot.focus?.path ?? ui.active ?? ui.tabs[0] ?? null;
  if (active !== null) {
    await openFile(active, {
      ...(boot.focus?.line ? { line: boot.focus.line } : {}),
      ...(boot.focus?.column ? { column: boot.focus.column } : {}),
      focus: true,
    });
  } else {
    set({ focus: sidebarDocked() ? "sidebar" : "editor" });
  }
  for (const problem of boot.problems) toast(problem, "warn");
  if (boot.returned) {
    const { exitCode, error } = boot.returned;
    toast(
      error
        ? `The shell did not start: ${error}`
        : `Back from the shell${exitCode ? ` (it ended with ${exitCode})` : ""}`,
      error ? "bad" : "info",
    );
  }
  startSync();
}

// ---- the tree --------------------------------------------------------------------------------

export async function listDir(dir: string): Promise<void> {
  try {
    const listing = await call<Listing>("list", { dir });
    set((s) => {
      const { [dir]: _gone, ...errors } = s.listingErrors;
      return { listings: { ...s.listings, [dir]: listing }, listingErrors: errors };
    });
  } catch (error) {
    set((s) => ({ listingErrors: { ...s.listingErrors, [dir]: messageOf(error) } }));
  }
}

export function setExpanded(dir: string, open: boolean): void {
  const expanded = get().expanded;
  if (open === expanded.includes(dir)) return;
  set({
    expanded: open
      ? [...expanded, dir]
      : // A folded directory folds what is in it too: it opens as it was first seen.
        expanded.filter((d) => !within(d, dir)),
  });
  if (open && !get().listings[dir]) void listDir(dir);
}

/** Shows a file in the tree: its directories open, its row chosen. */
export function reveal(path: string): void {
  const dirs = ancestors(path);
  set((s) => ({
    expanded: [...new Set([...s.expanded, ...dirs])],
    treeSelection: path,
  }));
  for (const dir of dirs) if (!get().listings[dir]) void listDir(dir);
}

// ---- tabs ------------------------------------------------------------------------------------

export interface OpenOptions {
  line?: number;
  column?: number;
  /** A range on a line to select, in UTF-16 units: a search match. */
  select?: { line: number; start: number; end: number };
  focus?: boolean;
}

export async function openFile(path: string, options: OpenOptions = {}): Promise<void> {
  const state = get();
  // A file opened from the drawer of a narrow pane closes it: the editor has the keys then.
  if (state.layout === "narrow" && state.drawer) options = { ...options, focus: true };
  if (!tabOf(path)) {
    // A new tab goes after the one shown, where the eye is.
    const at = state.active === null ? -1 : state.tabs.findIndex((t) => t.path === state.active);
    const tabs = [...state.tabs];
    tabs.splice(at < 0 ? tabs.length : at + 1, 0, { path, status: "loading" });
    set({ tabs });
  }
  // The tree follows the file shown, unless the keys are in it.
  if (get().focus !== "sidebar" && !path.startsWith("/")) reveal(path);
  set((s) => ({
    active: path,
    recent: [path, ...s.recent.filter((p) => p !== path)].slice(0, 50),
    treeSelection: s.focus === "sidebar" ? s.treeSelection : path,
    ...(s.layout === "narrow" ? { drawer: false } : {}),
    ...(options.focus ? { focus: "editor" as const } : {}),
  }));
  if (!editor.has(path)) {
    let opened: OpenedBuffer | Unopenable;
    try {
      opened = await busy(callText<OpenedBuffer | Unopenable>("open", { path }));
    } catch (error) {
      updateTab(path, { status: "error", error: messageOf(error) });
      return;
    }
    if ("reason" in opened) {
      updateTab(path, { status: "unopenable", unopenable: opened });
      return;
    }
    if (!editor.has(path)) editor.create(opened);
    updateTab(path, { status: "ready", info: infoOf(opened) });
  }
  if (get().active !== path) return;
  present(path, options);
}

/** Shows the active tab's file in the editor, at a line or a match when one is asked for. */
function present(path: string, options: OpenOptions = {}): void {
  const tab = tabOf(path);
  if (tab?.status !== "ready") {
    editor.clear();
    return;
  }
  const wanted = options.line !== undefined || options.select !== undefined;
  // A file that comes back is where it was left; one already shown stays as it is.
  editor.show(path, wanted || editor.shown === path ? undefined : get().places[path]);
  const view = editor.view;
  if (view && wanted) {
    const doc = view.state.doc;
    const lineNumber = Math.min(Math.max(1, options.select?.line ?? options.line ?? 1), doc.lines);
    const line = doc.line(lineNumber);
    const selection = options.select
      ? EditorSelection.single(
          Math.min(line.from + options.select.start, line.to),
          Math.min(line.from + options.select.end, line.to),
        )
      : EditorSelection.single(
          Math.min(line.from + Math.max(0, (options.column ?? 1) - 1), line.to),
        );
    view.dispatch({
      selection,
      effects: EditorView.scrollIntoView(selection.main.head, { y: "center" }),
    });
  }
  if (options.focus || get().focus === "editor") {
    requestAnimationFrame(() => editor.focus());
  }
}

export function activate(path: string, focus = true): void {
  if (!tabOf(path)) return;
  if (get().active === path) {
    if (focus) editor.focus();
    return;
  }
  void openFile(path, { focus });
}

/** The tab `step` places from the active one, around the ends. */
export function cycleTab(step: number): void {
  const { tabs, active } = get();
  if (tabs.length === 0) return;
  const at = tabs.findIndex((t) => t.path === active);
  const next = tabs[(at + step + tabs.length) % tabs.length]!;
  activate(next.path);
}

export function moveTab(step: number): void {
  const { tabs, active } = get();
  const at = tabs.findIndex((t) => t.path === active);
  const to = at + step;
  if (at < 0 || to < 0 || to >= tabs.length) return;
  const next = [...tabs];
  const [tab] = next.splice(at, 1);
  next.splice(to, 0, tab!);
  set({ tabs: next });
}

/** Closes a tab, asking first when its file has changes that are not saved. */
export async function closeTab(path: string): Promise<boolean> {
  const tab = tabOf(path);
  if (!tab) return true;
  await editor.flush(path);
  let discard = false;
  if (tabOf(path)?.info?.dirty) {
    const answer = await ask({
      title: `Save the changes to ${basename(path)}?`,
      message: "They are lost if the file closes without saving.",
      actions: [
        { id: "save", label: "Save", primary: true },
        { id: "discard", label: "Don’t Save", danger: true },
        { id: "cancel", label: "Cancel" },
      ],
    });
    if (answer === null || answer.action === "cancel") return false;
    if (answer.action === "save" && !(await save(path))) return false;
    discard = answer.action === "discard";
  }
  if (tab.status === "ready" || tab.info !== undefined) {
    try {
      await call("buffer", { path, action: "close", discard });
    } catch (error) {
      toast(messageOf(error), "bad");
      return false;
    }
  }
  editor.forget(path);
  const state = get();
  const tabs = state.tabs.filter((t) => t.path !== path);
  let active = state.active;
  if (active === path) {
    // The file seen last before this one comes back.
    active =
      state.recent.find((p) => p !== path && tabs.some((t) => t.path === p)) ??
      tabs[0]?.path ??
      null;
  }
  const { [path]: _place, ...places } = state.places;
  set({ tabs, active, places, preview: state.preview.filter((p) => p !== path) });
  if (active !== null && active !== state.active) void openFile(active, { focus: true });
  else if (active === null) editor.clear();
  // Opened on files, as `$EDITOR` is: the last one closed is the end of the edit.
  if (tabs.length === 0 && get().boot?.closeQuits) await endView(false);
  return true;
}

export async function closeOthers(keep: string | null): Promise<void> {
  for (const tab of [...get().tabs]) {
    if (tab.path === keep) continue;
    if (!(await closeTab(tab.path))) return;
  }
}

// ---- saving ----------------------------------------------------------------------------------

/** Saves a file; asks before writing over one that changed on disk. Resolves true when saved. */
export async function save(path = get().active, force = false): Promise<boolean> {
  if (path === null) return false;
  const tab = tabOf(path);
  if (tab?.status !== "ready") return false;
  try {
    await editor.flush(path);
    const result = await busy(
      call<{ saved: boolean; info: BufferInfo }>("save", {
        path,
        version: editor.version(path),
        force,
      }),
    );
    updateInfo(result.info);
    if (result.saved) {
      set((s) => ({ savedAt: { ...s.savedAt, [path]: Date.now() } }));
      return true;
    }
    const answer = await ask({
      title: `${basename(path)} changed on disk`,
      message:
        "Another program wrote to it after textpen read it. Saving now writes over what it wrote.",
      actions: [
        { id: "overwrite", label: "Overwrite", danger: true },
        { id: "reload", label: "Reload from Disk" },
        { id: "cancel", label: "Cancel", primary: true },
      ],
    });
    if (answer?.action === "overwrite") return save(path, true);
    if (answer?.action === "reload") await revert(path, false);
    return false;
  } catch (error) {
    toast(`Could not save ${basename(path)}: ${messageOf(error)}`, "bad");
    return false;
  }
}

export async function saveAll(): Promise<boolean> {
  await editor.flushAll();
  let all = true;
  for (const tab of get().tabs) {
    if (tab.info?.dirty) all = (await save(tab.path)) && all;
  }
  return all;
}

/** Reads the file again from disk; asks first when that drops changes. */
export async function revert(path = get().active, confirm = true): Promise<void> {
  if (path === null) return;
  const tab = tabOf(path);
  if (tab?.status !== "ready") return;
  await editor.flush(path);
  if (confirm && tabOf(path)?.info?.dirty) {
    const answer = await ask({
      title: `Drop the changes to ${basename(path)}?`,
      message: "The file is read again from disk, as it was last saved.",
      actions: [
        { id: "revert", label: "Drop Changes", danger: true },
        { id: "cancel", label: "Cancel", primary: true },
      ],
    });
    if (answer?.action !== "revert") return;
  }
  try {
    const opened = await busy(
      callText<OpenedBuffer | Unopenable>("buffer", { path, action: "revert" }),
    );
    if ("reason" in opened) {
      toast(opened.message, "bad");
      return;
    }
    editor.replace(opened);
    updateInfo(infoOf(opened));
  } catch (error) {
    toast(messageOf(error), "bad");
  }
}

/** Keeps the buffer's changes over a file that changed on disk. */
export async function keepMine(path: string): Promise<void> {
  try {
    updateInfo(await call<BufferInfo>("buffer", { path, action: "keep" }));
  } catch (error) {
    toast(messageOf(error), "bad");
  }
}

/** Takes or drops the draft an earlier textpen left of this file. */
export async function draft(path: string, take: boolean): Promise<void> {
  try {
    await editor.flush(path);
    const opened = await callText<OpenedBuffer>("buffer", {
      path,
      action: take ? "restore" : "drop",
    });
    if (take) editor.replace(opened);
    updateInfo(infoOf(opened));
    if (take) toast(`The unsaved changes to ${basename(path)} are back`, "ok");
  } catch (error) {
    toast(messageOf(error), "bad");
  }
}

export async function setEol(path: string, eol: "lf" | "crlf"): Promise<void> {
  try {
    await editor.flush(path);
    updateInfo(await call<BufferInfo>("buffer", { path, action: eol }));
  } catch (error) {
    toast(messageOf(error), "bad");
  }
}

// ---- files -----------------------------------------------------------------------------------

/** The row the tree has chosen, as the listing has it. */
export function selectedEntry(): Entry | undefined {
  const { treeSelection, listings } = get();
  if (treeSelection === null) return undefined;
  for (const listing of Object.values(listings)) {
    const entry = listing.entries.find((e) => e.path === treeSelection);
    if (entry) return entry;
  }
  return undefined;
}

/** The directory a new file goes in: the chosen directory of the tree, or the chosen file's. */
export function targetDir(): string {
  const { treeSelection, listings, focus, active } = get();
  const chosen = focus === "sidebar" ? treeSelection : active;
  if (chosen === null) return "";
  const isDir = Object.values(listings).some((l) =>
    l.entries.some((e) => e.path === chosen && e.kind === "dir"),
  );
  return isDir ? chosen : dirname(chosen);
}

export async function create(kind: "file" | "dir", dir = targetDir()): Promise<void> {
  const prefix = dir === "" ? "" : `${dir}/`;
  const answer = await ask({
    title: kind === "file" ? "New file" : "New folder",
    input: {
      value: prefix,
      placeholder: kind === "file" ? "src/name.ts" : "src/folder",
      hint: "A path from the project's root; missing folders are made.",
    },
    actions: [
      { id: "create", label: "Create", primary: true },
      { id: "cancel", label: "Cancel" },
    ],
  });
  const path = answer?.action === "create" ? answer.value.trim().replace(/\/+$/, "") : "";
  if (path === "" || path === dir) return;
  try {
    const created = await busy(call<{ path: string }>("create", { path, kind }));
    await listDir(dirname(created.path));
    reveal(created.path);
    if (kind === "file") await openFile(created.path, { focus: true });
    else setExpanded(created.path, true);
  } catch (error) {
    toast(messageOf(error), "bad");
  }
}

export async function rename(path: string): Promise<void> {
  const name = basename(path);
  const dot = name.lastIndexOf(".");
  const start = path.length - name.length;
  const answer = await ask({
    title: `Rename ${name}`,
    input: {
      value: path,
      select: [start, dot > 0 ? start + dot : path.length],
      hint: "Change the folders too, to move it.",
    },
    actions: [
      { id: "rename", label: "Rename", primary: true },
      { id: "cancel", label: "Cancel" },
    ],
  });
  const to = answer?.action === "rename" ? answer.value.trim().replace(/\/+$/, "") : "";
  if (to === "" || to === path) return;
  try {
    for (const tab of get().tabs) if (within(tab.path, path)) await editor.flush(tab.path);
    const result = await busy(
      call<{ path: string; buffers: BufferInfo[] }>("rename", { from: path, to }),
    );
    const moved = (p: string) => (within(p, path) ? result.path + p.slice(path.length) : p);
    for (const tab of get().tabs)
      if (within(tab.path, path)) editor.rename(tab.path, moved(tab.path));
    set((s) => ({
      tabs: s.tabs.map((t) => {
        const next = moved(t.path);
        const info = result.buffers.find((b) => b.path === next);
        return { ...t, path: next, ...(info ? { info } : {}) };
      }),
      active: s.active === null ? null : moved(s.active),
      recent: s.recent.map(moved),
      expanded: s.expanded.map(moved),
      preview: s.preview.map(moved),
      places: Object.fromEntries(Object.entries(s.places).map(([p, v]) => [moved(p), v])),
      treeSelection: result.path,
    }));
    await Promise.all([listDir(dirname(path)), listDir(dirname(result.path))]);
    toast(`Renamed to ${result.path}`, "ok");
  } catch (error) {
    toast(messageOf(error), "bad");
  }
}

export async function remove(path: string, isDir: boolean): Promise<void> {
  const answer = await ask({
    title: `Delete ${basename(path)}?`,
    message: isDir
      ? `The folder ${path} and everything in it are deleted. This cannot be undone.`
      : `${path} is deleted. This cannot be undone.`,
    actions: [
      { id: "delete", label: "Delete", danger: true },
      { id: "cancel", label: "Cancel", primary: true },
    ],
  });
  if (answer?.action !== "delete") return;
  try {
    await busy(call("delete", { path }));
    for (const tab of get().tabs) {
      if (!within(tab.path, path)) continue;
      await call("buffer", { path: tab.path, action: "close", discard: true }).catch(() => {});
      editor.forget(tab.path);
      set((s) => ({ tabs: s.tabs.filter((t) => t.path !== tab.path) }));
    }
    const state = get();
    if (state.active !== null && !state.tabs.some((t) => t.path === state.active)) {
      const next =
        state.recent.find((p) => state.tabs.some((t) => t.path === p)) ??
        state.tabs[0]?.path ??
        null;
      set({ active: next });
      if (next) void openFile(next);
      else editor.clear();
    }
    set((s) => ({
      expanded: s.expanded.filter((d) => !within(d, path)),
      treeSelection: dirname(path) || null,
    }));
    await listDir(dirname(path));
    toast(`Deleted ${path}`, "ok");
  } catch (error) {
    toast(messageOf(error), "bad");
  }
}

export async function copyPath(path: string, absolute: boolean): Promise<void> {
  const root = get().boot?.root ?? "";
  const text = absolute && !path.startsWith("/") ? join(root, path) : path;
  toast((await copy(text)) ? `Copied ${text}` : "The clipboard is not available here", "info");
}

// ---- the quick open's list -------------------------------------------------------------------

let indexing: Promise<FileIndex | null> | null = null;

/** The project's files, a message's worth at a time. */
async function allFiles(): Promise<FileIndex> {
  const files: string[] = [];
  let from: number | null = 0;
  let truncated = false;
  while (from !== null) {
    const page: FilesPage = await call<FilesPage>("files", { from });
    files.push(...page.files);
    truncated = page.truncated;
    from = page.next !== null && page.next > from ? page.next : null;
  }
  return { files, truncated };
}

export function fileIndex(fresh = false): Promise<FileIndex | null> {
  if (indexing && !fresh) return indexing;
  indexing = allFiles()
    .then((index) => {
      set({ fileIndex: index });
      return index;
    })
    .catch(() => null)
    .finally(() => setTimeout(() => (indexing = null), 10_000));
  return indexing;
}

// ---- search in files -------------------------------------------------------------------------

let searchRun = 0;

export async function runSearch(): Promise<void> {
  const search = get().search;
  const query = search.query;
  if (query.trim() === "") {
    set({ search: { ...search, result: null, error: null, running: false, chosen: null } });
    return;
  }
  const run = ++searchRun;
  set({ search: { ...search, running: true, error: null } });
  try {
    const result = await call<SearchResult>("search", {
      query,
      regex: search.regex,
      caseSensitive: search.caseSensitive,
      wholeWord: search.wholeWord,
    });
    if (run !== searchRun) return;
    set((s) => ({
      search: {
        ...s.search,
        running: false,
        result,
        chosen: result.files.length > 0 ? { file: 0, match: 0 } : null,
        collapsed: [],
      },
    }));
  } catch (error) {
    if (run !== searchRun) return;
    set((s) => ({
      search: { ...s.search, running: false, error: messageOf(error), result: null },
    }));
  }
}

/** Opens the match the search list has chosen, or the next or previous one (F4, ⇧F4). */
export function openMatch(step = 0, focus = true): void {
  const { result, chosen } = get().search;
  if (!result || result.files.length === 0) return;
  const flat = result.files.flatMap((f, file) => f.matches.map((_, match) => ({ file, match })));
  let at = chosen ? flat.findIndex((m) => m.file === chosen.file && m.match === chosen.match) : -1;
  at = step === 0 ? Math.max(0, at) : (at + step + flat.length) % flat.length;
  const next = flat[at]!;
  set((s) => ({ search: { ...s.search, chosen: next } }));
  const file = result.files[next.file]!;
  const match = file.matches[next.match]!;
  void openFile(file.path, {
    select: { line: match.line, start: match.offset + match.start, end: match.offset + match.end },
    focus,
  });
}

// ---- staying in step with the program ----------------------------------------------------------

let uiTimer: ReturnType<typeof setTimeout> | undefined;
let endChanges: (() => void) | undefined;
let lastUi = "";

/**
 * The program tells the view what happens to the files (`changes`: git, files changed on
 * disk, folders that changed) as it happens, and the view tells the program where it is
 * (tabs, folders, cursors) when that differs from what the program has.
 */
export function startSync(): void {
  stopSync();
  endChanges = subscribe<Changes>("changes", {}, (changes) => {
    void apply(changes).catch(() => {});
  });
  const tick = async () => {
    if (get().handingOff === null) await keepUi().catch(() => {});
    uiTimer = setTimeout(tick, document.hidden ? 6000 : 2000);
  };
  uiTimer = setTimeout(tick, 1500);
}

export function stopSync(): void {
  clearTimeout(uiTimer);
  endChanges?.();
  endChanges = undefined;
}

/** Tells the program where the view is, if that is not what it was last told. */
async function keepUi(): Promise<void> {
  const place = editor.place();
  const active = get().active;
  if (place && active) set((s) => ({ places: { ...s.places, [active]: place } }));
  const ui = uiState();
  const text = JSON.stringify(ui);
  if (text === lastUi) return;
  await call("ui", { ui });
  lastUi = text;
}

async function apply(changes: Changes): Promise<void> {
  if (changes.git !== undefined) set({ git: changes.git });
  if (changes.handoffFailed) {
    set({ handingOff: null });
    toast(changes.handoffFailed, "bad");
  }
  for (const info of changes.buffers) {
    const tab = tabOf(info.path);
    if (!tab || tab.status !== "ready") continue;
    const ours = editor.version(info.path);
    if (!info.dirty && !info.conflict && info.version !== ours && !editor.unsent(info.path)) {
      const opened = await callText<OpenedBuffer>("buffer", {
        path: info.path,
        action: "text",
      });
      editor.replace(opened);
      toast(`${basename(info.path)} changed on disk and was read again`, "info");
    }
    updateInfo(info);
  }
  const listings = get().listings;
  for (const dir of changes.dirs) if (listings[dir]) void listDir(dir);
}

// ---- the pane --------------------------------------------------------------------------------

/** Hands the pane to a shell in the project; the editor comes back when it ends. */
export async function shell(): Promise<void> {
  const boot = get().boot;
  if (!boot?.handoff) {
    toast("There is no terminal to hand over here.", "warn");
    return;
  }
  await editor.flushAll();
  if (get().settings.autoSave) await saveAll();
  await handOffToShell();
}

/** Ends textpen, asking first about files that are not saved. */
export async function quit(): Promise<void> {
  await editor.flushAll();
  const dirty = get().tabs.filter((t) => t.info?.dirty);
  let discard = false;
  if (dirty.length > 0) {
    const answer = await ask({
      title:
        dirty.length === 1
          ? `Save the changes to ${basename(dirty[0]!.path)} before quitting?`
          : `${dirty.length} files have changes that are not saved`,
      message: dirty.length === 1 ? undefined : dirty.map((t) => basename(t.path)).join(", "),
      actions: [
        {
          id: "save",
          label: dirty.length === 1 ? "Save and Quit" : "Save All and Quit",
          primary: true,
        },
        { id: "discard", label: "Quit Without Saving", danger: true },
        { id: "cancel", label: "Cancel" },
      ],
    });
    if (answer === null || answer.action === "cancel") return;
    if (answer.action === "save" && !(await saveAll())) return;
    discard = answer.action === "discard";
  }
  await endView(discard);
}

/**
 * Asks the host to close the view. The program is told and ends with it, keeping what is
 * not saved as drafts, or dropping it first when the person asked to quit without saving.
 */
async function endView(discard: boolean): Promise<void> {
  try {
    await keepState();
    if (discard) await call("discard");
  } catch (error) {
    toast(messageOf(error), "bad");
    return;
  }
  set({ handingOff: "quit", overlay: null });
  close();
}

/** The state goes ahead of the pane, so that the next view (after the shell) opens here. */
async function keepState(): Promise<void> {
  const place = editor.place();
  const active = get().active;
  if (place && active) set((s) => ({ places: { ...s.places, [active]: place } }));
  const ui = uiState();
  await call("ui", { ui });
  lastUi = JSON.stringify(ui);
}

async function handOffToShell(): Promise<void> {
  try {
    await keepState();
    const result = await call<{ state: string }>("handoff", { kind: "shell" });
    if (result.state === "scheduled") set({ handingOff: "shell", overlay: null });
    else
      toast(
        result.state === "busy" ? "The pane is already being handed over." : "Not possible here.",
        "warn",
      );
  } catch (error) {
    toast(messageOf(error), "bad");
  }
}

// ---- settings --------------------------------------------------------------------------------

let settingsTimer: ReturnType<typeof setTimeout> | undefined;

export function changeSettings(change: Partial<Settings>): void {
  const before = get().settings;
  const settings = { ...before, ...change };
  set({ settings });
  editor.configure(settings, before);
  clearTimeout(settingsTimer);
  settingsTimer = setTimeout(() => {
    void call("settings", { settings: get().settings }).catch((error) =>
      toast(`The settings were not kept: ${messageOf(error)}`, "warn"),
    );
  }, 400);
}

export function togglePreview(path = get().active): void {
  if (path === null) return;
  set((s) => ({
    preview: s.preview.includes(path) ? s.preview.filter((p) => p !== path) : [...s.preview, path],
  }));
}

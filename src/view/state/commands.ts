/**
 * Every command, with its keys: the palette lists them, the help shows them, and the
 * keyboard runs them. A key a terminal keeps for itself never reaches the view, so no
 * command depends on one: each is in the palette too.
 *
 * Keys marked `editor` are CodeMirror's own, bound in the editor; the list shows them
 * and the palette runs the same command on the editor.
 */

import {
  copyLineDown,
  copyLineUp,
  deleteLine,
  moveLineDown,
  moveLineUp,
  redo,
  selectLine,
  toggleComment,
  undo,
  addCursorAbove,
  addCursorBelow,
} from "@codemirror/commands";
import { foldAll, unfoldAll } from "@codemirror/language";
import {
  getSearchQuery,
  openSearchPanel,
  selectNextOccurrence,
  selectSelectionMatches,
  setSearchQuery,
} from "@codemirror/search";
import type { Command as EditorCommand } from "@codemirror/view";

import { editor } from "../editor/controller";
import { wantReplace } from "../editor/SearchPanel";
import { isMac } from "../lib/keys";
import { extension } from "../lib/files";
import {
  changeSettings,
  closeOthers,
  closeTab,
  copyPath,
  create,
  cycleTab,
  draft,
  keepMine,
  moveTab,
  openFile,
  openMatch,
  quit,
  remove,
  rename,
  reveal,
  revert,
  save,
  saveAll,
  setEol,
  shell,
  togglePreview,
  activate,
} from "./actions";
import { get, set, sidebarDocked, tabOf, toast } from "./store";
import type { PaletteMode } from "./store";

export interface Command {
  id: string;
  title: string;
  category: "File" | "Edit" | "Selection" | "Go" | "View" | "Tabs" | "Search" | "textpen";
  keys?: string[];
  /** The keys are CodeMirror's, handled in the editor; the window does not take them. */
  editor?: boolean;
  /** Whether the command means anything now; it is not listed or run when not. */
  when?: () => boolean;
  run: () => void | Promise<void>;
}

const hasFile = () => tabOf(get().active)?.status === "ready";
const editorFocused = () => {
  const view = editor.view;
  return view !== null && view.dom.contains(document.activeElement);
};
/** Vim takes the ⌃ keys in the editor (⌃W, ⌃D, ⌃U…); textpen's give way to it there. */
const ctrlFree = () => !(get().settings.vim && editorFocused());

function onEditor(command: EditorCommand): () => void {
  return () => {
    const view = editor.view;
    if (!view || !hasFile()) return;
    command(view);
    view.focus();
  };
}

export function openPalette(mode: PaletteMode, query = ""): void {
  set({ overlay: { kind: "palette", mode, query } });
}

export function focusSidebar(panel: "files" | "search"): void {
  set((s) => ({
    sidebar: panel,
    focus: "sidebar",
    ...(s.layout === "narrow" ? { drawer: true } : { settings: { ...s.settings, sidebar: true } }),
  }));
  if (!get().settings.sidebar && get().layout !== "narrow") changeSettings({ sidebar: true });
}

export function focusEditor(): void {
  set({ focus: "editor", drawer: false });
  requestAnimationFrame(() => editor.focus());
}

function toggleSidebar(): void {
  const state = get();
  if (state.layout === "narrow") {
    set({ drawer: !state.drawer, focus: state.drawer ? "editor" : "sidebar" });
    if (state.drawer) focusEditor();
    return;
  }
  const shown = !state.settings.sidebar;
  changeSettings({ sidebar: shown });
  if (!shown && state.focus === "sidebar") focusEditor();
}

/** F6: the sidebar and the editor, in turn. */
function cycleFocus(): void {
  const state = get();
  if (state.focus === "sidebar" || !(sidebarDocked(state) || state.layout === "narrow")) {
    focusEditor();
  } else {
    focusSidebar(state.sidebar);
  }
}

export function find(replace: boolean): void {
  const view = editor.view;
  if (!view || !hasFile()) {
    focusSidebar("search");
    return;
  }
  set({ focus: "editor", drawer: false });
  if (replace) wantReplace();
  view.focus();
  openSearchPanel(view);
  // An open panel learns that the replacement is wanted from an update.
  if (replace) view.dispatch({ effects: setSearchQuery.of(getSearchQuery(view.state)) });
}

const RECENTLY_CLOSED: string[] = [];

/** Closes a tab, and remembers it for Reopen Closed Tab. */
export async function closeAndRemember(path: string): Promise<void> {
  if (await closeTab(path)) {
    RECENTLY_CLOSED.unshift(path);
    RECENTLY_CLOSED.splice(20);
  }
}

function zoom(step: number | null): void {
  const size = get().settings.fontSize;
  changeSettings({ fontSize: step === null ? 13 : Math.min(28, Math.max(9, size + step)) });
}

export const COMMANDS: Command[] = [
  // ---- going places ------------------------------------------------------------------------
  {
    id: "files",
    title: "Go to File…",
    category: "Go",
    keys: ["Mod-k"],
    run: () => openPalette("files"),
  },
  {
    id: "commands",
    title: "Show All Commands",
    category: "Go",
    keys: ["F1"],
    run: () => openPalette("commands"),
  },
  {
    id: "switch",
    title: "Switch to Open File…",
    category: "Go",
    keys: ["Mod-e"],
    when: () => get().tabs.length > 0,
    run: () => openPalette("tabs"),
  },
  {
    id: "line",
    title: "Go to Line…",
    category: "Go",
    keys: ["Mod-l"],
    when: hasFile,
    run: () => openPalette("line"),
  },
  {
    id: "language",
    title: "Change Language…",
    category: "View",
    when: hasFile,
    run: () => openPalette("language"),
  },
  // ---- files -------------------------------------------------------------------------------
  {
    id: "save",
    title: "Save",
    category: "File",
    keys: ["Mod-s"],
    when: hasFile,
    run: () => void save(),
  },
  {
    id: "save-all",
    title: "Save All",
    category: "File",
    keys: ["Alt-Mod-s"],
    when: () => get().tabs.some((t) => t.info?.dirty),
    run: () => void saveAll(),
  },
  {
    id: "new-file",
    title: "New File…",
    category: "File",
    keys: ["Alt-Mod-n"],
    run: () => void create("file"),
  },
  {
    id: "new-folder",
    title: "New Folder…",
    category: "File",
    run: () => void create("dir"),
  },
  {
    id: "rename",
    title: "Rename…",
    category: "File",
    keys: ["F2"],
    when: () => (get().focus === "sidebar" ? get().treeSelection !== null : get().active !== null),
    run: () => {
      const s = get();
      const path = s.focus === "sidebar" ? s.treeSelection : s.active;
      if (path) void rename(path);
    },
  },
  {
    id: "delete",
    title: "Delete File…",
    category: "File",
    when: () => get().active !== null,
    run: () => {
      const path = get().active;
      if (path) void remove(path, false);
    },
  },
  {
    id: "revert",
    title: "Revert File",
    category: "File",
    when: () => tabOf(get().active)?.info?.dirty === true,
    run: () => void revert(),
  },
  {
    id: "draft-restore",
    title: "Restore Unsaved Changes from Last Time",
    category: "File",
    when: () => tabOf(get().active)?.info?.draft !== undefined,
    run: () => void draft(get().active!, true),
  },
  {
    id: "draft-drop",
    title: "Discard Unsaved Changes from Last Time",
    category: "File",
    when: () => tabOf(get().active)?.info?.draft !== undefined,
    run: () => void draft(get().active!, false),
  },
  {
    id: "conflict-reload",
    title: "Reload from Disk (Drop My Changes)",
    category: "File",
    when: () => tabOf(get().active)?.info?.conflict === true,
    run: () => void revert(get().active, false),
  },
  {
    id: "conflict-keep",
    title: "Keep My Changes over the Disk",
    category: "File",
    when: () => tabOf(get().active)?.info?.conflict === true,
    run: () => void keepMine(get().active!),
  },
  {
    id: "copy-path",
    title: "Copy Path",
    category: "File",
    when: () => get().active !== null,
    run: () => void copyPath(get().active!, false),
  },
  {
    id: "copy-absolute-path",
    title: "Copy Absolute Path",
    category: "File",
    when: () => get().active !== null,
    run: () => void copyPath(get().active!, true),
  },
  {
    id: "reveal",
    title: "Reveal in Files",
    category: "File",
    when: () => get().active !== null,
    run: () => {
      reveal(get().active!);
      focusSidebar("files");
    },
  },
  {
    id: "eol",
    title: "Change Line Endings (LF / CRLF)",
    category: "File",
    when: hasFile,
    run: () => {
      const tab = tabOf(get().active);
      if (tab?.info) void setEol(tab.path, tab.info.eol === "lf" ? "crlf" : "lf");
    },
  },
  // ---- tabs --------------------------------------------------------------------------------
  {
    id: "close",
    title: "Close Tab",
    category: "Tabs",
    keys: ["Ctrl-w"],
    when: () => get().active !== null && ctrlFree(),
    run: () => {
      const path = get().active;
      if (path) void closeAndRemember(path);
    },
  },
  {
    id: "close-others",
    title: "Close Other Tabs",
    category: "Tabs",
    when: () => get().tabs.length > 1,
    run: () => void closeOthers(get().active),
  },
  {
    id: "close-all",
    title: "Close All Tabs",
    category: "Tabs",
    when: () => get().tabs.length > 0,
    run: () => void closeOthers(null),
  },
  {
    id: "reopen",
    title: "Reopen Closed Tab",
    category: "Tabs",
    keys: ["Shift-Mod-t"],
    when: () => RECENTLY_CLOSED.length > 0,
    run: () => {
      const path = RECENTLY_CLOSED.shift();
      if (path) void openFile(path, { focus: true });
    },
  },
  {
    id: "next-tab",
    title: "Next Tab",
    category: "Tabs",
    keys: ["Ctrl-PageDown", "Ctrl-Alt-ArrowRight"],
    when: () => get().tabs.length > 1,
    run: () => cycleTab(1),
  },
  {
    id: "previous-tab",
    title: "Previous Tab",
    category: "Tabs",
    keys: ["Ctrl-PageUp", "Ctrl-Alt-ArrowLeft"],
    when: () => get().tabs.length > 1,
    run: () => cycleTab(-1),
  },
  {
    id: "move-tab-right",
    title: "Move Tab Right",
    category: "Tabs",
    keys: ["Shift-Ctrl-PageDown"],
    when: () => get().tabs.length > 1,
    run: () => moveTab(1),
  },
  {
    id: "move-tab-left",
    title: "Move Tab Left",
    category: "Tabs",
    keys: ["Shift-Ctrl-PageUp"],
    when: () => get().tabs.length > 1,
    run: () => moveTab(-1),
  },
  ...Array.from({ length: 9 }, (_, i): Command => ({
    id: `tab-${i + 1}`,
    title: `Go to Tab ${i + 1}`,
    category: "Tabs",
    keys: [`Ctrl-${i + 1}`],
    when: () => get().tabs.length > i && ctrlFree(),
    run: () => {
      const tabs = get().tabs;
      // ⌃9 is the last tab, as in a browser.
      const tab = i === 8 ? tabs.at(-1) : tabs[i];
      if (tab) activate(tab.path);
    },
  })),
  // ---- view --------------------------------------------------------------------------------
  {
    id: "sidebar",
    title: "Toggle Sidebar",
    category: "View",
    keys: ["Mod-b"],
    run: toggleSidebar,
  },
  {
    id: "show-files",
    title: "Show Files",
    category: "View",
    keys: ["Shift-Mod-e"],
    run: () => focusSidebar("files"),
  },
  {
    id: "focus",
    title: "Move Focus Between Sidebar and Editor",
    category: "View",
    keys: ["F6"],
    run: cycleFocus,
  },
  {
    id: "zoom-in",
    title: "Bigger Text",
    category: "View",
    keys: ["Mod-="],
    run: () => zoom(1),
  },
  {
    id: "zoom-out",
    title: "Smaller Text",
    category: "View",
    keys: ["Mod--"],
    run: () => zoom(-1),
  },
  {
    id: "zoom-reset",
    title: "Actual Text Size",
    category: "View",
    keys: ["Mod-0"],
    run: () => zoom(null),
  },
  {
    id: "wrap",
    title: "Toggle Word Wrap",
    category: "View",
    run: () => changeSettings({ wrap: !get().settings.wrap }),
  },
  {
    id: "whitespace",
    title: "Toggle Whitespace",
    category: "View",
    run: () => changeSettings({ whitespace: !get().settings.whitespace }),
  },
  {
    id: "line-numbers",
    title: "Line Numbers: On, Relative, Off",
    category: "View",
    run: () => {
      const order = ["on", "relative", "off"] as const;
      const next = order[(order.indexOf(get().settings.lineNumbers) + 1) % order.length]!;
      changeSettings({ lineNumbers: next });
      toast(`Line numbers: ${next}`, "info", undefined, 1600);
    },
  },
  {
    id: "vim",
    title: "Toggle Vim Keys",
    category: "textpen",
    run: () => {
      const vim = !get().settings.vim;
      changeSettings({ vim });
      toast(vim ? "Vim keys on: :w saves, :q closes the tab, :qa quits" : "Vim keys off", "info");
    },
  },
  {
    id: "preview",
    title: "Toggle Markdown Preview",
    category: "View",
    keys: ["Shift-Mod-v"],
    when: () => ["md", "markdown", "mdx"].includes(extension(get().active ?? "")),
    run: () => togglePreview(),
  },
  {
    id: "indentation",
    title: "Indentation…",
    category: "View",
    run: () => openPalette("indent"),
  },
  {
    id: "settings",
    title: "Settings",
    category: "textpen",
    run: () => set({ overlay: { kind: "settings" } }),
  },
  {
    id: "help",
    title: "Keyboard Shortcuts",
    category: "textpen",
    keys: ["Shift-Mod-/"],
    run: () => set({ overlay: { kind: "help" } }),
  },
  {
    id: "shell",
    title: "Open a Shell Here",
    category: "textpen",
    keys: ["Mod-j"],
    when: () => get().boot?.handoff === true,
    run: () => void shell(),
  },
  {
    id: "quit",
    title: "Quit textpen",
    category: "textpen",
    keys: ["Ctrl-q"],
    when: () => get().boot?.quit === true,
    run: () => void quit(),
  },
  // ---- search ------------------------------------------------------------------------------
  {
    id: "find",
    title: "Find",
    category: "Search",
    keys: ["Mod-f"],
    // In the editor ⌘F is CodeMirror's; elsewhere it brings the editor and its search.
    when: () => !editorFocused(),
    run: () => find(false),
  },
  {
    id: "replace",
    title: "Replace",
    category: "Search",
    keys: ["Mod-Alt-f"],
    when: hasFile,
    run: () => find(true),
  },
  {
    id: "search",
    title: "Search in Files",
    category: "Search",
    keys: ["Shift-Mod-f"],
    run: () => {
      // The words chosen in the editor are what is looked for.
      const view = editor.view;
      const selected =
        view && hasFile()
          ? view.state.sliceDoc(view.state.selection.main.from, view.state.selection.main.to)
          : "";
      if (selected && !selected.includes("\n"))
        set((s) => ({ search: { ...s.search, query: selected } }));
      focusSidebar("search");
    },
  },
  {
    id: "next-match",
    title: "Next Search Result",
    category: "Search",
    keys: ["F4"],
    when: () => (get().search.result?.matches ?? 0) > 0,
    run: () => openMatch(1),
  },
  {
    id: "previous-match",
    title: "Previous Search Result",
    category: "Search",
    keys: ["Shift-F4"],
    when: () => (get().search.result?.matches ?? 0) > 0,
    run: () => openMatch(-1),
  },
  // ---- editing (CodeMirror's keys) ---------------------------------------------------------
  {
    id: "undo",
    title: "Undo",
    category: "Edit",
    keys: ["Mod-z"],
    editor: true,
    when: hasFile,
    run: onEditor(undo),
  },
  {
    id: "redo",
    title: "Redo",
    category: "Edit",
    keys: ["Shift-Mod-z"],
    editor: true,
    when: hasFile,
    run: onEditor(redo),
  },
  {
    id: "comment",
    title: "Toggle Comment",
    category: "Edit",
    keys: ["Mod-/"],
    editor: true,
    when: hasFile,
    run: onEditor(toggleComment),
  },
  {
    id: "delete-line",
    title: "Delete Line",
    category: "Edit",
    keys: ["Shift-Mod-k"],
    editor: true,
    when: hasFile,
    run: onEditor(deleteLine),
  },
  {
    id: "move-line-up",
    title: "Move Line Up",
    category: "Edit",
    keys: ["Alt-ArrowUp"],
    editor: true,
    when: hasFile,
    run: onEditor(moveLineUp),
  },
  {
    id: "move-line-down",
    title: "Move Line Down",
    category: "Edit",
    keys: ["Alt-ArrowDown"],
    editor: true,
    when: hasFile,
    run: onEditor(moveLineDown),
  },
  {
    id: "copy-line-up",
    title: "Copy Line Up",
    category: "Edit",
    keys: ["Shift-Alt-ArrowUp"],
    editor: true,
    when: hasFile,
    run: onEditor(copyLineUp),
  },
  {
    id: "copy-line-down",
    title: "Copy Line Down",
    category: "Edit",
    keys: ["Shift-Alt-ArrowDown"],
    editor: true,
    when: hasFile,
    run: onEditor(copyLineDown),
  },
  {
    id: "fold-all",
    title: "Fold All",
    category: "Edit",
    keys: ["Ctrl-Alt-["],
    editor: true,
    when: hasFile,
    run: onEditor(foldAll),
  },
  {
    id: "unfold-all",
    title: "Unfold All",
    category: "Edit",
    keys: ["Ctrl-Alt-]"],
    editor: true,
    when: hasFile,
    run: onEditor(unfoldAll),
  },
  {
    id: "select-line",
    title: "Select Line",
    category: "Selection",
    keys: ["Alt-l"].filter(() => !isMac),
    editor: true,
    when: hasFile,
    run: onEditor(selectLine),
  },
  {
    id: "select-next",
    title: "Add Next Occurrence",
    category: "Selection",
    keys: [isMac ? "Ctrl-g" : "Mod-d"],
    editor: true,
    when: hasFile,
    run: onEditor(selectNextOccurrence),
  },
  {
    id: "select-all-matches",
    title: "Select All Occurrences",
    category: "Selection",
    keys: ["Shift-Mod-l"],
    editor: true,
    when: hasFile,
    run: onEditor(selectSelectionMatches),
  },
  {
    id: "cursor-above",
    title: "Add Cursor Above",
    category: "Selection",
    keys: ["Ctrl-Shift-ArrowUp"],
    editor: true,
    when: hasFile,
    run: onEditor(addCursorAbove),
  },
  {
    id: "cursor-below",
    title: "Add Cursor Below",
    category: "Selection",
    keys: ["Ctrl-Shift-ArrowDown"],
    editor: true,
    when: hasFile,
    run: onEditor(addCursorBelow),
  },
];

export function available(): Command[] {
  return COMMANDS.filter((c) => !c.when || c.when());
}

export function command(id: string): Command | undefined {
  return COMMANDS.find((c) => c.id === id);
}

export function run(id: string): void {
  const found = command(id);
  if (found && (!found.when || found.when())) void found.run();
}

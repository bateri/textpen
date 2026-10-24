/**
 * The editor: one CodeMirror view, and one editor state for each open file, so that a
 * file keeps its undo history, selection and folds while another is shown.
 *
 * The program holds the text; this side sends it what changed. Every edit is composed
 * into the file's pending changes and sent a moment after typing stops, as a list of
 * plain replacements made from the end of the file to its start (so that one does not
 * move the next), cut so that no call comes near the 1 MiB a terminal lets through to
 * its program. Each call names the version it was made on; the program refuses one
 * that is not its own, and the view then sends its whole text again.
 */

import {
  autocompletion,
  closeBrackets,
  closeBracketsKeymap,
  completionKeymap,
} from "@codemirror/autocomplete";
import {
  addCursorAbove,
  addCursorBelow,
  defaultKeymap,
  history,
  historyKeymap,
  indentWithTab,
} from "@codemirror/commands";
import {
  bracketMatching,
  foldGutter,
  foldKeymap,
  indentOnInput,
  indentUnit,
  syntaxHighlighting,
} from "@codemirror/language";
import {
  highlightSelectionMatches,
  search,
  searchKeymap,
  selectNextOccurrence,
} from "@codemirror/search";
import {
  Annotation,
  ChangeSet,
  Compartment,
  EditorSelection,
  EditorState,
  Transaction,
} from "@codemirror/state";
import type { Extension } from "@codemirror/state";
import {
  EditorView,
  crosshairCursor,
  drawSelection,
  dropCursor,
  highlightActiveLine,
  highlightActiveLineGutter,
  highlightSpecialChars,
  highlightTrailingWhitespace,
  highlightWhitespace,
  keymap,
  lineNumbers,
  rectangularSelection,
} from "@codemirror/view";
import type { KeyBinding, ViewUpdate } from "@codemirror/view";

import type { BufferInfo, OpenedBuffer, Place, Settings } from "../../server/types";
import { CallError, call } from "../lib/page";
import { callText } from "../lib/text";
import { isMac } from "../lib/keys";
import { get, set, toast, updateInfo } from "../state/store";
import { batches } from "./batches";
import { describe, find, load } from "./languages";
import { createSearchPanel } from "./SearchPanel";
import { editorTheme, highlight } from "./theme";

/** A change the program made (a file read again from disk): not sent back to it. */
const fromProgram = Annotation.define<boolean>();

/** How long after the last keystroke the changes go to the program. */
const SEND_DELAY = 350;
/** Keys CodeMirror binds that a terminal keeps for itself (bateri's tabs, splits and folders). */
const KEPT_BY_TERMINAL = new Set([
  "Mod-[",
  "Mod-]",
  "Mod-d",
  "Mod-Alt-g",
  "Mod-Alt-ArrowUp",
  "Mod-Alt-ArrowDown",
  "Shift-Mod-\\",
]);

function withoutKept(bindings: readonly KeyBinding[]): KeyBinding[] {
  return bindings.filter(
    (b) => !KEPT_BY_TERMINAL.has(b.key ?? "") && !KEPT_BY_TERMINAL.has(b.mac ?? ""),
  );
}

interface Sync {
  path: string;
  /** The program's version of the text, as of the last answer. */
  version: number;
  /** What changed since the last call, as one change set over the text the program has. */
  pending: ChangeSet | null;
  timer: ReturnType<typeof setTimeout> | undefined;
  chain: Promise<void>;
  /** A call failed: the next one sends the whole text. */
  broken: boolean;
}

const language = new Compartment();
const wrapping = new Compartment();
const indentation = new Compartment();
const numbers = new Compartment();
const whitespace = new Compartment();
const vimMode = new Compartment();
const readOnly = new Compartment();

type VimModule = typeof import("@replit/codemirror-vim");

class Controller {
  view: EditorView | null = null;
  readonly #states = new Map<string, EditorState>();
  readonly #syncs = new Map<string, Sync>();
  #shown: string | null = null;
  #vim: VimModule | null = null;
  #vimLoading: Promise<VimModule> | null = null;
  #cursorFrame = 0;
  #numbersLine = 0;
  /** Called with the vim command line's `:w`, `:q` and the rest. */
  onVim: ((command: "write" | "quit" | "writequit" | "quitall", force: boolean) => void) | null =
    null;

  get shown(): string | null {
    return this.#shown;
  }

  attach(parent: HTMLElement): void {
    if (this.view) {
      parent.append(this.view.dom);
      return;
    }
    this.view = new EditorView({ parent, state: EditorState.create({ doc: "" }) });
  }

  has(path: string): boolean {
    return this.#states.has(path);
  }

  /** Makes the editor state of a file the program opened. */
  create(opened: OpenedBuffer): void {
    const settings = get().settings;
    const state = EditorState.create({
      doc: opened.text,
      extensions: this.#extensions(settings, opened.readonly),
    });
    this.#states.set(opened.path, state);
    this.#syncs.set(opened.path, {
      path: opened.path,
      version: opened.version,
      pending: null,
      timer: undefined,
      chain: Promise.resolve(),
      broken: false,
    });
    void this.#loadLanguage(opened.path, opened.text.slice(0, 200).split("\n")[0] ?? "");
  }

  /** Shows a file's state in the view, with its cursor and scroll where they were. */
  show(path: string, place?: Place): void {
    const view = this.view;
    if (!view || !this.#states.has(path)) return;
    if (this.#shown !== path) {
      // The state of the file shown is the view's own: it is kept before another takes its place.
      this.#remember();
      view.setState(this.#states.get(path)!);
      this.#shown = path;
      this.#attachVim();
    }
    if (place) {
      const max = view.state.doc.length;
      view.dispatch({
        selection: EditorSelection.single(Math.min(place.anchor, max), Math.min(place.head, max)),
        effects: EditorView.scrollIntoView(Math.min(place.top, max), { y: "start" }),
      });
    }
    this.#report();
  }

  /** The view shows nothing: the last tab closed. */
  clear(): void {
    this.#remember();
    this.#shown = null;
    this.view?.setState(EditorState.create({ doc: "" }));
  }

  focus(): void {
    this.view?.focus();
  }

  state(path: string): EditorState | undefined {
    if (path === this.#shown && this.view) return this.view.state;
    return this.#states.get(path);
  }

  text(path: string): string | undefined {
    return this.state(path)?.doc.toString();
  }

  forget(path: string): void {
    const sync = this.#syncs.get(path);
    if (sync) clearTimeout(sync.timer);
    this.#syncs.delete(path);
    this.#states.delete(path);
    if (this.#shown === path) this.clear();
  }

  rename(from: string, to: string): void {
    const state = this.#states.get(from);
    const sync = this.#syncs.get(from);
    if (state) {
      this.#states.delete(from);
      this.#states.set(to, state);
    }
    if (sync) {
      this.#syncs.delete(from);
      sync.path = to;
      this.#syncs.set(to, sync);
    }
    if (this.#shown === from) this.#shown = to;
    void this.#loadLanguage(to, "");
  }

  /** Where the cursor and the scroll are in the file shown. */
  place(): Place | undefined {
    const view = this.view;
    if (!view || this.#shown === null) return undefined;
    const { anchor, head } = view.state.selection.main;
    const top = view.lineBlockAtHeight(view.scrollDOM.scrollTop).from;
    return { anchor, head, top };
  }

  #remember(): void {
    const path = this.#shown;
    const view = this.view;
    if (path === null || !view) return;
    this.#states.set(path, view.state);
    const place = this.place();
    if (place) set((s) => ({ places: { ...s.places, [path]: place } }));
  }

  // ---- what goes to the program -----------------------------------------------------------

  /** Sends what changed in a file now, and resolves when the program has it. */
  flush(path: string): Promise<void> {
    const sync = this.#syncs.get(path);
    if (!sync) return Promise.resolve();
    clearTimeout(sync.timer);
    sync.timer = undefined;
    sync.chain = sync.chain.then(() => this.#send(sync));
    return sync.chain;
  }

  flushAll(): Promise<void> {
    return Promise.all([...this.#syncs.keys()].map((p) => this.flush(p))).then(() => {});
  }

  version(path: string): number | undefined {
    return this.#syncs.get(path)?.version;
  }

  /** Whether a file has changes the program has not been sent yet. */
  unsent(path: string): boolean {
    const sync = this.#syncs.get(path);
    return sync !== undefined && (sync.pending !== null || sync.broken);
  }

  #changed(path: string, changes: ChangeSet): void {
    const sync = this.#syncs.get(path);
    if (!sync) return;
    sync.pending = sync.pending ? sync.pending.compose(changes) : changes;
    clearTimeout(sync.timer);
    sync.timer = setTimeout(() => void this.flush(path), SEND_DELAY);
    // The tab says so at once; the program's answer says whether it still differs from the file.
    const tab = get().tabs.find((t) => t.path === path);
    if (tab?.info && !tab.info.dirty) updateInfo({ ...tab.info, dirty: true });
  }

  async #send(sync: Sync): Promise<void> {
    if (sync.broken) {
      await this.#resend(sync);
      return;
    }
    const pending = sync.pending;
    if (pending === null || pending.empty) {
      sync.pending = null;
      return;
    }
    sync.pending = null;
    try {
      for (const edits of batches(pending)) {
        const info = await call<BufferInfo>("edit", {
          path: sync.path,
          base: sync.version,
          edits,
        });
        sync.version = info.version;
        updateInfo(info);
      }
    } catch (error) {
      sync.broken = true;
      await this.#resend(sync).catch(() => {
        toast(
          `Could not send the changes to ${sync.path}: ${messageOf(error)}. They are kept here; saving tries again.`,
          "bad",
        );
      });
    }
  }

  /** The program's text and the view's differ: the view's whole text replaces the program's. */
  async #resend(sync: Sync): Promise<void> {
    const text = this.text(sync.path);
    if (text === undefined) return;
    const current = await callText<OpenedBuffer>("buffer", {
      path: sync.path,
      action: "text",
    });
    sync.version = current.version;
    sync.pending = null;
    if (current.text !== text) {
      const changes = ChangeSet.of(
        [{ from: 0, to: current.text.length, insert: text }],
        current.text.length,
      );
      for (const edits of batches(changes)) {
        const info = await call<BufferInfo>("edit", {
          path: sync.path,
          base: sync.version,
          edits,
        });
        sync.version = info.version;
        updateInfo(info);
      }
    }
    sync.broken = false;
  }

  /**
   * The program's text replaces the view's: the file was read again from disk, or a draft
   * was taken. Only what differs is replaced, so the cursor stays where it was.
   */
  replace(opened: OpenedBuffer): void {
    const sync = this.#syncs.get(opened.path);
    const state = this.state(opened.path);
    if (!sync || !state) return;
    clearTimeout(sync.timer);
    sync.pending = null;
    sync.broken = false;
    sync.version = opened.version;
    const old = state.doc.toString();
    if (old === opened.text) return;
    let start = 0;
    const end = Math.min(old.length, opened.text.length);
    while (start < end && old.charCodeAt(start) === opened.text.charCodeAt(start)) start += 1;
    let oldEnd = old.length;
    let newEnd = opened.text.length;
    while (
      oldEnd > start &&
      newEnd > start &&
      old.charCodeAt(oldEnd - 1) === opened.text.charCodeAt(newEnd - 1)
    ) {
      oldEnd -= 1;
      newEnd -= 1;
    }
    const spec = {
      changes: { from: start, to: oldEnd, insert: opened.text.slice(start, newEnd) },
      annotations: [fromProgram.of(true), Transaction.addToHistory.of(false)],
    };
    if (opened.path === this.#shown && this.view) this.view.dispatch(spec);
    else this.#states.set(opened.path, state.update(spec).state);
  }

  // ---- settings ----------------------------------------------------------------------------

  /** Applies the settings to every open file. */
  configure(settings: Settings, before: Settings): void {
    if (settings.vim && !this.#vim) {
      void this.#loadVim().then(() => this.configure(get().settings, before));
      return;
    }
    const effects = [
      wrapping.reconfigure(settings.wrap ? EditorView.lineWrapping : []),
      indentation.reconfigure(indentationOf(settings)),
      numbers.reconfigure(this.#numbersOf(settings)),
      whitespace.reconfigure(
        settings.whitespace ? [highlightWhitespace(), highlightTrailingWhitespace()] : [],
      ),
      vimMode.reconfigure(settings.vim && this.#vim ? this.#vim.vim({ status: false }) : []),
    ];
    for (const [path, state] of this.#states) {
      if (path === this.#shown && this.view) this.view.dispatch({ effects });
      else this.#states.set(path, state.update({ effects }).state);
    }
    if (settings.vim !== before.vim) {
      this.#attachVim();
      if (!settings.vim) set({ vimMode: null });
    }
    if (settings.fontSize !== before.fontSize) this.view?.requestMeasure();
  }

  setReadOnly(path: string, value: boolean): void {
    const effects = readOnly.reconfigure(EditorState.readOnly.of(value));
    const state = this.#states.get(path);
    if (path === this.#shown && this.view) this.view.dispatch({ effects });
    else if (state) this.#states.set(path, state.update({ effects }).state);
  }

  /** Changes the language a file is read as, by its name in the language list. */
  async setLanguage(path: string, name: string | null): Promise<void> {
    const description = name === null ? null : find(name);
    const support = description ? await load(description) : null;
    this.#applyLanguage(path, support ?? [], description?.name ?? "Plain Text");
  }

  async #loadLanguage(path: string, firstLine: string): Promise<void> {
    const description = describe(path, firstLine);
    if (!description) {
      this.#applyLanguage(path, [], "Plain Text");
      return;
    }
    const support = await load(description);
    this.#applyLanguage(path, support ?? [], support ? description.name : "Plain Text");
  }

  readonly #languages = new Map<string, string>();

  languageOf(path: string): string {
    return this.#languages.get(path) ?? "Plain Text";
  }

  #applyLanguage(path: string, support: Extension, name: string): void {
    this.#languages.set(path, name);
    const effects = language.reconfigure(support);
    const state = this.#states.get(path);
    if (path === this.#shown && this.view) this.view.dispatch({ effects });
    else if (state) this.#states.set(path, state.update({ effects }).state);
    if (path === this.#shown) set({ language: name });
  }

  #numbersOf(settings: Settings): Extension {
    if (settings.lineNumbers === "off") return [];
    if (settings.lineNumbers === "on") return [lineNumbers(), highlightActiveLineGutter()];
    return [
      lineNumbers({
        formatNumber: (n, state) => {
          const current = state.doc.lineAt(state.selection.main.head).number;
          return n === current ? String(n) : String(Math.abs(n - current));
        },
      }),
      highlightActiveLineGutter(),
    ];
  }

  async #loadVim(): Promise<VimModule> {
    this.#vimLoading ??= import("@replit/codemirror-vim").then((module) => {
      this.#vim = module;
      const { Vim } = module;
      const run =
        (command: "write" | "quit" | "writequit" | "quitall") =>
        (_cm: unknown, params: { argString?: string; input?: string }) =>
          this.onVim?.(command, /!$/.test(params.input ?? "") || params.argString === "!");
      Vim.defineEx("write", "w", run("write"));
      Vim.defineEx("quit", "q", run("quit"));
      Vim.defineEx("wq", "wq", run("writequit"));
      Vim.defineEx("xit", "x", run("writequit"));
      Vim.defineEx("qall", "qa", run("quitall"));
      return module;
    });
    return this.#vimLoading;
  }

  #attachVim(): void {
    const view = this.view;
    const vim = this.#vim;
    if (!view || !vim || !get().settings.vim) return;
    const cm = vim.getCM(view);
    if (!cm || (cm as unknown as { textpen?: boolean }).textpen) return;
    (cm as unknown as { textpen?: boolean }).textpen = true;
    cm.on("vim-mode-change", (event: { mode: string; subMode?: string }) => {
      const mode = event.subMode ? `${event.mode} ${event.subMode}` : event.mode;
      set({ vimMode: mode });
    });
    set({ vimMode: "normal" });
  }

  // ---- the extensions -------------------------------------------------------------------------

  #extensions(settings: Settings, readonly: boolean): Extension {
    const keys: KeyBinding[] = [
      ...withoutKept(closeBracketsKeymap),
      ...withoutKept(defaultKeymap),
      ...withoutKept(searchKeymap),
      ...withoutKept(historyKeymap),
      ...withoutKept(foldKeymap),
      ...withoutKept(completionKeymap),
      indentWithTab,
      // In place of the keys the terminal keeps: ⌘D, ⌥⌘↑ and ⌥⌘↓.
      { key: isMac ? "Ctrl-g" : "Mod-d", run: selectNextOccurrence, preventDefault: true },
      { key: "Ctrl-Shift-ArrowUp", run: addCursorAbove, preventDefault: true },
      { key: "Ctrl-Shift-ArrowDown", run: addCursorBelow, preventDefault: true },
    ];
    return [
      vimMode.of(settings.vim && this.#vim ? this.#vim.vim({ status: false }) : []),
      numbers.of(this.#numbersOf(settings)),
      foldGutter({ openText: "⌄", closedText: "›" }),
      highlightSpecialChars(),
      history(),
      drawSelection(),
      dropCursor(),
      EditorState.allowMultipleSelections.of(true),
      indentOnInput(),
      syntaxHighlighting(highlight, { fallback: true }),
      bracketMatching(),
      closeBrackets(),
      autocompletion({ icons: false }),
      rectangularSelection(),
      crosshairCursor(),
      highlightActiveLine(),
      highlightSelectionMatches(),
      search({ top: true, createPanel: createSearchPanel }),
      keymap.of(keys),
      language.of([]),
      wrapping.of(settings.wrap ? EditorView.lineWrapping : []),
      indentation.of(indentationOf(settings)),
      whitespace.of(
        settings.whitespace ? [highlightWhitespace(), highlightTrailingWhitespace()] : [],
      ),
      readOnly.of(EditorState.readOnly.of(readonly)),
      editorTheme,
      EditorView.contentAttributes.of({
        spellcheck: "false",
        autocorrect: "off",
        autocapitalize: "off",
        translate: "no",
      }),
      EditorView.updateListener.of((update) => this.#update(update)),
    ];
  }

  #update(update: ViewUpdate): void {
    const path = this.#shown;
    if (path === null || update.view !== this.view) return;
    if (update.docChanged) {
      const remote = update.transactions.some((tr) => tr.annotation(fromProgram));
      if (!remote) this.#changed(path, update.changes);
    }
    if (update.docChanged || update.selectionSet) {
      cancelAnimationFrame(this.#cursorFrame);
      this.#cursorFrame = requestAnimationFrame(() => this.#report());
      if (get().settings.lineNumbers === "relative") {
        const line = update.state.doc.lineAt(update.state.selection.main.head).number;
        if (line !== this.#numbersLine) {
          this.#numbersLine = line;
          queueMicrotask(() =>
            this.view?.dispatch({ effects: numbers.reconfigure(this.#numbersOf(get().settings)) }),
          );
        }
      }
    }
  }

  #report(): void {
    const view = this.view;
    if (!view || this.#shown === null) return;
    const state = view.state;
    const main = state.selection.main;
    const line = state.doc.lineAt(main.head);
    set({
      cursor: {
        line: line.number,
        column: main.head - line.from + 1,
        selections: state.selection.ranges.length,
        selected: state.selection.ranges.reduce((sum, r) => sum + (r.to - r.from), 0),
        lines: state.doc.lines,
      },
      language: this.languageOf(this.#shown),
    });
  }
}

function indentationOf(settings: Settings): Extension {
  return [
    EditorState.tabSize.of(settings.tabSize),
    indentUnit.of(settings.insertSpaces ? " ".repeat(settings.tabSize) : "\t"),
  ];
}

function messageOf(error: unknown): string {
  return error instanceof CallError || error instanceof Error ? error.message : String(error);
}

export const editor = new Controller();

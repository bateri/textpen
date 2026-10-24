/**
 * The status line: the branch, the keys that work where the keyboard is, and the file's
 * place, indentation, line ends and language — each a button to change it. Items give
 * way, the least needed first, as the pane narrows.
 */

import {
  IconArrowDown,
  IconArrowUp,
  IconCheck,
  IconGitBranch,
  IconHelpCircle,
  IconTerminal2,
} from "@tabler/icons-react";
import { useEffect, useState } from "react";

import type { Entry } from "../lib/types";
import { label } from "../lib/keys";
import {
  create,
  openFile,
  remove,
  rename,
  runSearch,
  save,
  selectedEntry,
  setEol,
  setExpanded,
} from "../state/actions";
import { closeAndRemember, find, focusEditor, openPalette, run } from "../state/commands";
import { get, set, useStore } from "../state/store";
import { entryMenu } from "./FileTree";

type Hint = [key: string, text: string, run: (at: { x: number; y: number }) => void];

/**
 * The keys that work where the keyboard is, each also a button: a click does what the key
 * does. The tree's act on its chosen row.
 */
function hints(
  focus: "editor" | "sidebar",
  sidebar: "files" | "search",
  hasFile: boolean,
  vim: boolean,
): Hint[] {
  if (focus === "sidebar" && sidebar === "files") {
    // The row is the tree's choice when the hint is clicked, not when the line was drawn.
    const onRow = (act: (entry: Entry) => void) => () => {
      const entry = selectedEntry();
      if (entry) act(entry);
    };
    return [
      [
        "⏎",
        "open",
        onRow((entry) => {
          if (entry.kind === "dir") setExpanded(entry.path, !get().expanded.includes(entry.path));
          else void openFile(entry.path, { focus: true });
        }),
      ],
      ["a", "new", () => void create("file")],
      ["r", "rename", onRow((entry) => void rename(entry.path))],
      ["d", "delete", onRow((entry) => void remove(entry.path, entry.kind === "dir"))],
      [".", "menu", (at) => entryMenu(selectedEntry(), { x: at.x, y: at.y - 8 })],
      ["Esc", "editor", focusEditor],
    ];
  }
  if (focus === "sidebar") {
    return [
      ["⏎", "search", () => void runSearch()],
      ["↓", "results", () => document.querySelector<HTMLElement>(".results")?.focus()],
      [
        label("Mod-Alt-r"),
        "regex",
        () => {
          set((s) => ({ search: { ...s.search, regex: !s.search.regex } }));
          void runSearch();
        },
      ],
      ["Esc", "editor", focusEditor],
    ];
  }
  return [
    [label("Mod-k"), "files", () => openPalette("files")],
    ["F1", "commands", () => openPalette("commands")],
    ...(hasFile
      ? ([
          [label("Mod-s"), "save", () => void save()],
          [label("Mod-f"), "find", () => find(false)],
        ] satisfies Hint[])
      : []),
    ...((vim
      ? [[":w", "save", () => void save()]]
      : [
          [
            label("Ctrl-w"),
            "close",
            () => {
              const active = get().active;
              if (active) void closeAndRemember(active);
            },
          ],
        ]) satisfies Hint[]),
  ];
}

export function StatusBar() {
  const git = useStore((s) => s.git);
  const cursor = useStore((s) => s.cursor);
  const tab = useStore((s) => s.tabs.find((t) => t.path === s.active));
  const settings = useStore((s) => s.settings);
  const language = useStore((s) => s.language);
  const vimMode = useStore((s) => s.vimMode);
  const focus = useStore((s) => s.focus);
  const sidebar = useStore((s) => s.sidebar);
  const layout = useStore((s) => s.layout);
  const busy = useStore((s) => s.busy > 0);
  const savedAt = useStore((s) => (s.active ? s.savedAt[s.active] : undefined));
  const handoff = useStore((s) => s.boot?.handoff === true);
  const [flash, setFlash] = useState(false);

  useEffect(() => {
    if (savedAt === undefined || Date.now() - savedAt > 1000) return;
    setFlash(true);
    const timer = setTimeout(() => setFlash(false), 1400);
    return () => clearTimeout(timer);
  }, [savedAt]);

  const ready = tab?.status === "ready";
  const info = tab?.info;
  const wide = layout === "wide";

  return (
    // A click on the line changes something; the keyboard stays where it was.
    <footer className="status" role="status" onMouseDown={(e) => e.preventDefault()}>
      {git && (
        <span className="status-item" title={`Branch ${git.branch}`}>
          <IconGitBranch size={13} stroke={1.8} />
          <span className="ellipsis branch">{git.branch}</span>
          {git.ahead > 0 && (
            <span className="status-count">
              <IconArrowUp size={11} stroke={2} />
              {git.ahead}
            </span>
          )}
          {git.behind > 0 && (
            <span className="status-count">
              <IconArrowDown size={11} stroke={2} />
              {git.behind}
            </span>
          )}
        </span>
      )}
      {vimMode && (
        <span className="status-vim" data-mode={vimMode.split(" ")[0]}>
          {vimMode.toUpperCase()}
        </span>
      )}
      {busy && <span className="spinner small" aria-label="working" />}
      {flash && (
        <span className="status-saved">
          <IconCheck size={12} stroke={2.2} /> Saved
        </span>
      )}
      <span className="status-hints">
        {wide &&
          hints(focus, sidebar, ready, settings.vim).map(([key, text, act]) => (
            <button
              key={key + text}
              type="button"
              className="hint"
              tabIndex={-1}
              onClick={(event) => act({ x: event.clientX, y: event.clientY })}
            >
              <kbd>{key}</kbd>
              {text}
            </button>
          ))}
      </span>
      {ready && info && (
        <>
          <button
            type="button"
            className="status-button"
            onClick={() => openPalette("line")}
            title={`Go to line (${label("Mod-l")})`}
          >
            Ln {cursor.line}, Col {cursor.column}
            {cursor.selected > 0 && (
              <span className="faint">
                {" "}
                ({cursor.selected} selected
                {cursor.selections > 1 ? `, ${cursor.selections} cursors` : ""})
              </span>
            )}
          </button>
          {layout !== "narrow" && (
            <>
              <button
                type="button"
                className="status-button"
                onClick={() => openPalette("indent")}
                title="Indentation"
              >
                {settings.insertSpaces ? `Spaces: ${settings.tabSize}` : `Tab: ${settings.tabSize}`}
              </button>
              <span className="status-item faint">UTF-8{info.bom ? " BOM" : ""}</span>
              <button
                type="button"
                className="status-button"
                onClick={() => void setEol(tab.path, info.eol === "lf" ? "crlf" : "lf")}
                title="Line endings: click to change"
              >
                {info.eol.toUpperCase()}
              </button>
            </>
          )}
          <button
            type="button"
            className="status-button"
            onClick={() => openPalette("language")}
            title="Change language"
          >
            {language}
          </button>
        </>
      )}
      {handoff && layout !== "narrow" && (
        <button
          type="button"
          className="status-button icon"
          onClick={() => run("shell")}
          title={`Shell here (${label("Mod-j")})`}
        >
          <IconTerminal2 size={14} stroke={1.7} />
        </button>
      )}
      <button
        type="button"
        className="status-button icon"
        onClick={() => set({ overlay: { kind: "help" } })}
        title={`Keyboard shortcuts (${label("Shift-Mod-/")})`}
        aria-label="Keyboard shortcuts"
      >
        <IconHelpCircle size={14} stroke={1.7} />
      </button>
    </footer>
  );
}

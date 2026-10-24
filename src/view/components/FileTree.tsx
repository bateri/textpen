/**
 * The project's files, as a tree: what git says of each, which are open, which have
 * changes not saved. Worked with the keys as in a terminal file manager — j k h l and
 * the arrows to move, ⏎ to open, a to add, r to rename, d to delete — or the mouse.
 */

import { IconChevronRight, IconFolder, IconFolderOpen } from "@tabler/icons-react";
import { memo, useEffect, useMemo, useRef } from "react";

import type { Entry, GitState } from "../lib/types";
import { FileIcon, ancestors, dirname } from "../lib/files";
import { matches } from "../lib/keys";
import { create, copyPath, listDir, openFile, remove, rename, setExpanded } from "../state/actions";
import { focusEditor, openPalette } from "../state/commands";
import { get, set, useStore } from "../state/store";
import { openMenu } from "./Menu";
import { moveBy, useWindow } from "./ui";

const ROW = 24;
/** Never shown: the repository's own records and a Mac's folder notes. */
const HIDDEN = new Set([".git", ".DS_Store"]);
/** Shown, but quieter: what tools put there. */
const QUIET = new Set([
  "node_modules",
  "dist",
  "build",
  "target",
  ".next",
  "__pycache__",
  ".venv",
  "venv",
]);

interface Row {
  entry: Entry;
  depth: number;
  open: boolean;
  loading: boolean;
}

const GIT_LETTER: Record<GitState, string> = {
  modified: "M",
  added: "A",
  untracked: "U",
  deleted: "D",
  renamed: "R",
  conflict: "!",
};

const GIT_RANK: Record<GitState, number> = {
  conflict: 5,
  modified: 4,
  deleted: 3,
  renamed: 2,
  added: 1,
  untracked: 1,
};

/** The menu of a row of the tree, or of the tree itself (`entry` undefined). */
export function entryMenu(entry: Entry | undefined, at: { x: number; y: number }): void {
  const isDir = entry?.kind === "dir";
  openMenu(at, [
    {
      label: "New File…",
      keys: "a",
      run: () => void create("file", entry ? (isDir ? entry.path : dirname(entry.path)) : ""),
    },
    {
      label: "New Folder…",
      keys: "Shift-a",
      run: () => void create("dir", entry ? (isDir ? entry.path : dirname(entry.path)) : ""),
    },
    ...(entry
      ? [
          { separator: true as const },
          ...(isDir
            ? []
            : [
                {
                  label: "Open",
                  keys: "Enter",
                  run: () => void openFile(entry.path, { focus: true }),
                },
              ]),
          { label: "Rename…", keys: "r", run: () => void rename(entry.path) },
          {
            label: "Delete…",
            keys: "d",
            danger: true,
            run: () => void remove(entry.path, isDir),
          },
          { separator: true as const },
          { label: "Copy Path", keys: "y", run: () => void copyPath(entry.path, false) },
          {
            label: "Copy Absolute Path",
            keys: "Shift-y",
            run: () => void copyPath(entry.path, true),
          },
        ]
      : []),
  ]);
}

export function FileTree() {
  const listings = useStore((s) => s.listings);
  const errors = useStore((s) => s.listingErrors);
  const expanded = useStore((s) => s.expanded);
  const selection = useStore((s) => s.treeSelection);
  const active = useStore((s) => s.active);
  const git = useStore((s) => s.git);
  const tabs = useStore((s) => s.tabs);
  const focus = useStore((s) => s.focus);
  const rootName = useStore((s) => s.boot?.rootName ?? "");

  const rows = useMemo(() => {
    const out: Row[] = [];
    const open = new Set(expanded);
    const walk = (dir: string, depth: number) => {
      const listing = listings[dir];
      if (!listing) return;
      for (const entry of listing.entries) {
        if (HIDDEN.has(entry.name)) continue;
        const isOpen = entry.kind === "dir" && open.has(entry.path);
        out.push({ entry, depth, open: isOpen, loading: isOpen && !listings[entry.path] });
        if (isOpen) walk(entry.path, depth + 1);
      }
    };
    walk("", 0);
    return out;
  }, [listings, expanded]);

  // What git says of a directory: the strongest of what it says of the files in it.
  const dirStates = useMemo(() => {
    const states = new Map<string, GitState>();
    for (const [path, state] of Object.entries(git?.changes ?? {})) {
      for (const dir of ancestors(path)) {
        const known = states.get(dir);
        if (!known || GIT_RANK[state] > GIT_RANK[known]) states.set(dir, state);
      }
    }
    return states;
  }, [git]);

  const dirty = useMemo(
    () => new Set(tabs.filter((t) => t.info?.dirty).map((t) => t.path)),
    [tabs],
  );

  const index = rows.findIndex((r) => r.entry.path === selection);
  const list = useWindow(rows.length, ROW);
  const focused = focus === "sidebar";
  const element = list.ref;

  useEffect(() => {
    if (focused && document.activeElement !== element.current) element.current?.focus();
  }, [focused, element]);

  // The chosen row is brought into sight when the choice changes, or when its folder has
  // been read and it is there to show; scrolling the tree by hand leaves it where it is.
  const { reveal } = list;
  const found = index >= 0;
  useEffect(() => {
    if (found) reveal(index);
    // `index` is left out on purpose: rows added above the chosen one move it, not the view.
  }, [selection, found, reveal]);

  const choose = (at: number) => {
    const row = rows[at];
    if (row) set({ treeSelection: row.entry.path });
  };

  const activateRow = (row: Row, focusEditorAfter: boolean) => {
    if (row.entry.kind === "dir") setExpanded(row.entry.path, !row.open);
    else void openFile(row.entry.path, { focus: focusEditorAfter });
  };

  const menuFor = (row: Row | undefined, at: { x: number; y: number }) => entryMenu(row?.entry, at);

  const onKeyDown = (event: React.KeyboardEvent) => {
    const native = event.nativeEvent;
    if (native.metaKey || native.altKey) return;
    if (native.ctrlKey && !["n", "p"].includes(event.key)) return;
    const row = rows[index];
    const next = moveBy(
      event,
      index,
      rows.length,
      Math.max(1, Math.floor((element.current?.clientHeight ?? 240) / ROW) - 1),
    );
    const take = () => {
      event.preventDefault();
      event.stopPropagation();
    };
    if (next !== undefined) {
      take();
      choose(next);
      return;
    }
    switch (event.key) {
      case "ArrowRight":
      case "l":
        take();
        if (!row) return;
        if (row.entry.kind === "dir") {
          if (!row.open) setExpanded(row.entry.path, true);
          else if (rows[index + 1]?.depth === row.depth + 1) choose(index + 1);
        } else {
          void openFile(row.entry.path, { focus: false });
        }
        return;
      case "ArrowLeft":
      case "h":
        take();
        if (!row) return;
        if (row.entry.kind === "dir" && row.open) setExpanded(row.entry.path, false);
        else {
          const parent = dirname(row.entry.path);
          if (parent !== "") set({ treeSelection: parent });
        }
        return;
      case "Enter":
      case "o":
        take();
        if (row) activateRow(row, true);
        return;
      case " ":
        take();
        if (row) activateRow(row, false);
        return;
      case "a":
        take();
        void create("file");
        return;
      case "A":
        take();
        void create("dir");
        return;
      case "r":
        take();
        if (row) void rename(row.entry.path);
        return;
      case "d":
      case "Delete":
      case "Backspace":
        take();
        if (row) void remove(row.entry.path, row.entry.kind === "dir");
        return;
      case "y":
      case "Y":
        take();
        if (row) void copyPath(row.entry.path, event.key === "Y");
        return;
      case "R":
        take();
        void listDir("");
        for (const dir of get().expanded) void listDir(dir);
        return;
      case "/":
        take();
        openPalette("files");
        return;
      case ".":
      case "ContextMenu":
        take();
        if (row) {
          const rect = element.current?.getBoundingClientRect();
          menuFor(row, {
            x: (rect?.left ?? 0) + 40 + row.depth * 12,
            y: (rect?.top ?? 0) + (index + 1) * ROW - (element.current?.scrollTop ?? 0),
          });
        }
        return;
      case "Escape":
        take();
        focusEditor();
        return;
    }
    if (matches(native, "Shift-F10")) {
      take();
      if (row) menuFor(row, { x: 60, y: 120 });
    }
  };

  const rootListing = listings[""];
  const rootError = errors[""];

  return (
    <div
      className="tree"
      ref={list.ref}
      role="tree"
      aria-label={`Files of ${rootName}`}
      tabIndex={0}
      data-focused={focused}
      onKeyDown={onKeyDown}
      onFocus={() => {
        if (get().focus !== "sidebar") set({ focus: "sidebar" });
        if (get().treeSelection === null && rows[0]) set({ treeSelection: rows[0].entry.path });
      }}
      onContextMenu={(event) => {
        if (event.target === event.currentTarget) {
          event.preventDefault();
          menuFor(undefined, { x: event.clientX, y: event.clientY });
        }
      }}
    >
      {rootError ? (
        <div className="tree-note bad">{rootError}</div>
      ) : !rootListing ? (
        <div className="tree-note">
          <span className="spinner" /> Reading the folder…
        </div>
      ) : rows.length === 0 ? (
        <div className="tree-note">
          This folder is empty.{" "}
          <button type="button" className="link" onClick={() => void create("file", "")}>
            Make a file
          </button>
        </div>
      ) : (
        <div className="tree-body" style={{ height: list.total }}>
          {rows.slice(list.start, list.end).map((row, i) => {
            const at = list.start + i;
            const path = row.entry.path;
            const state = row.entry.kind === "dir" ? dirStates.get(path) : git?.changes[path];
            return (
              <TreeRow
                key={path}
                row={row}
                top={at * ROW}
                selected={at === index}
                focused={focused}
                active={path === active}
                git={state}
                dirty={dirty.has(path)}
                error={errors[path]}
                onClick={(event) => {
                  set({ treeSelection: path, focus: "sidebar" });
                  // One click chooses the row and gives the tree the keys (r, d, . act on
                  // it); a second click goes to the file. A drawer closes on a file.
                  const toEditor = event.detail > 1 || get().layout === "narrow";
                  if (!toEditor) list.ref.current?.focus();
                  activateRow(row, toEditor);
                }}
                onMenu={(event) => {
                  event.preventDefault();
                  set({ treeSelection: path });
                  menuFor(row, { x: event.clientX, y: event.clientY });
                }}
              />
            );
          })}
        </div>
      )}
    </div>
  );
}

const TreeRow = memo(function TreeRow({
  row,
  top,
  selected,
  focused,
  active,
  git,
  dirty,
  error,
  onClick,
  onMenu,
}: {
  row: Row;
  top: number;
  selected: boolean;
  focused: boolean;
  active: boolean;
  git: GitState | undefined;
  dirty: boolean;
  error: string | undefined;
  onClick: (event: React.MouseEvent) => void;
  onMenu: (event: React.MouseEvent) => void;
}) {
  const { entry, depth, open, loading } = row;
  const isDir = entry.kind === "dir";
  return (
    <div
      className="tree-row"
      role="treeitem"
      aria-selected={selected}
      aria-expanded={isDir ? open : undefined}
      aria-level={depth + 1}
      data-selected={selected}
      data-focused={selected && focused}
      data-active={active}
      data-git={git}
      data-quiet={QUIET.has(entry.name) || (entry.name.startsWith(".") && !git)}
      style={{ top, paddingLeft: 8 + depth * 12 }}
      title={error ?? entry.path}
      onClick={onClick}
      onContextMenu={onMenu}
      onMouseDown={(e) => e.preventDefault()}
    >
      {Array.from({ length: depth }, (_, i) => (
        <span key={i} className="tree-guide" style={{ left: 14 + i * 12 }} />
      ))}
      <span className="tree-chevron" data-open={open} data-hidden={!isDir}>
        {isDir && <IconChevronRight size={13} stroke={2} />}
      </span>
      <span className="tree-icon">
        {isDir ? (
          open ? (
            <IconFolderOpen size={15} stroke={1.6} className="folder-icon" />
          ) : (
            <IconFolder size={15} stroke={1.6} className="folder-icon" />
          )
        ) : (
          <FileIcon path={entry.path} />
        )}
      </span>
      <span className="tree-name ellipsis">
        {entry.name}
        {entry.link && <span className="tree-link"> ↗</span>}
      </span>
      {loading && <span className="spinner small" />}
      {dirty && <span className="dirty-dot" aria-label="not saved" />}
      {git && !isDir && <span className="git-letter">{GIT_LETTER[git]}</span>}
      {git && isDir && <span className="git-dot" />}
    </div>
  );
});

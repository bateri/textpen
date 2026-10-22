/**
 * Search in the project's files: the program reads them, the view lists what matched,
 * by file. ⏎ in the field searches; ↓ goes to the results, where ↑ ↓ choose, ⏎ opens the
 * match and goes to it, Space opens it and stays, ← → fold a file's matches.
 * F4 and ⇧F4 go through them from anywhere.
 */

import { IconChevronRight, IconLetterCase, IconRegex, IconSearch } from "@tabler/icons-react";
import { useEffect, useMemo, useRef } from "react";

import { FileIcon, basename, dirname } from "../lib/files";
import { matches } from "../lib/keys";
import { openMatch, runSearch } from "../state/actions";
import { focusEditor } from "../state/commands";
import { get, set, useStore } from "../state/store";
import type { SearchMatch } from "../../server/types";
import type { SearchState } from "../state/store";
import { moveBy, useWindow } from "./ui";

const ROW = 24;

type Row =
  | { kind: "file"; file: number; path: string; count: number; collapsed: boolean }
  | { kind: "match"; file: number; match: number };

/** Where a match's line is shown from: a little before the match, so that it is in sight. */
function cut(match: SearchMatch): number {
  const before = match.text.slice(0, match.start);
  const indent = before.length - before.trimStart().length;
  return match.start - indent > 18 ? match.start - 12 : 0;
}

function update(change: Partial<SearchState>): void {
  set((s) => ({ search: { ...s.search, ...change } }));
}

export function ProjectSearch() {
  const search = useStore((s) => s.search);
  const focused = useStore((s) => s.focus === "sidebar" && s.sidebar === "search");
  const field = useRef<HTMLInputElement>(null);
  const results = useRef<HTMLDivElement>(null);
  const timer = useRef<ReturnType<typeof setTimeout>>(undefined);

  useEffect(() => {
    if (focused && !results.current?.contains(document.activeElement)) {
      field.current?.focus();
      field.current?.select();
    }
  }, [focused]);

  const schedule = (change: Partial<SearchState>) => {
    update(change);
    clearTimeout(timer.current);
    timer.current = setTimeout(() => void runSearch(), 280);
  };

  const toggles = [
    {
      key: "Mod-Alt-c",
      title: "Match case",
      on: search.caseSensitive,
      flip: () => schedule({ caseSensitive: !search.caseSensitive }),
      icon: <IconLetterCase size={15} stroke={1.7} />,
    },
    {
      key: "Mod-Alt-w",
      title: "Whole words",
      on: search.wholeWord,
      flip: () => schedule({ wholeWord: !search.wholeWord }),
      icon: <span className="word-glyph">ab</span>,
    },
    {
      key: "Mod-Alt-r",
      title: "Regular expression",
      on: search.regex,
      flip: () => schedule({ regex: !search.regex }),
      icon: <IconRegex size={15} stroke={1.7} />,
    },
  ];

  const result = search.result;
  return (
    <div className="search-side">
      <div className="search-form">
        <label className="field">
          <IconSearch size={14} stroke={1.8} className="field-icon" />
          <input
            ref={field}
            value={search.query}
            placeholder="Search in files"
            aria-label="Search in files"
            spellCheck={false}
            autoComplete="off"
            onChange={(e) => schedule({ query: e.target.value })}
            onFocus={() => set({ focus: "sidebar", sidebar: "search" })}
            onKeyDown={(event) => {
              const native = event.nativeEvent;
              for (const toggle of toggles) {
                if (matches(native, toggle.key)) {
                  event.preventDefault();
                  toggle.flip();
                  return;
                }
              }
              if (event.key === "Enter") {
                event.preventDefault();
                clearTimeout(timer.current);
                if (event.metaKey || event.ctrlKey) openMatch(0);
                else void runSearch();
              } else if (event.key === "ArrowDown" && result && result.files.length > 0) {
                event.preventDefault();
                results.current?.focus();
              } else if (event.key === "Escape") {
                event.preventDefault();
                focusEditor();
              }
            }}
          />
          <span className="find-toggles">
            {toggles.map((toggle) => (
              <button
                key={toggle.key}
                type="button"
                className="toggle"
                aria-pressed={toggle.on}
                title={`${toggle.title}`}
                onClick={toggle.flip}
                onMouseDown={(e) => e.preventDefault()}
              >
                {toggle.icon}
              </button>
            ))}
          </span>
        </label>
        <div className="search-summary" aria-live="polite">
          {search.running ? (
            <>
              <span className="spinner small" /> Searching…
            </>
          ) : search.error ? (
            <span className="bad">{search.error}</span>
          ) : result ? (
            result.matches === 0 ? (
              <span>No results in {result.searched} files</span>
            ) : (
              <span>
                {result.matches}
                {result.truncated ? "+" : ""} results in {result.files.length} files
                <span className="faint"> · {result.ms} ms</span>
              </span>
            )
          ) : (
            <span className="faint">⏎ searches · ↓ goes to the results</span>
          )}
        </div>
      </div>
      {result && result.files.length > 0 && <Results resultsRef={results} search={search} />}
    </div>
  );
}

function Results({
  search,
  resultsRef,
}: {
  search: SearchState;
  resultsRef: React.RefObject<HTMLDivElement | null>;
}) {
  const result = search.result!;
  const collapsed = useMemo(() => new Set(search.collapsed), [search.collapsed]);
  const rows = useMemo(() => {
    const out: Row[] = [];
    result.files.forEach((file, index) => {
      const isCollapsed = collapsed.has(file.path);
      out.push({
        kind: "file",
        file: index,
        path: file.path,
        count: file.matches.length,
        collapsed: isCollapsed,
      });
      if (!isCollapsed)
        file.matches.forEach((_, match) => out.push({ kind: "match", file: index, match }));
    });
    return out;
  }, [result, collapsed]);
  const chosen = search.chosen;
  const index = rows.findIndex((r) =>
    chosen === null
      ? false
      : r.kind === "match"
        ? r.file === chosen.file && r.match === chosen.match
        : r.file === chosen.file && chosen.match === -1,
  );
  const list = useWindow(rows.length, ROW);

  // As in the tree: the chosen result is brought into sight when the choice changes, not
  // each time the list is scrolled by hand.
  const { reveal } = list;
  const found = index >= 0;
  useEffect(() => {
    if (found) reveal(index);
  }, [chosen?.file, chosen?.match, found, reveal]);

  const choose = (row: Row | undefined) => {
    if (!row) return;
    update({ chosen: { file: row.file, match: row.kind === "match" ? row.match : -1 } });
  };

  const fold = (path: string, value: boolean) =>
    update({
      collapsed: value ? [...search.collapsed, path] : search.collapsed.filter((p) => p !== path),
    });

  return (
    <div
      className="results"
      role="tree"
      aria-label="Search results"
      tabIndex={0}
      ref={(element) => {
        list.ref.current = element!;
        resultsRef.current = element;
      }}
      onFocus={() => {
        if (index < 0) choose(rows[0]);
      }}
      onKeyDown={(event) => {
        const row = rows[index];
        const next = moveBy(event, index, rows.length, 12);
        const take = () => {
          event.preventDefault();
          event.stopPropagation();
        };
        if (next !== undefined) {
          take();
          choose(rows[next]);
          return;
        }
        if (!row) return;
        if (event.key === "Enter" || event.key === " ") {
          take();
          if (row.kind === "file") fold(row.path, !row.collapsed);
          else openMatch(0, event.key === "Enter");
        } else if (event.key === "ArrowLeft" || event.key === "h") {
          take();
          const path = result.files[row.file]!.path;
          fold(path, true);
          update({ chosen: { file: row.file, match: -1 } });
        } else if (event.key === "ArrowRight" || event.key === "l") {
          take();
          if (row.kind === "file") fold(row.path, false);
        } else if (event.key === "Escape") {
          take();
          (document.querySelector(".search-form input") as HTMLInputElement | null)?.focus();
        }
      }}
    >
      <div className="tree-body" style={{ height: list.total }}>
        {rows.slice(list.start, list.end).map((row, i) => {
          const at = list.start + i;
          const top = at * ROW;
          if (row.kind === "file") {
            return (
              <div
                key={`f:${row.path}`}
                className="result-file tree-row"
                role="treeitem"
                aria-expanded={!row.collapsed}
                data-selected={at === index}
                style={{ top }}
                onMouseDown={(e) => e.preventDefault()}
                onClick={() => {
                  choose(row);
                  fold(row.path, !row.collapsed);
                }}
              >
                <span className="tree-chevron" data-open={!row.collapsed}>
                  <IconChevronRight size={13} stroke={2} />
                </span>
                <span className="tree-icon">
                  <FileIcon path={row.path} />
                </span>
                <span className="ellipsis">
                  <span className="tree-name">{basename(row.path)}</span>
                  <span className="faint"> {dirname(row.path)}</span>
                </span>
                <span className="count">{row.count}</span>
              </div>
            );
          }
          const match = result.files[row.file]!.matches[row.match]!;
          return (
            <div
              key={`m:${row.file}:${row.match}`}
              className="result-match tree-row"
              role="treeitem"
              data-selected={at === index}
              style={{ top }}
              title={`Line ${match.line}`}
              onMouseDown={(e) => e.preventDefault()}
              onClick={(event) => {
                choose(row);
                set((s) => ({
                  search: { ...s.search, chosen: { file: row.file, match: row.match } },
                }));
                openMatch(0, event.detail > 1 || get().layout === "narrow");
              }}
            >
              <span className="line-number">{match.line}</span>
              <span className="ellipsis match-text">
                {(match.offset > 0 || cut(match) > 0) && <span className="faint">…</span>}
                {match.text.slice(cut(match), match.start).trimStart()}
                <mark>{match.text.slice(match.start, match.end)}</mark>
                {match.text.slice(match.end)}
              </span>
            </div>
          );
        })}
      </div>
    </div>
  );
}

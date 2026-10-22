/**
 * The palette: one field for going anywhere. ⌘K finds a file (`name:12` at a line);
 * typed `>` or ⌘K again lists the commands, `:` goes to a line; ⌘E lists the open files
 * by when they were seen. ↑ ↓ (⌃N ⌃P) choose, ⏎ goes, Esc comes back, ⌫ on an empty
 * field goes back to the files.
 */

import { EditorSelection } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import { IconCheck, IconCommand, IconFileSearch, IconHash, IconStack2 } from "@tabler/icons-react";
import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import type { ReactNode } from "react";

import { editor } from "../editor/controller";
import { LANGUAGES } from "../editor/languages";
import { FileIcon, basename, dirname } from "../lib/files";
import { fuzzy, fuzzyPath } from "../lib/fuzzy";
import { matches } from "../lib/keys";
import { changeSettings, fileIndex, openFile } from "../state/actions";
import { available } from "../state/commands";
import { get, set, useStore } from "../state/store";
import type { PaletteMode } from "../state/store";
import { Highlighted, Keys, moveBy } from "./ui";

interface Item {
  id: string;
  title: ReactNode;
  detail?: ReactNode;
  icon?: ReactNode;
  keys?: string;
  badge?: ReactNode;
  current?: boolean;
  run: () => void;
}

const MODES: Record<PaletteMode, { label: string; icon: ReactNode; placeholder: string }> = {
  files: {
    label: "Files",
    icon: <IconFileSearch size={14} stroke={1.7} />,
    placeholder: "Go to file…  (> commands, : line)",
  },
  commands: {
    label: "Commands",
    icon: <IconCommand size={14} stroke={1.7} />,
    placeholder: "Run a command…",
  },
  line: {
    label: "Line",
    icon: <IconHash size={14} stroke={1.7} />,
    placeholder: "Line number, or line:column",
  },
  tabs: {
    label: "Open",
    icon: <IconStack2 size={14} stroke={1.7} />,
    placeholder: "Switch to an open file…",
  },
  language: {
    label: "Language",
    icon: <IconCommand size={14} stroke={1.7} />,
    placeholder: "Read this file as…",
  },
  indent: {
    label: "Indent",
    icon: <IconCommand size={14} stroke={1.7} />,
    placeholder: "Indentation…",
  },
};

const LIMIT = 120;
const recentCommands: string[] = [];

function close(): void {
  set({ overlay: null });
}

/**
 * After the palette: what was run may have put the keyboard somewhere (a dialog, the
 * tree, the editor); if it did not, the editor has it again, not the page.
 */
export function regainFocus(): void {
  requestAnimationFrame(() => {
    const state = get();
    if (state.overlay !== null || state.handingOff !== null) return;
    const active = document.activeElement;
    if (active === null || active === document.body) {
      if (state.focus === "sidebar")
        (document.querySelector(".tree, .search-form input") as HTMLElement | null)?.focus();
      else editor.focus();
    }
  });
}

export function PaletteLayer() {
  const overlay = useStore((s) => s.overlay);
  if (overlay?.kind !== "palette") return null;
  return <Palette mode={overlay.mode} initial={overlay.query} />;
}

function Palette({ mode, initial }: { mode: PaletteMode; initial: string }) {
  const [query, setQuery] = useState(initial);
  const [index, setIndex] = useState(mode === "tabs" ? 1 : 0);
  const field = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLDivElement>(null);
  const index$ = useStore((s) => s.fileIndex);
  const tabs = useStore((s) => s.tabs);
  const recent = useStore((s) => s.recent);
  const git = useStore((s) => s.git);
  const settings = useStore((s) => s.settings);
  const active = useStore((s) => s.active);

  // Go to Line moves the view as it is typed; Esc puts it back.
  const origin = useRef<{ anchor: number; head: number; top: number } | null>(null);

  useLayoutEffect(() => {
    field.current?.focus();
    if (mode === "files") void fileIndex();
    if (mode === "line") origin.current = editor.place() ?? null;
  }, [mode]);

  useEffect(() => {
    setQuery(initial);
    setIndex(mode === "tabs" ? 1 : 0);
  }, [mode, initial]);

  const switchMode = (next: PaletteMode, text = "") =>
    set({ overlay: { kind: "palette", mode: next, query: text } });

  const items = useMemo<Item[]>(() => {
    switch (mode) {
      case "files":
        return fileItems(
          query,
          index$?.files ?? [],
          tabs.map((t) => t.path),
          recent,
          git?.changes ?? {},
        );
      case "commands":
        return commandItems(query);
      case "tabs":
        return tabItems(
          query,
          tabs.map((t) => t.path),
          recent,
          active,
        );
      case "line":
        return lineItems(query);
      case "language":
        return languageItems(query, active);
      case "indent":
        return indentItems(query, settings.insertSpaces, settings.tabSize);
    }
  }, [mode, query, index$, tabs, recent, git, active, settings.insertSpaces, settings.tabSize]);

  const chosen = Math.min(index, Math.max(0, items.length - 1));

  useEffect(() => {
    listRef.current?.querySelector('[data-chosen="true"]')?.scrollIntoView({ block: "nearest" });
  }, [chosen, items]);

  // The line under the field, shown as it is typed.
  useEffect(() => {
    if (mode !== "line") return;
    const view = editor.view;
    const target = parseLine(query);
    if (!view || !target) return;
    const line = view.state.doc.line(Math.min(Math.max(1, target.line), view.state.doc.lines));
    view.dispatch({ effects: EditorView.scrollIntoView(line.from, { y: "center" }) });
  }, [mode, query]);

  const dismiss = () => {
    if (mode === "line" && origin.current && editor.view) {
      editor.view.dispatch({
        effects: EditorView.scrollIntoView(origin.current.top, { y: "start" }),
      });
    }
    close();
    if (get().focus === "editor") requestAnimationFrame(() => editor.focus());
  };

  const runItem = (item: Item | undefined) => {
    if (!item) return;
    close();
    item.run();
    regainFocus();
  };

  const info = MODES[mode];
  return (
    <div className="scrim" onMouseDown={dismiss}>
      <div
        className="palette pop"
        role="dialog"
        aria-label={info.label}
        onMouseDown={(e) => e.stopPropagation()}
      >
        <div className="palette-field">
          <span className="palette-mode">
            {info.icon}
            <span>{info.label}</span>
          </span>
          <input
            ref={field}
            value={query}
            placeholder={info.placeholder}
            aria-label={info.placeholder}
            aria-controls="palette-list"
            aria-activedescendant={items[chosen] ? `palette-${chosen}` : undefined}
            spellCheck={false}
            autoComplete="off"
            onChange={(event) => {
              const value = event.target.value;
              if (mode === "files" && value.startsWith(">"))
                switchMode("commands", value.slice(1).trimStart());
              else if (mode === "files" && value.startsWith(":") && editor.shown)
                switchMode("line", value.slice(1));
              else {
                setQuery(value);
                setIndex(0);
              }
            }}
            onKeyDown={(event) => {
              const native = event.nativeEvent;
              if (matches(native, "Mod-k")) {
                event.preventDefault();
                switchMode(mode === "files" ? "commands" : "files");
                return;
              }
              if (event.key === "Escape") {
                event.preventDefault();
                dismiss();
                return;
              }
              if (event.key === "Backspace" && query === "" && mode !== "files") {
                event.preventDefault();
                switchMode("files");
                return;
              }
              if (event.key === "Enter") {
                event.preventDefault();
                runItem(items[chosen]);
                return;
              }
              if (matches(native, "Mod-e") && mode === "tabs") {
                event.preventDefault();
                setIndex((chosen + 1) % Math.max(1, items.length));
                return;
              }
              const next = moveBy(event, chosen, items.length, 8, false);
              if (next !== undefined && !["Home", "End"].includes(event.key)) {
                event.preventDefault();
                setIndex(next);
              }
            }}
          />
        </div>
        <div className="palette-list" id="palette-list" role="listbox" ref={listRef}>
          {items.length === 0 ? (
            <div className="palette-empty">
              {mode === "files" && index$ === null ? (
                <>
                  <span className="spinner small" /> Listing the project…
                </>
              ) : mode === "line" ? (
                "Type a line number"
              ) : (
                "Nothing matches"
              )}
            </div>
          ) : (
            items.map((item, i) => (
              <div
                key={item.id}
                id={`palette-${i}`}
                role="option"
                aria-selected={i === chosen}
                className="palette-item"
                data-chosen={i === chosen}
                onMouseMove={() => i !== chosen && setIndex(i)}
                onClick={() => runItem(item)}
              >
                {item.icon && <span className="palette-icon">{item.icon}</span>}
                <span className="palette-title ellipsis">
                  {item.title}
                  {item.detail && <span className="palette-detail"> {item.detail}</span>}
                </span>
                {item.badge}
                {item.current && <IconCheck size={14} stroke={2} className="palette-current" />}
                {item.keys && <Keys spec={item.keys} quiet />}
              </div>
            ))
          )}
        </div>
        <div className="palette-foot" aria-hidden>
          <span>
            <kbd>↑</kbd>
            <kbd>↓</kbd> choose
          </span>
          <span>
            <kbd>⏎</kbd> {mode === "commands" ? "run" : "go"}
          </span>
          {(mode === "files" || mode === "commands") && (
            <span>
              <Keys spec="Mod-k" quiet /> {mode === "files" ? "commands" : "files"}
            </span>
          )}
          <span>
            <kbd>Esc</kbd> close
          </span>
        </div>
      </div>
    </div>
  );
}

// ---- the lists ---------------------------------------------------------------------------------

const GIT_LETTER: Record<string, string> = {
  modified: "M",
  added: "A",
  untracked: "U",
  deleted: "D",
  renamed: "R",
  conflict: "!",
};

function fileItems(
  raw: string,
  files: readonly string[],
  open: readonly string[],
  recent: readonly string[],
  changes: Record<string, string>,
): Item[] {
  // `name:12` and `name:12:4` go to a line.
  const place = /^(.*?):(\d+)(?::(\d+))?$/.exec(raw.trim());
  const query = place ? place[1]! : raw;
  const line = place ? Number(place[2]) : undefined;
  const column = place?.[3] ? Number(place[3]) : undefined;
  const all = [
    ...new Set([...recent.filter((p) => open.includes(p) || files.includes(p)), ...open, ...files]),
  ];
  const item = (path: string, positions: readonly number[] = []): Item => {
    const nameStart = path.length - basename(path).length;
    const state = changes[path];
    return {
      id: path,
      icon: <FileIcon path={path} />,
      title: (
        <Highlighted
          text={basename(path)}
          positions={positions.filter((p) => p >= nameStart)}
          offset={nameStart}
        />
      ),
      detail: (
        <Highlighted text={dirname(path)} positions={positions.filter((p) => p < nameStart)} />
      ),
      badge: (
        <>
          {open.includes(path) && <span className="palette-badge">open</span>}
          {state && (
            <span className="git-letter" data-git={state}>
              {GIT_LETTER[state]}
            </span>
          )}
        </>
      ),
      run: () =>
        void openFile(path, {
          focus: true,
          ...(line ? { line } : {}),
          ...(column ? { column } : {}),
        }),
    };
  };
  if (query.trim() === "") {
    // Nothing typed: what was open lately, then the rest.
    return all.slice(0, LIMIT).map((path) => item(path));
  }
  const recentRank = new Map(recent.map((p, i) => [p, i]));
  const scored: { path: string; score: number; positions: number[] }[] = [];
  for (const path of all) {
    const match = fuzzyPath(query, path);
    if (!match) continue;
    const bonus = recentRank.has(path) ? 12 - Math.min(10, recentRank.get(path)!) : 0;
    scored.push({ path, score: match.score + bonus, positions: match.positions });
  }
  scored.sort((a, b) => b.score - a.score || a.path.length - b.path.length);
  return scored.slice(0, LIMIT).map((s) => item(s.path, s.positions));
}

function commandItems(query: string): Item[] {
  const commands = available();
  const scored = commands
    .map((command) => {
      const text = `${command.category}: ${command.title}`;
      const match = fuzzy(query, text);
      return match ? { command, match, text } : null;
    })
    .filter((x) => x !== null);
  if (query.trim() === "") {
    // Nothing typed: the commands run lately first, then by kind.
    const rank = (id: string) => {
      const at = recentCommands.indexOf(id);
      return at < 0 ? 1000 : at;
    };
    scored.sort((a, b) => rank(a.command.id) - rank(b.command.id));
  } else {
    scored.sort((a, b) => b.match.score - a.match.score);
  }
  return scored.slice(0, LIMIT).map(({ command, match, text }) => {
    const split = command.category.length + 2;
    return {
      id: command.id,
      title: (
        <>
          <span className="palette-category">
            <Highlighted
              text={`${command.category}: `}
              positions={match.positions.filter((p) => p < split)}
            />
          </span>
          <Highlighted
            text={text.slice(split)}
            positions={match.positions.filter((p) => p >= split)}
            offset={split}
          />
        </>
      ),
      ...(command.keys?.[0] ? { keys: command.keys[0] } : {}),
      run: () => {
        const at = recentCommands.indexOf(command.id);
        if (at >= 0) recentCommands.splice(at, 1);
        recentCommands.unshift(command.id);
        // The editor has the keyboard back before an editing command runs on it.
        if (command.editor) editor.focus();
        void command.run();
      },
    };
  });
}

function tabItems(
  query: string,
  open: readonly string[],
  recent: readonly string[],
  active: string | null,
): Item[] {
  const ordered = [
    ...new Set([...(active ? [active] : []), ...recent.filter((p) => open.includes(p)), ...open]),
  ];
  return ordered
    .map((path) => ({ path, match: fuzzyPath(query, path) }))
    .filter((x) => x.match !== undefined)
    .map(({ path, match }) => {
      const nameStart = path.length - basename(path).length;
      const tab = get().tabs.find((t) => t.path === path);
      return {
        id: path,
        icon: <FileIcon path={path} />,
        title: (
          <Highlighted
            text={basename(path)}
            positions={match!.positions.filter((p) => p >= nameStart)}
            offset={nameStart}
          />
        ),
        detail: dirname(path),
        current: path === active,
        badge: tab?.info?.dirty ? <span className="dirty-dot" aria-label="not saved" /> : undefined,
        run: () => void openFile(path, { focus: true }),
      };
    });
}

function parseLine(query: string): { line: number; column?: number } | null {
  const match = /^\s*(\d+)(?:\s*[:,]\s*(\d+))?\s*$/.exec(query);
  if (!match) return null;
  return { line: Number(match[1]), ...(match[2] ? { column: Number(match[2]) } : {}) };
}

function lineItems(query: string): Item[] {
  const view = editor.view;
  const target = parseLine(query);
  if (!view || !target) return [];
  const lines = view.state.doc.lines;
  const line = Math.min(Math.max(1, target.line), lines);
  return [
    {
      id: "line",
      title: `Go to line ${line}${target.column ? `, column ${target.column}` : ""}`,
      detail: `of ${lines}`,
      run: () => {
        const doc = view.state.doc.line(line);
        const at = Math.min(doc.from + Math.max(0, (target.column ?? 1) - 1), doc.to);
        view.dispatch({
          selection: EditorSelection.cursor(at),
          effects: EditorView.scrollIntoView(at, { y: "center" }),
        });
        view.focus();
      },
    },
  ];
}

function languageItems(query: string, path: string | null): Item[] {
  if (path === null) return [];
  const current = editor.languageOf(path);
  const names = ["Plain Text", ...LANGUAGES.map((l) => l.name).sort((a, b) => a.localeCompare(b))];
  return names
    .map((name) => ({ name, match: fuzzy(query, name) }))
    .filter((x) => x.match !== undefined)
    .sort((a, b) => (query ? b.match!.score - a.match!.score : 0))
    .map(({ name, match }) => ({
      id: name,
      title: <Highlighted text={name} positions={match!.positions} />,
      current: name === current,
      run: () =>
        void editor
          .setLanguage(path, name === "Plain Text" ? null : name)
          .then(() => editor.focus()),
    }));
}

function indentItems(query: string, spaces: boolean, size: number): Item[] {
  const items: Item[] = [
    {
      id: "spaces",
      title: "Indent with spaces",
      current: spaces,
      run: () => changeSettings({ insertSpaces: true }),
    },
    {
      id: "tabs",
      title: "Indent with tabs",
      current: !spaces,
      run: () => changeSettings({ insertSpaces: false }),
    },
    ...[2, 4, 8, 3, 1].map((n) => ({
      id: `size-${n}`,
      title: `Tab size: ${n}`,
      current: size === n,
      run: () => changeSettings({ tabSize: n }),
    })),
  ];
  const q = query.trim().toLowerCase();
  return q === "" ? items : items.filter((i) => String(i.title).toLowerCase().includes(q));
}

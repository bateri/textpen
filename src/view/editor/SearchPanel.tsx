/**
 * Find and replace in the file shown: CodeMirror's search, with a panel of textpen's own
 * that counts the matches, moves to the first one as you type, and is worked with keys:
 * ⏎ and ⇧⏎ go through the matches, ⌥⌘C ⌥⌘W ⌥⌘R switch case, whole words and regular
 * expressions, ⌥⌘F shows the replacement, ⌘⏎ replaces all, Esc goes back to the text.
 */

import {
  SearchQuery,
  closeSearchPanel,
  findNext,
  findPrevious,
  getSearchQuery,
  replaceAll,
  replaceNext,
  selectMatches,
  setSearchQuery,
} from "@codemirror/search";
import type { EditorState } from "@codemirror/state";
import { EditorView, runScopeHandlers } from "@codemirror/view";
import type { Panel, ViewUpdate } from "@codemirror/view";
import {
  IconArrowDown,
  IconArrowUp,
  IconChevronRight,
  IconLetterCase,
  IconRegex,
  IconReplace,
  IconSelectAll,
  IconX,
} from "@tabler/icons-react";
import { useLayoutEffect, useRef, useState } from "react";
import { flushSync } from "react-dom";
import { createRoot } from "react-dom/client";

import { label, matches } from "../lib/keys";

const COUNT_LIMIT = 9999;

/** Whether the next panel opens with its replacement shown (⌥⌘F). */
let replaceWanted = false;

export function wantReplace(): void {
  replaceWanted = true;
}

interface Counted {
  doc: EditorState["doc"];
  query: SearchQuery;
  ranges: { from: number; to: number }[];
  more: boolean;
}

function count(state: EditorState, query: SearchQuery, last: Counted | null): Counted {
  if (last && last.doc === state.doc && last.query.eq(query)) return last;
  const ranges: { from: number; to: number }[] = [];
  let more = false;
  if (query.valid && query.search !== "") {
    const cursor = query.getCursor(state);
    for (let next = cursor.next(); !next.done; next = cursor.next()) {
      if (ranges.length >= COUNT_LIMIT) {
        more = true;
        break;
      }
      ranges.push(next.value);
    }
  }
  return { doc: state.doc, query, ranges, more };
}

function SearchBox({ view, tick, close }: { view: EditorView; tick: number; close: () => void }) {
  const query = getSearchQuery(view.state);
  const [showReplace, setShowReplace] = useState(() => {
    const wanted = replaceWanted;
    replaceWanted = false;
    return wanted || query.replace !== "";
  });
  const counted = useRef<Counted | null>(null);
  const origin = useRef(view.state.selection.main.from);
  const findField = useRef<HTMLInputElement>(null);
  const replaceField = useRef<HTMLInputElement>(null);
  void tick;

  counted.current = count(view.state, query, counted.current);
  const { ranges, more } = counted.current;
  const main = view.state.selection.main;
  const index = ranges.findIndex((r) => r.from === main.from && r.to === main.to);

  useLayoutEffect(() => {
    if (replaceWanted) {
      replaceWanted = false;
      setShowReplace(true);
      requestAnimationFrame(() => replaceField.current?.focus());
    }
  }, [tick]);

  const change = (spec: Partial<ConstructorParameters<typeof SearchQuery>[0]>, move = false) => {
    const next = new SearchQuery({
      search: query.search,
      caseSensitive: query.caseSensitive,
      regexp: query.regexp,
      wholeWord: query.wholeWord,
      replace: query.replace,
      ...spec,
    });
    view.dispatch({ effects: setSearchQuery.of(next) });
    if (!move || !next.valid || next.search === "") return;
    // As you type, the first match from where the search began is chosen.
    let cursor = next.getCursor(view.state, origin.current);
    let found = cursor.next();
    if (found.done) {
      cursor = next.getCursor(view.state);
      found = cursor.next();
    }
    if (!found.done) {
      view.dispatch({
        selection: { anchor: found.value.from, head: found.value.to },
        effects: EditorView.scrollIntoView(found.value.from, { y: "center" }),
      });
    }
  };

  const toggles = [
    {
      key: "Mod-Alt-c",
      title: "Match case",
      on: query.caseSensitive,
      icon: <IconLetterCase size={15} stroke={1.7} />,
      flip: () => change({ caseSensitive: !query.caseSensitive }, true),
    },
    {
      key: "Mod-Alt-w",
      title: "Whole words",
      on: query.wholeWord,
      icon: <span className="word-glyph">ab</span>,
      flip: () => change({ wholeWord: !query.wholeWord }, true),
    },
    {
      key: "Mod-Alt-r",
      title: "Regular expression",
      on: query.regexp,
      icon: <IconRegex size={15} stroke={1.7} />,
      flip: () => change({ regexp: !query.regexp }, true),
    },
  ];

  const onKeyDown = (event: React.KeyboardEvent, field: "find" | "replace") => {
    const native = event.nativeEvent;
    for (const toggle of toggles) {
      if (matches(native, toggle.key)) {
        event.preventDefault();
        toggle.flip();
        return;
      }
    }
    if (matches(native, "Mod-Alt-f")) {
      event.preventDefault();
      setShowReplace(true);
      requestAnimationFrame(() => replaceField.current?.focus());
      return;
    }
    if (event.key === "Escape") {
      event.preventDefault();
      close();
      return;
    }
    if (event.key === "Enter") {
      event.preventDefault();
      if (field === "replace") {
        if (event.metaKey || event.ctrlKey) replaceAll(view);
        else replaceNext(view);
      } else if (event.metaKey || event.ctrlKey) {
        selectMatches(view);
        view.focus();
      } else if (event.shiftKey) {
        findPrevious(view);
      } else {
        findNext(view);
      }
      return;
    }
    if (event.key === "Tab" && field === "find" && !event.shiftKey && showReplace) {
      event.preventDefault();
      replaceField.current?.focus();
      return;
    }
    if (event.key === "Tab" && field === "replace" && event.shiftKey) {
      event.preventDefault();
      findField.current?.focus();
      return;
    }
    // ⌘G, ⇧⌘G, ⌘F and the rest of the search's own keys.
    if (runScopeHandlers(view, native, "search-panel")) event.preventDefault();
  };

  const status =
    query.search === ""
      ? ""
      : !query.valid
        ? "Not a pattern"
        : ranges.length === 0
          ? "No results"
          : `${index >= 0 ? index + 1 : "?"} of ${ranges.length}${more ? "+" : ""}`;

  return (
    <div className="find" data-replace={showReplace}>
      <div className="find-row">
        <button
          type="button"
          className="find-expand icon-button"
          aria-label={showReplace ? "Hide replace" : "Show replace"}
          title={`Replace (${label("Mod-Alt-f")})`}
          aria-expanded={showReplace}
          tabIndex={-1}
          onClick={() => {
            setShowReplace(!showReplace);
            if (!showReplace) requestAnimationFrame(() => replaceField.current?.focus());
          }}
        >
          <IconChevronRight size={14} stroke={1.8} />
        </button>
        <label className="find-field" data-invalid={query.search !== "" && !query.valid}>
          <input
            ref={findField}
            main-field="true"
            value={query.search}
            placeholder="Find"
            aria-label="Find"
            spellCheck={false}
            autoComplete="off"
            onChange={(e) => change({ search: e.target.value }, true)}
            onKeyDown={(e) => onKeyDown(e, "find")}
            onFocus={() => (origin.current = view.state.selection.main.from)}
          />
          <span className="find-toggles">
            {toggles.map((toggle) => (
              <button
                key={toggle.key}
                type="button"
                className="toggle"
                aria-pressed={toggle.on}
                title={`${toggle.title} (${label(toggle.key)})`}
                onClick={toggle.flip}
              >
                {toggle.icon}
              </button>
            ))}
          </span>
        </label>
        <span className="find-count" data-none={status === "No results"} aria-live="polite">
          {status}
        </span>
        <button
          type="button"
          className="icon-button"
          title={`Previous (${label("Shift-Enter")})`}
          aria-label="Previous match"
          disabled={ranges.length === 0}
          onClick={() => findPrevious(view)}
        >
          <IconArrowUp size={15} stroke={1.7} />
        </button>
        <button
          type="button"
          className="icon-button"
          title={`Next (${label("Enter")})`}
          aria-label="Next match"
          disabled={ranges.length === 0}
          onClick={() => findNext(view)}
        >
          <IconArrowDown size={15} stroke={1.7} />
        </button>
        <button
          type="button"
          className="icon-button hide-narrow"
          title={`Select all matches (${label("Mod-Enter")})`}
          aria-label="Select all matches"
          disabled={ranges.length === 0}
          onClick={() => {
            selectMatches(view);
            view.focus();
          }}
        >
          <IconSelectAll size={15} stroke={1.7} />
        </button>
        <button
          type="button"
          className="icon-button"
          title="Close (Esc)"
          aria-label="Close find"
          onClick={close}
        >
          <IconX size={15} stroke={1.7} />
        </button>
      </div>
      {showReplace && (
        <div className="find-row">
          <span className="find-expand-space" />
          <label className="find-field">
            <input
              ref={replaceField}
              value={query.replace}
              placeholder="Replace"
              aria-label="Replace"
              spellCheck={false}
              autoComplete="off"
              onChange={(e) => change({ replace: e.target.value })}
              onKeyDown={(e) => onKeyDown(e, "replace")}
            />
          </label>
          <button
            type="button"
            className="text-button"
            title={`Replace this match (${label("Enter")})`}
            disabled={ranges.length === 0 || view.state.readOnly}
            onClick={() => replaceNext(view)}
          >
            <IconReplace size={14} stroke={1.7} />
            <span>Replace</span>
          </button>
          <button
            type="button"
            className="text-button"
            title={`Replace all matches (${label("Mod-Enter")})`}
            disabled={ranges.length === 0 || view.state.readOnly}
            onClick={() => replaceAll(view)}
          >
            <span>All</span>
          </button>
        </div>
      )}
    </div>
  );
}

export function createSearchPanel(view: EditorView): Panel {
  const dom = document.createElement("div");
  dom.className = "find-panel";
  const root = createRoot(dom);
  let tick = 0;
  const close = () => {
    closeSearchPanel(view);
    view.focus();
  };
  const render = () => root.render(<SearchBox view={view} tick={tick++} close={close} />);
  // At once: CodeMirror puts the cursor in the field as soon as the panel is made.
  flushSync(render);
  return {
    dom,
    top: true,
    mount() {
      // A panel of one's own puts the cursor in its field itself, as CodeMirror's does.
      const field = dom.querySelector<HTMLInputElement>("[main-field]");
      field?.focus();
      field?.select();
    },
    update(update: ViewUpdate) {
      if (
        update.docChanged ||
        update.selectionSet ||
        update.transactions.some((tr) => tr.effects.some((e) => e.is(setSearchQuery)))
      ) {
        render();
      }
    },
    destroy() {
      // Unmounting inside CodeMirror's update would be rendering during a render.
      setTimeout(() => root.unmount());
    },
  };
}

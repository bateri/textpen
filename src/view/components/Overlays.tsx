/**
 * What comes over the editor: a question (with a field, or a choice of buttons, each
 * answered with its first letter), the keyboard's page, the settings, and the notes
 * that come and go at the bottom.
 */

import { IconX } from "@tabler/icons-react";
import { useLayoutEffect, useMemo, useRef, useState } from "react";

import { editor } from "../editor/controller";
import { fuzzy } from "../lib/fuzzy";
import { label } from "../lib/keys";
import { changeSettings } from "../state/actions";
import { COMMANDS } from "../state/commands";
import { dismissToast, get, set, useStore } from "../state/store";
import type { Dialog as DialogSpec } from "../state/store";
import type { Settings } from "../../server/types";
import { Keys, moveBy } from "./ui";

function backToEditor(): void {
  if (get().focus === "editor") requestAnimationFrame(() => editor.focus());
}

// ---- a question ---------------------------------------------------------------------------------

export function DialogLayer() {
  const overlay = useStore((s) => s.overlay);
  if (overlay?.kind !== "dialog") return null;
  return <Dialog dialog={overlay.dialog} />;
}

function Dialog({ dialog }: { dialog: DialogSpec }) {
  const [value, setValue] = useState(dialog.input?.value ?? "");
  const field = useRef<HTMLInputElement>(null);
  const box = useRef<HTMLDivElement>(null);
  const primary = dialog.actions.find((a) => a.primary) ?? dialog.actions[0]!;
  // Each button answers to its first letter, when no other shares it.
  const letters = useMemo(() => {
    const counts = new Map<string, number>();
    for (const action of dialog.actions) {
      const letter = action.label[0]!.toLowerCase();
      counts.set(letter, (counts.get(letter) ?? 0) + 1);
    }
    return new Map(
      dialog.actions
        .filter((a) => counts.get(a.label[0]!.toLowerCase()) === 1)
        .map((a) => [a.label[0]!.toLowerCase(), a.id]),
    );
  }, [dialog.actions]);

  useLayoutEffect(() => {
    if (dialog.input && field.current) {
      field.current.focus();
      const [start, end] = dialog.input.select ?? [
        dialog.input.value.length,
        dialog.input.value.length,
      ];
      field.current.setSelectionRange(start, end);
    } else {
      box.current?.querySelector<HTMLButtonElement>('[data-primary="true"]')?.focus();
    }
  }, [dialog]);

  const answer = (action: string | null) => {
    dialog.resolve(action === null ? null : { action, value });
    backToEditor();
  };

  return (
    <div className="scrim" onMouseDown={() => answer(null)}>
      <div
        className="dialog pop"
        role="alertdialog"
        aria-modal
        aria-label={dialog.title}
        ref={box}
        onMouseDown={(e) => e.stopPropagation()}
        onKeyDown={(event) => {
          event.stopPropagation();
          if (event.key === "Escape") {
            event.preventDefault();
            answer(null);
          } else if (
            event.key === "Enter" &&
            (event.target === field.current || !(event.target instanceof HTMLButtonElement))
          ) {
            event.preventDefault();
            answer(primary.id);
          } else if (!dialog.input && !event.metaKey && !event.ctrlKey && !event.altKey) {
            const id = letters.get(event.key.toLowerCase());
            if (id) {
              event.preventDefault();
              answer(id);
            } else if (event.key === "ArrowLeft" || event.key === "ArrowRight") {
              const buttons = [
                ...(box.current?.querySelectorAll<HTMLButtonElement>(".dialog-actions button") ??
                  []),
              ];
              const at = buttons.indexOf(document.activeElement as HTMLButtonElement);
              const next =
                buttons[
                  (at + (event.key === "ArrowRight" ? 1 : -1) + buttons.length) % buttons.length
                ];
              next?.focus();
              event.preventDefault();
            }
          }
        }}
      >
        <div className="dialog-title">{dialog.title}</div>
        {dialog.message && <div className="dialog-message">{dialog.message}</div>}
        {dialog.input && (
          <>
            <input
              ref={field}
              className="dialog-input"
              value={value}
              placeholder={dialog.input.placeholder}
              spellCheck={false}
              autoComplete="off"
              aria-label={dialog.title}
              onChange={(e) => setValue(e.target.value)}
            />
            {dialog.input.hint && <div className="dialog-hint">{dialog.input.hint}</div>}
          </>
        )}
        <div className="dialog-actions">
          {dialog.actions.map((action) => {
            const letter = letters.has(action.label[0]!.toLowerCase()) && !dialog.input;
            return (
              <button
                key={action.id}
                type="button"
                className="text-button"
                data-primary={action.id === primary.id}
                data-danger={action.danger}
                onClick={() => answer(action.id)}
              >
                {letter ? (
                  <span>
                    <u>{action.label[0]}</u>
                    {action.label.slice(1)}
                  </span>
                ) : (
                  action.label
                )}
              </button>
            );
          })}
        </div>
      </div>
    </div>
  );
}

// ---- the keyboard's page ---------------------------------------------------------------------------

const TREE_KEYS: [string, string][] = [
  ["↑ ↓  j k", "Move"],
  ["← →  h l", "Fold, unfold, go to the parent"],
  ["⏎  o", "Open and go to the editor"],
  ["Space", "Open, staying in the tree"],
  ["a  ⇧A", "New file, new folder"],
  ["r  F2", "Rename or move"],
  ["d  ⌫", "Delete"],
  ["y  ⇧Y", "Copy the path, the absolute path"],
  [".", "Menu of the row"],
  ["⇧R", "Read the folders again"],
  ["Esc", "Back to the editor"],
];

export function HelpLayer() {
  const overlay = useStore((s) => s.overlay);
  if (overlay?.kind !== "help") return null;
  return <Help />;
}

function Help() {
  const [query, setQuery] = useState("");
  const field = useRef<HTMLInputElement>(null);
  const list = useRef<HTMLDivElement>(null);
  useLayoutEffect(() => field.current?.focus(), []);
  const groups = useMemo(() => {
    const out = new Map<string, typeof COMMANDS>();
    for (const command of COMMANDS) {
      if (!command.keys?.length || (command.id.startsWith("tab-") && command.id !== "tab-1"))
        continue;
      if (query && !fuzzy(query, `${command.category} ${command.title}`)) continue;
      out.set(command.category, [...(out.get(command.category) ?? []), command]);
    }
    return [...out];
  }, [query]);
  const close = () => {
    set({ overlay: null });
    backToEditor();
  };
  return (
    <div className="scrim" onMouseDown={close}>
      <div
        className="sheet pop"
        role="dialog"
        aria-label="Keyboard shortcuts"
        onMouseDown={(e) => e.stopPropagation()}
      >
        <div className="sheet-head">
          <span className="sheet-title">Keyboard</span>
          <input
            ref={field}
            className="sheet-filter"
            value={query}
            placeholder="Filter…"
            aria-label="Filter the keys"
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Escape") {
                event.preventDefault();
                event.stopPropagation();
                if (query) setQuery("");
                else close();
              } else if (event.key === "ArrowDown" || event.key === "PageDown") {
                event.preventDefault();
                list.current?.scrollBy({ top: event.key === "PageDown" ? 300 : 40 });
              } else if (event.key === "ArrowUp" || event.key === "PageUp") {
                event.preventDefault();
                list.current?.scrollBy({ top: event.key === "PageUp" ? -300 : -40 });
              }
            }}
          />
          <button type="button" className="icon-button" aria-label="Close" onClick={close}>
            <IconX size={15} stroke={1.7} />
          </button>
        </div>
        <div className="sheet-body help-grid" ref={list}>
          {groups.map(([category, commands]) => (
            <section key={category} className="help-group">
              <h3>{category}</h3>
              {commands.map((command) => (
                <div key={command.id} className="help-row">
                  <span className="ellipsis">
                    {command.id === "tab-1" ? "Go to Tab 1–9" : command.title}
                  </span>
                  <span className="help-keys">
                    {command.keys!.map((key, i) => (
                      <span key={key} className="help-key">
                        {i > 0 && <span className="faint">or</span>}
                        <Keys spec={command.id === "tab-1" ? "Ctrl-1" : key} />
                      </span>
                    ))}
                    {command.id === "tab-1" && <span className="faint">…9</span>}
                  </span>
                </div>
              ))}
            </section>
          ))}
          {!query && (
            <section className="help-group">
              <h3>Files (click a row, or {label("Shift-Mod-e")} / F6)</h3>
              {TREE_KEYS.map(([keys, what]) => (
                <div key={keys} className="help-row">
                  <span>{what}</span>
                  <span className="help-keys">
                    <kbd>{keys}</kbd>
                  </span>
                </div>
              ))}
            </section>
          )}
          <p className="help-note">
            ⌃C always reaches textpen and ends it; what is not saved is kept as a draft and offered
            the next time. The terminal keeps its own keys (⌘T, ⌘W, ⌘P, ⌘1–9 in bateri): everything
            they would do here is in the palette, <Keys spec="F1" />.
          </p>
        </div>
      </div>
    </div>
  );
}

// ---- settings ---------------------------------------------------------------------------------------

interface Row {
  key: keyof Settings;
  title: string;
  detail: string;
  value: (s: Settings) => string;
  /** One step: +1 forward (Space, →), −1 back (←). */
  step: (s: Settings, by: 1 | -1) => Partial<Settings>;
}

const cycle = <T,>(values: readonly T[], value: T, by: 1 | -1): T =>
  values[(values.indexOf(value) + by + values.length) % values.length]!;

const ROWS: Row[] = [
  {
    key: "vim",
    title: "Vim keys",
    detail: ":w saves, :q closes the tab, :qa quits",
    value: (s) => (s.vim ? "On" : "Off"),
    step: (s) => ({ vim: !s.vim }),
  },
  {
    key: "fontSize",
    title: "Text size",
    detail: "⌘+ and ⌘− too",
    value: (s) => `${s.fontSize}px`,
    step: (s, by) => ({ fontSize: Math.min(28, Math.max(9, s.fontSize + by)) }),
  },
  {
    key: "wrap",
    title: "Word wrap",
    detail: "Long lines wrap at the edge",
    value: (s) => (s.wrap ? "On" : "Off"),
    step: (s) => ({ wrap: !s.wrap }),
  },
  {
    key: "lineNumbers",
    title: "Line numbers",
    detail: "Relative counts from the cursor",
    value: (s) => s.lineNumbers,
    step: (s, by) => ({
      lineNumbers: cycle(["on", "relative", "off"] as const, s.lineNumbers, by),
    }),
  },
  {
    key: "tabSize",
    title: "Tab size",
    detail: "Columns a tab and an indent take",
    value: (s) => String(s.tabSize),
    step: (s, by) => ({ tabSize: cycle([2, 4, 8], s.tabSize, by) }),
  },
  {
    key: "insertSpaces",
    title: "Indent with",
    detail: "What ⇥ puts in",
    value: (s) => (s.insertSpaces ? "Spaces" : "Tabs"),
    step: (s) => ({ insertSpaces: !s.insertSpaces }),
  },
  {
    key: "whitespace",
    title: "Show whitespace",
    detail: "Spaces as dots, tabs as lines",
    value: (s) => (s.whitespace ? "On" : "Off"),
    step: (s) => ({ whitespace: !s.whitespace }),
  },
  {
    key: "autoSave",
    title: "Save when leaving",
    detail: "Before the shell, on another tab",
    value: (s) => (s.autoSave ? "On" : "Off"),
    step: (s) => ({ autoSave: !s.autoSave }),
  },
];

export function SettingsLayer() {
  const overlay = useStore((s) => s.overlay);
  if (overlay?.kind !== "settings") return null;
  return <SettingsSheet />;
}

function SettingsSheet() {
  const settings = useStore((s) => s.settings);
  const [index, setIndex] = useState(0);
  const box = useRef<HTMLDivElement>(null);
  useLayoutEffect(() => box.current?.focus(), []);
  const close = () => {
    set({ overlay: null });
    backToEditor();
  };
  return (
    <div className="scrim" onMouseDown={close}>
      <div
        className="sheet settings pop"
        role="dialog"
        aria-label="Settings"
        tabIndex={-1}
        ref={box}
        onMouseDown={(e) => e.stopPropagation()}
        onKeyDown={(event) => {
          event.stopPropagation();
          const row = ROWS[index]!;
          const next = moveBy(event, index, ROWS.length, 4);
          if (next !== undefined) {
            event.preventDefault();
            setIndex(next);
          } else if (
            event.key === " " ||
            event.key === "Enter" ||
            event.key === "ArrowRight" ||
            event.key === "l"
          ) {
            event.preventDefault();
            changeSettings(row.step(get().settings, 1));
          } else if (event.key === "ArrowLeft" || event.key === "h") {
            event.preventDefault();
            changeSettings(row.step(get().settings, -1));
          } else if (event.key === "Escape") {
            event.preventDefault();
            close();
          }
        }}
      >
        <div className="sheet-head">
          <span className="sheet-title">Settings</span>
          <span className="faint hide-narrow">kept in ~/.config/textpen</span>
          <span className="grow" />
          <button type="button" className="icon-button" aria-label="Close" onClick={close}>
            <IconX size={15} stroke={1.7} />
          </button>
        </div>
        <div className="sheet-body" role="list">
          {ROWS.map((row, i) => (
            <div
              key={row.key}
              role="listitem"
              className="setting"
              data-chosen={i === index}
              onMouseMove={() => setIndex(i)}
              onClick={() => changeSettings(row.step(get().settings, 1))}
            >
              <span className="setting-text">
                <span>{row.title}</span>
                <span className="faint ellipsis">{row.detail}</span>
              </span>
              <span className="setting-value" data-on={row.value(settings) === "On"}>
                {row.value(settings)}
              </span>
            </div>
          ))}
        </div>
        <div className="palette-foot" aria-hidden>
          <span>
            <kbd>↑</kbd>
            <kbd>↓</kbd> choose
          </span>
          <span>
            <kbd>Space</kbd>
            <kbd>←</kbd>
            <kbd>→</kbd> change
          </span>
          <span>
            <kbd>Esc</kbd> close
          </span>
        </div>
      </div>
    </div>
  );
}

// ---- notes --------------------------------------------------------------------------------------------

export function Toasts() {
  const toasts = useStore((s) => s.toasts);
  return (
    <div className="toasts" aria-live="polite">
      {toasts.map((toast) => (
        <div key={toast.id} className="toast" data-tone={toast.tone}>
          <span className="toast-text">{toast.text}</span>
          {toast.action && (
            <button
              type="button"
              className="text-button"
              onClick={() => {
                toast.action!.run();
                dismissToast(toast.id);
              }}
            >
              {toast.action.label}
            </button>
          )}
          <button
            type="button"
            className="icon-button"
            aria-label="Dismiss"
            onClick={() => dismissToast(toast.id)}
          >
            <IconX size={13} stroke={1.8} />
          </button>
        </div>
      ))}
    </div>
  );
}

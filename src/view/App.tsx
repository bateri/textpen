/**
 * textpen's view. It is an MCP App: it talks to its program through whatever host shows
 * it, with the messages MCP Apps defines. In a pencere terminal it is the whole pane,
 * in the terminal's colours and font, and every key but ⌃C and the terminal's own is
 * its.
 */

import { IconTerminal2 } from "@tabler/icons-react";
import { useEffect, useLayoutEffect, useState } from "react";

import type { Boot } from "../server/types";
import { EditorArea } from "./components/EditorArea";
import { MenuLayer } from "./components/Menu";
import { DialogLayer, HelpLayer, SettingsLayer, Toasts } from "./components/Overlays";
import { PaletteLayer } from "./components/Palette";
import { Resizer, Sidebar } from "./components/Sidebar";
import { StatusBar } from "./components/StatusBar";
import { Tabs } from "./components/Tabs";
import { Tooltips } from "./components/Tooltips";
import { editor } from "./editor/controller";
import { attach, call } from "./lib/bridge";
import { Host } from "./lib/host";
import { closeTab, quit, save, start, stopPolling } from "./state/actions";
import { onKeyDown } from "./state/keyboard";
import { get, set, sidebarDocked, useStore } from "./state/store";
import type { Layout } from "./state/store";

/** The package's version, which the build writes in (`vite.config.ts`). */
declare const __TEXTPEN_VERSION__: string;
const VERSION = __TEXTPEN_VERSION__;

function booted(result: { structuredContent?: unknown }): void {
  const shown = result.structuredContent as { boot?: Boot } | undefined;
  if (!shown?.boot || get().boot !== null) return;
  // The keys are the view's from the first moment, without a click into it first.
  window.focus();
  void start(shown.boot);
}

let connection: Promise<Host> | undefined;

/** The one connection to the host, made once: StrictMode mounts twice in development. */
function connect(): Promise<Host> {
  connection ??= (async () => {
    const host = new Host({
      toolResult: (result) => booted(result),
      // The host takes the view down: what was typed last goes to the program first.
      teardown: async () => {
        stopPolling();
        await Promise.race([editor.flushAll(), new Promise((r) => setTimeout(r, 900))]);
      },
    });
    await host.connect({ name: "textpen", version: VERSION });
    return host;
  })();
  return connection;
}

// `:w`, `:q`, `:wq`, `:qa` in vim's command line.
editor.onVim = (command, force) => {
  const path = get().active;
  const close = async () => {
    if (path === null) return;
    if (force) await call("textpen_buffer", { path, action: "revert" }).catch(() => {});
    const last = get().tabs.length === 1;
    if (await closeTab(path)) {
      // As in vim, quitting the last window quits.
      if (last && !get().boot?.closeQuits) void quit();
    }
  };
  if (command === "write") void save();
  else if (command === "quit") void close();
  else if (command === "writequit") {
    void save().then(async (saved) => {
      if (saved) await close();
    });
  } else void quit();
};

function layoutOf(width: number): Layout {
  if (width < 760) return "narrow";
  if (width < 1080) return "medium";
  return "wide";
}

export function App() {
  const [error, setError] = useState<Error | null>(null);

  useEffect(() => {
    let timer: ReturnType<typeof setTimeout> | undefined;
    connect().then(
      (host) => {
        attach(host);
        // A host that does not call the view's tool by itself: the view asks for what it shows.
        timer = setTimeout(() => {
          if (get().boot !== null) return;
          void call<{ boot: Boot }>("show_editor", {}).then(
            (shown) => booted({ structuredContent: shown }),
            (failure: unknown) =>
              setError(failure instanceof Error ? failure : new Error(String(failure))),
          );
        }, 2500);
      },
      (failure: unknown) =>
        setError(failure instanceof Error ? failure : new Error(String(failure))),
    );
    return () => clearTimeout(timer);
  }, []);

  useLayoutEffect(() => {
    const measure = () => {
      const layout = layoutOf(window.innerWidth);
      const short = window.innerHeight < 320;
      const state = get();
      if (state.layout !== layout || state.short !== short) {
        set({ layout, short, ...(layout !== "narrow" ? { drawer: false } : {}) });
      }
    };
    measure();
    window.addEventListener("resize", measure);
    const keydown = (event: KeyboardEvent) => onKeyDown(event);
    window.addEventListener("keydown", keydown, true);
    // The page's own menu has nothing for a person here; the rows have theirs.
    const context = (event: MouseEvent) => {
      if (!(event.target as HTMLElement).closest("input, textarea, .cm-content, .preview")) {
        event.preventDefault();
      }
    };
    window.addEventListener("contextmenu", context);
    // Where the keys are, as the page has it: a click into the editor or the sidebar moves
    // them, and the status line and the tree say so.
    const focusin = (event: FocusEvent) => {
      const target = event.target as HTMLElement;
      const next = editor.view?.dom.contains(target)
        ? "editor"
        : target.closest?.(".sidebar")
          ? "sidebar"
          : null;
      if (next !== null && get().focus !== next) set({ focus: next });
    };
    window.addEventListener("focusin", focusin);
    return () => {
      window.removeEventListener("focusin", focusin);
      window.removeEventListener("resize", measure);
      window.removeEventListener("keydown", keydown, true);
      window.removeEventListener("contextmenu", context);
    };
  }, []);

  const booting = useStore((s) => s.boot === null);
  const docked = useStore((s) => sidebarDocked(s));
  const drawer = useStore((s) => s.layout === "narrow" && s.drawer);
  const width = useStore((s) => s.settings.sidebarWidth);
  const layout = useStore((s) => s.layout);
  const handingOff = useStore((s) => s.handingOff);
  const overlay = useStore((s) => s.overlay !== null);
  const focus = useStore((s) => s.focus);

  return (
    <>
      <div
        className="app"
        data-layout={layout}
        data-sidebar={docked}
        data-focus={focus}
        data-overlay={overlay}
        data-handing-off={handingOff !== null}
        style={{ "--sidebar-w": `${width}px` } as React.CSSProperties}
      >
        {docked && (
          <>
            <Sidebar drawer={false} />
            <Resizer />
          </>
        )}
        <main className="main">
          <Tabs />
          <EditorArea />
        </main>
        <StatusBar />
        {drawer && (
          <>
            <div
              className="drawer-scrim"
              onMouseDown={() => set({ drawer: false, focus: "editor" })}
            />
            <Sidebar drawer />
          </>
        )}
      </div>
      {booting && (
        <div className="boot" data-error={error !== null}>
          <div className="boot-inner">
            {error ? (
              <>
                <b>The terminal did not answer</b>
                <span className="faint">{error.message}</span>
              </>
            ) : (
              <span className="spinner" />
            )}
          </div>
        </div>
      )}
      {handingOff && (
        <div className="handoff">
          <div className="handoff-inner pop">
            <IconTerminal2 size={22} stroke={1.5} />
            <span>
              {handingOff === "shell" ? "Handing the pane to a shell…" : "Closing textpen…"}
            </span>
            {handingOff === "shell" && <span className="faint">exit or ⌃D comes back here</span>}
          </div>
        </div>
      )}
      <MenuLayer />
      <DialogLayer />
      <PaletteLayer />
      <HelpLayer />
      <SettingsLayer />
      <Toasts />
      <Tooltips />
    </>
  );
}

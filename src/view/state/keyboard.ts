/**
 * The keyboard: one listener on the window, ahead of everything (capture), for the keys
 * of textpen's commands. What is over the page (the palette, a dialog) takes its own
 * keys; the editor takes CodeMirror's.
 *
 * ⌃C is never taken: it is the program's, and the terminal sends it there whatever the
 * view does.
 */

import { matches } from "../lib/keys";
import { COMMANDS } from "./commands";
import { get, set } from "./store";

export function onKeyDown(event: KeyboardEvent): void {
  if (event.defaultPrevented || event.isComposing) return;
  if (event.ctrlKey && !event.metaKey && event.key.toLowerCase() === "c") return;
  const state = get();
  if (state.handingOff !== null) {
    event.preventDefault();
    return;
  }
  if (state.overlay !== null) {
    // ⌘K in the palette moves between files and commands; the palette does that itself.
    if (state.overlay.kind === "help" || state.overlay.kind === "settings") {
      if (event.key === "Escape") {
        event.preventDefault();
        set({ overlay: null });
      }
    }
    return;
  }
  for (const command of COMMANDS) {
    if (command.editor || !command.keys) continue;
    if (!command.keys.some((key) => matches(event, key))) continue;
    if (command.when && !command.when()) continue;
    event.preventDefault();
    event.stopPropagation();
    void command.run();
    return;
  }
}

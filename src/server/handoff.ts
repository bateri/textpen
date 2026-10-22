/**
 * The pane, handed to a shell and back. The view has closed by then and the terminal is
 * the program's again, on the primary screen with its own modes, so the shell gets a
 * terminal as if it had been started from the prompt — in the project's directory.
 */

import { spawn } from "node:child_process";

import type { Returned } from "./types.js";

/**
 * The shell integrations a person's rc file loads whatever the terminal, turned off in the
 * shell textpen starts: their prompt marks carry no identity of the terminal's own
 * integration, and a short-lived shell inside textpen's command has no prompt the terminal
 * needs to know of — its marks would only move the terminal's idea of the pane (bateri's
 * phase and its dock) back and forth. iTerm2's script is skipped when its own guard is set;
 * WezTerm's has a switch for it.
 */
const QUIET_INTEGRATIONS = {
  ITERM_SHELL_INTEGRATION_INSTALLED: "Yes",
  WEZTERM_SHELL_SKIP_ALL: "1",
};

const BAR = (text: string) => `\x1b[7m textpen \x1b[0m ${text}\x1b[0m`;
const DIM = (text: string) => `\x1b[2m${text}\x1b[0m`;

export async function runShell(cwd: string, label: string): Promise<Returned> {
  const out = process.stdout;
  const shell = process.env.SHELL || "/bin/sh";
  out.write(
    `${BAR(`shell in \x1b[1m${label}\x1b[0m`)}  ${DIM("exit or ⌃D goes back to the editor")}\n`,
  );
  const started = Date.now();
  try {
    const exitCode = await new Promise<number | null>((resolve, reject) => {
      const child = spawn(shell, [], {
        cwd,
        stdio: "inherit",
        env: { ...process.env, ...QUIET_INTEGRATIONS, TEXTPEN_SHELL: "1" },
      });
      child.on("error", reject);
      child.on("close", (code) => resolve(code));
    });
    out.write(DIM("\n↩ back to textpen\n"));
    return { exitCode, duration: Date.now() - started };
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    out.write(`${BAR(`could not start ${shell}: ${message}`)}\n`);
    await new Promise((resolve) => setTimeout(resolve, 1500));
    return { exitCode: null, duration: Date.now() - started, error: message };
  }
}

/**
 * The editor of the person's own choosing, for a terminal that does not show views:
 * `$VISUAL`, `$EDITOR` or vi, run as git runs it (`sh -c '$EDITOR "$@"'`), so that an
 * editor given with options works. textpen itself is passed over, so that `EDITOR=textpen`
 * does not start textpen again.
 */
export async function runOwnEditor(paths: string[], cwd: string): Promise<number> {
  const chosen = [process.env.VISUAL, process.env.EDITOR].find(
    (editor) => editor && !/(^|\/)textpen(\s|$)/.test(editor.trim()),
  );
  const editor = chosen ?? "vi";
  return new Promise<number>((resolve) => {
    const child = spawn("sh", ["-c", `${editor} "$@"`, editor, ...paths], {
      cwd,
      stdio: "inherit",
    });
    child.on("error", () => resolve(127));
    child.on("close", (code) => resolve(code ?? 1));
  });
}

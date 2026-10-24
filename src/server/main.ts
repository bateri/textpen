#!/usr/bin/env node
/**
 * textpen. In a terminal that shows pencere views, an editor opens in the pane; in one
 * that does not, the person's own `$EDITOR` does.
 *
 * The view hands the pane over when the person asks for a shell: the program closes
 * the view, runs the shell in the terminal, and shows the view again where it was
 * when the shell ends. What is not saved lives in the program, so it crosses that gap,
 * and a ⌃C or a dropped connection leaves it as a draft for the next run.
 */

import { bundleFromDirectory, serve, ViewControl } from "@pencere/sdk";
import type { Bundle } from "@pencere/sdk";
import { statSync, watch } from "node:fs";
import { dirname, isAbsolute, relative, resolve } from "node:path";
import { createInterface } from "node:readline";

import { Textpen, VERSION } from "./app.js";
import type { Focus } from "./app.js";
import { runOwnEditor, runShell } from "./handoff.js";
import { ICONS } from "./icon.js";
import { trace, traceTty } from "./trace.js";
import { VIEW_DIRECTORY, createProgram } from "./program.js";

const USAGE = `usage: textpen [options] [+line] [path[:line[:column]] ...]

A code editor in the terminal pane: a file tree, tabs, search, and the
terminal's own colours and font, with the keyboard or the mouse.

  path                 a file to open, or the directory to open as the project
                       (the current directory by default, with the files that
                       were open there last time)
  +line, path:line     put the cursor on that line
      --dev            show the view again when its files in dist/view change
  -h, --help           this text
  -v, --version        the version

In a terminal that does not show pencere views it runs $VISUAL or $EDITOR.`;

interface Options {
  paths: string[];
  focus?: Focus;
  dev: boolean;
}

function parse(argv: readonly string[]): Options {
  const options: Options = { paths: [], dev: false };
  let line: number | undefined;
  let rest = false;
  for (const arg of argv) {
    if (!rest && (arg === "-h" || arg === "--help")) {
      console.log(USAGE);
      process.exit(0);
    } else if (!rest && (arg === "-v" || arg === "--version")) {
      console.log(VERSION);
      process.exit(0);
    } else if (!rest && arg === "--dev") {
      options.dev = true;
    } else if (!rest && arg === "--") {
      rest = true;
    } else if (!rest && /^\+\d+$/.test(arg)) {
      line = Number(arg.slice(1));
    } else if (!rest && arg.startsWith("-") && arg !== "-") {
      console.error(`textpen: unknown option ${arg}\n\n${USAGE}`);
      process.exit(2);
    } else {
      const { path, line: at, column } = splitPlace(arg);
      options.paths.push(path);
      if (options.focus === undefined && (at !== undefined || line !== undefined)) {
        options.focus = {
          path,
          ...((at ?? line) !== undefined ? { line: (at ?? line)! } : {}),
          ...(column !== undefined ? { column } : {}),
        };
      }
    }
  }
  return options;
}

/** `src/a.ts:12:4` is the file at a line and column, unless a file has that very name. */
function splitPlace(arg: string): { path: string; line?: number; column?: number } {
  const match = /^(.*?):(\d+)(?::(\d+))?$/.exec(arg);
  if (!match || exists(arg)) return { path: arg };
  return {
    path: match[1]!,
    line: Number(match[2]),
    ...(match[3] ? { column: Number(match[3]) } : {}),
  };
}

function exists(path: string): boolean {
  try {
    statSync(path);
    return true;
  } catch {
    return false;
  }
}

function isDirectory(path: string): boolean {
  try {
    return statSync(path).isDirectory();
  } catch {
    return false;
  }
}

/**
 * The project: the directory given, or the current one when the files given are in it,
 * or else the directory of the first file.
 */
function rootOf(paths: string[]): { root: string; files: string[] } {
  const cwd = process.cwd();
  const first = paths[0];
  if (first !== undefined && isDirectory(first)) {
    return { root: resolve(first), files: paths.slice(1).filter((p) => !isDirectory(p)) };
  }
  const files = paths.filter((p) => !isDirectory(p)).map((p) => resolve(p));
  const outside = (path: string) => {
    const rel = relative(cwd, path);
    return rel.startsWith("..") || isAbsolute(rel);
  };
  const root = files.length > 0 && outside(files[0]!) ? dirname(files[0]!) : cwd;
  return { root, files };
}

const options = parse(process.argv.slice(2));
const { root, files } = rootOf(options.paths);
const textpen = new Textpen(root);
if (files.length > 0) {
  const focus = options.focus && { ...options.focus, path: resolve(options.focus.path) };
  textpen.startWith(files, focus);
}
const program = createProgram(textpen);

function viewBundle(): Promise<Bundle> {
  return bundleFromDirectory(VIEW_DIRECTORY);
}

/**
 * The editor did not come back after the shell: the terminal did not answer, or would not
 * show it. What is open is still the program's, so the person chooses: once more, or quit
 * (keeping what is not saved as drafts). True to try again.
 */
async function tryAgain(reason: string): Promise<boolean> {
  const why = reason === "no-host" ? "the terminal did not answer" : `the terminal said ${reason}`;
  process.stdout.write(
    `\x1b[7m textpen \x1b[0m the editor did not come back: ${why}.\n` +
      `\x1b[2m         What is not saved is kept. ⏎ tries again, q and ⏎ quits.\x1b[0m `,
  );
  const lines = createInterface({ input: process.stdin, terminal: false });
  const answer = await new Promise<string>((resolve) => {
    lines.once("line", resolve);
    lines.once("close", () => resolve("q"));
  });
  lines.close();
  process.stdin.pause();
  return answer.trim().toLowerCase() !== "q";
}

// ⌃C reaches textpen as SIGINT. While the view is up the SDK gives the terminal back
// and `serve` ends (with a listener of the program's own, it does not end the process);
// while a shell or the person's own editor has the pane, ⌃C is theirs.
process.on("SIGINT", () => {});
// A dropped connection hangs the program up: what is not saved becomes a draft first.
process.once("SIGHUP", () => {
  textpen.ending();
  process.kill(process.pid, "SIGHUP");
});

let bundle = await viewBundle();
let control: ViewControl | undefined;
let exitCode = 0;

if (options.dev) {
  let timer: ReturnType<typeof setTimeout> | undefined;
  watch(VIEW_DIRECTORY, { recursive: true }, () => {
    clearTimeout(timer);
    timer = setTimeout(async () => {
      const next = await viewBundle().catch(() => undefined);
      if (next === undefined || control === undefined) return;
      bundle = next;
      await control.reopen({ views: [{ kind: "web", bundle: next }] });
    }, 300);
  }).unref();
}

let shown = false;
for (;;) {
  const current = new ViewControl();
  control = current;
  let shellAsked = false;
  textpen.desk = {
    shell() {
      if (shellAsked) return "busy";
      shellAsked = true;
      // The answer to the view's call goes out first, so that it can show what comes.
      setTimeout(() => {
        void current.close().then((closed) => {
          if (closed.kind === "closed") return;
          shellAsked = false;
          textpen.handoffFailed =
            closed.kind === "unsupported"
              ? "This terminal cannot hand its pane to a command and back."
              : "The view could not be closed just now.";
        });
      }, 120);
      return "scheduled";
    },
  };
  traceTty(shown ? "reopen" : "open");
  const asked = Date.now();
  const result = await serve({
    views: [{ kind: "web", bundle }],
    program: "textpen",
    // What a terminal writes above the view, and the icon it draws beside it.
    title: "Editor",
    icons: ICONS,
    ...program,
    control: current,
    // The person's own editor stands in for a terminal that shows no views, at the start
    // only: once the editor has been shown, what is open is textpen's, and a view that
    // does not come back is asked about, not replaced with another program.
    ...(shown
      ? {}
      : {
          fallback: async (why) => {
            if (why.reason === "no-tty") {
              // No terminal at all (a script, a pipe): there is no pane for any editor.
              console.error("textpen: needs a terminal to edit in");
              exitCode = 1;
              return;
            }
            exitCode = await runOwnEditor(
              files.length > 0 ? files.map((f) => relative(root, f) || f) : ["."],
              root,
            );
          },
        }),
  });
  textpen.desk = undefined;
  control = undefined;
  // How long the wait took tells the no-view reasons apart: at once, the tty's modes could
  // not be set or the terminal answered `CSI c` and not the `hello`; two seconds, nothing.
  trace("served", { ...result, ms: Date.now() - asked, shell: shellAsked });
  if (result.kind === "closed") shown = true;
  if (result.kind === "closed" && result.by === "program" && shellAsked) {
    textpen.returned = await runShell(root, textpen.project.root);
    trace("shell ended", { ...textpen.returned });
    continue;
  }
  if (result.kind === "no-view" && shown && result.reason !== "aborted") {
    if (await tryAgain(result.reason)) continue;
  }
  if (result.kind === "closed" && result.by === "signal") exitCode = 130;
  else if (result.kind === "closed" && result.by === "error") exitCode = 1;
  else if (result.kind === "no-view" && result.reason === "aborted") exitCode = 130;
  break;
}
textpen.ending();
process.exit(exitCode);

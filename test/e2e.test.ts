// textpen in a terminal pane, with pencere's stand-in host (`pencere-fake-host`) playing
// the terminal: the view's calls travel to the program through the tty, as they do in
// bateri. Skipped where the stand-in is not built: set PENCERE_FAKE_HOST, or build it in
// a pencere checkout beside this one (`cargo build -p pencere-fake-host`).
// Run against the built program (`npm run build`).

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { existsSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, beforeEach, test } from "node:test";

const HOST =
  process.env.PENCERE_FAKE_HOST ??
  new URL("../../pencere/target/debug/pencere-fake-host", import.meta.url).pathname;
const MAIN = new URL("../dist/server/main.js", import.meta.url).pathname;
const skip = !existsSync(HOST)
  ? "pencere-fake-host is not built"
  : !existsSync(MAIN)
    ? "run npm run build first"
    : false;

let dir: string;
let env: NodeJS.ProcessEnv;

beforeEach(() => {
  dir = mkdtempSync(join(tmpdir(), "textpen-e2e-"));
  env = {
    ...process.env,
    XDG_STATE_HOME: join(dir, "state"),
    XDG_CONFIG_HOME: join(dir, "config"),
    SHELL: "/usr/bin/true",
  };
  writeFileSync(join(dir, "a.txt"), "world\n");
});

afterEach(() => rmSync(dir, { recursive: true, force: true }));

/** Runs textpen on a.txt in a pane; the view makes the calls, then `then` happens. */
function pane(calls: [string, unknown][], then: string[] = ["--then", "interrupt"]) {
  const args = ["--size", "100x30", "--limit", "25"];
  for (const [name, input] of calls) args.push("--call", `${name}=${JSON.stringify(input)}`);
  const run = spawnSync(HOST, [...args, ...then, "--", process.execPath, MAIN, "a.txt"], {
    cwd: dir,
    env,
    encoding: "utf8",
    timeout: 40_000,
  });
  return run.stderr;
}

test("an edit sent through the tty is saved to the file", { skip }, () => {
  const log = pane([
    ["textpen_open", { path: "a.txt" }],
    ["textpen_edit", { path: "a.txt", base: 1, edits: [[0, 0, "hello "]] }],
    ["textpen_save", { path: "a.txt", version: 2 }],
  ]);
  assert.match(log, /tool result "Saved a\.txt"/);
  assert.match(log, /program: bye id=\d+ reason=signal/);
  assert.match(log, /alternate screen off/);
  assert.equal(readFileSync(join(dir, "a.txt"), "utf8"), "hello world\n");
});

test("⌃C keeps what is not saved as a draft, and the next run offers it", { skip }, () => {
  pane([
    ["textpen_open", { path: "a.txt" }],
    ["textpen_edit", { path: "a.txt", base: 1, edits: [[5, 5, "!"]] }],
  ]);
  assert.equal(readFileSync(join(dir, "a.txt"), "utf8"), "world\n");
  assert.equal(readdirSync(join(dir, "state", "textpen", "drafts")).length, 1);
  const log = pane([
    ["textpen_open", { path: "a.txt" }],
    ["textpen_buffer", { path: "a.txt", action: "restore" }],
    ["textpen_save", { path: "a.txt", version: 2 }],
  ]);
  assert.match(log, /tool result "Saved a\.txt"/);
  assert.equal(readFileSync(join(dir, "a.txt"), "utf8"), "world!\n");
});

test("the pane goes to a shell and comes back to the editor", { skip }, () => {
  const log = pane(
    [["textpen_handoff", { kind: "shell" }]],
    ["--on-view", "2", "--then", "interrupt"],
  );
  assert.match(log, /program: dismiss/);
  assert.match(log, /closed id=\d+ re=\d+ reason=program/);
  // The second view is served from what the terminal kept of the first.
  assert.match(log, /need id=\d+ re=2 missing=0/);
  assert.equal((log.match(/tools\/call show_editor/g) ?? []).length, 2);
});

test("quitting from the view ends textpen with success", { skip }, () => {
  const log = pane([["textpen_handoff", { kind: "quit" }]], []);
  assert.match(log, /program: exited with status 0/);
});

test("a terminal without pencere runs the person's own editor", { skip }, () => {
  env.VISUAL = "textpen";
  env.EDITOR = "cat -n";
  const run = spawnSync(
    HOST,
    ["--no-host", "--passthrough", "--limit", "15", "--", process.execPath, MAIN, "a.txt"],
    {
      cwd: dir,
      env,
      encoding: "utf8",
      timeout: 30_000,
    },
  );
  assert.match(run.stdout, /1\tworld/);
  assert.match(run.stderr, /exited with status 0/);
});

test("a text too long for one message is read in pieces", { skip }, () => {
  // Every character outside ASCII crosses the stream as six bytes: 3 million of them do
  // not fit in one 16 MiB message.
  writeFileSync(join(dir, "a.txt"), ("ş".repeat(99) + "\n").repeat(30_000));
  const log = pane([
    ["textpen_open", { path: "a.txt" }],
    ["textpen_slice", { path: "a.txt", version: 1, from: 0, to: 1_000_000 }],
    ["textpen_slice", { path: "a.txt", version: 1, from: 2_000_000, to: 3_000_000 }],
  ]);
  assert.match(log, /tool result "a\.txt, 3000000 characters"/);
  assert.equal((log.match(/tool result "1000000 characters"/g) ?? []).length, 2);
});

test("the shell starts with the rc file's own shell integration off", { skip }, () => {
  // An integration a rc file loads anyway writes prompt marks the terminal would take for a
  // prompt of its own; the shell inside textpen's command starts with them off.
  const shell = join(dir, "shell");
  writeFileSync(
    shell,
    `#!/bin/sh\necho "$ITERM_SHELL_INTEGRATION_INSTALLED $WEZTERM_SHELL_SKIP_ALL" > ${join(dir, "env.out")}\n`,
    { mode: 0o755 },
  );
  env.SHELL = shell;
  const log = pane(
    [["textpen_handoff", { kind: "shell" }]],
    ["--on-view", "2", "--then", "interrupt"],
  );
  assert.equal((log.match(/tools\/call show_editor/g) ?? []).length, 2);
  assert.equal(readFileSync(join(dir, "env.out"), "utf8").trim(), "Yes 1");
});

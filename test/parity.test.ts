// What the program answers, read off the stand-in host's record of a pane: the same tests run on the
// Node program (`npm run build`) and on the Lua one (`TEXTPEN_BIN=dist/textpen-lua`, made by
// `make lua`), so that a port is held to what the first one does. The page is the same in both, so
// the calls keep their names, their params and the shapes of their results.
//
// A test that reads a result needs all of it: the host's record cuts a result at 100 characters
// unless PENCERE_FAKE_GIST says otherwise, which these tests do. See e2e.test.ts for how the stand-in
// and the program are found, and for why a missing one fails the run where it is asked for.

import assert from "node:assert/strict";
import { spawn, spawnSync } from "node:child_process";
import {
  chmodSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  readdirSync,
  realpathSync,
  rmSync,
  symlinkSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { afterEach, beforeEach, test } from "node:test";

const HOST = process.env.PENCERE_FAKE_HOST
  ? resolve(process.env.PENCERE_FAKE_HOST)
  : new URL("../../pencere/target/debug/pencere-fake-host", import.meta.url).pathname;
const BIN = process.env.TEXTPEN_BIN ? resolve(process.env.TEXTPEN_BIN) : undefined;
const MAIN = new URL("../dist/server/main.js", import.meta.url).pathname;
const PROGRAM: string[] = BIN ? [BIN] : [process.execPath, MAIN];
const REQUIRED = BIN !== undefined || process.env.TEXTPEN_REQUIRE_E2E === "1";
const LUA = BIN !== undefined;

const skip = !existsSync(HOST)
  ? `pencere-fake-host is not built (${HOST})`
  : !existsSync(BIN ?? MAIN)
    ? `the program is not built (${BIN ?? MAIN})`
    : false;
if (skip && REQUIRED) {
  throw new Error(`the parity tests cannot run, and a run that needs them may not skip: ${skip}`);
}

type Call = [string, unknown];
interface Answer {
  ok: boolean;
  result?: any;
  code?: string;
  message?: string;
}

let dir: string;
let env: NodeJS.ProcessEnv;

beforeEach(() => {
  dir = mkdtempSync(join(tmpdir(), "textpen-parity-"));
  // Nothing the program keeps is written under the real home.
  env = {
    ...process.env,
    HOME: dir,
    XDG_STATE_HOME: join(dir, "state"),
    XDG_CONFIG_HOME: join(dir, "config"),
    SHELL: "/usr/bin/true",
    PENCERE_FAKE_GIST: "100000000",
  };
});

afterEach(() => rmSync(dir, { recursive: true, force: true }));

/** The record of a pane in which the view makes `calls`, one after the other, and then `then` happens. */
function record(
  calls: Call[],
  options: {
    then?: string[];
    limit?: number;
    env?: NodeJS.ProcessEnv;
    args?: string[];
    hostArgs?: string[];
  } = {},
): string {
  const args = [
    "--size",
    "100x30",
    "--limit",
    String(options.limit ?? 25),
    ...(options.hostArgs ?? []),
  ];
  for (const [method, params] of calls) args.push("--call", `${method}=${JSON.stringify(params)}`);
  const run = spawnSync(
    HOST,
    [
      ...args,
      ...(options.then ?? ["--then", "interrupt"]),
      "--",
      ...PROGRAM,
      ...(options.args ?? []),
    ],
    {
      cwd: dir,
      env: { ...env, ...options.env },
      encoding: "utf8",
      maxBuffer: 256 * 1024 * 1024,
      timeout: ((options.limit ?? 25) + 15) * 1000,
    },
  );
  return run.stderr;
}

/** What the page heard in answer to its calls, in order. */
function answers(log: string): Answer[] {
  const found: Answer[] = [];
  for (const line of log.split("\n")) {
    const match =
      /^view: <- response id=\d+ (result|error)(?: code=(\S+))?(?: message=(.*))?(?:=(.*))?$/.exec(
        line,
      );
    if (!match) continue;
    if (line.includes(" result=")) {
      found.push({ ok: true, result: JSON.parse(line.slice(line.indexOf(" result=") + 8)) });
    } else {
      found.push({ ok: false, code: match[2], message: match[3] });
    }
  }
  return found;
}

function ask(calls: Call[], options: Parameters<typeof record>[1] = {}): Answer[] {
  const log = record(calls, { ...options, args: options.args ?? ["."] });
  const heard = answers(log);
  assert.equal(heard.length, calls.length, `every call was answered:\n${log.slice(-1500)}`);
  return heard;
}

const failed = (code: string) => (answer: Answer) => !answer.ok && answer.code === code;

test(
  "a listing has the folders first, then the files, in the order people read them",
  { skip },
  () => {
    writeFileSync(join(dir, "b.txt"), "");
    writeFileSync(join(dir, "A.txt"), "");
    writeFileSync(join(dir, "file10"), "");
    writeFileSync(join(dir, "file9"), "");
    writeFileSync(join(dir, "Zeta.txt"), "");
    mkdirSync(join(dir, "zdir"));
    mkdirSync(join(dir, "Adir"));
    const [list] = ask([["list", { dir: "" }]]);
    assert.deepEqual(
      list!.result.entries.map((e: { name: string }) => e.name),
      ["Adir", "zdir", "A.txt", "b.txt", "file9", "file10", "Zeta.txt"],
    );
    assert.equal(list!.result.truncated, false);
  },
);

test(
  "the listing says which entries are links, and a link to nothing is a file of no size",
  { skip },
  () => {
    writeFileSync(join(dir, "real.txt"), "12345");
    mkdirSync(join(dir, "realdir"));
    symlinkSync("real.txt", join(dir, "to-file"));
    symlinkSync("realdir", join(dir, "to-dir"));
    symlinkSync("nowhere", join(dir, "to-nothing"));
    const [list] = ask([["list", { dir: "" }]]);
    const byName = Object.fromEntries(list!.result.entries.map((e: any) => [e.name, e]));
    assert.deepEqual(byName["to-file"], {
      name: "to-file",
      path: "to-file",
      kind: "file",
      link: true,
      size: 5,
    });
    // (A link to a directory has the size the system gives that directory.)
    assert.deepEqual(
      { ...byName["to-dir"], size: 0 },
      { name: "to-dir", path: "to-dir", kind: "dir", link: true, size: 0 },
    );
    assert.deepEqual(byName["to-nothing"], {
      name: "to-nothing",
      path: "to-nothing",
      kind: "file",
      link: true,
      size: 0,
    });
    assert.deepEqual(byName["real.txt"], {
      name: "real.txt",
      path: "real.txt",
      kind: "file",
      size: 5,
    });
    assert.deepEqual(byName["realdir"], { name: "realdir", path: "realdir", kind: "dir", size: 0 });
  },
);

test(
  "a file is created only where nothing is: nothing is truncated, and no link is written through",
  { skip },
  () => {
    writeFileSync(join(dir, "kept.txt"), "keep me");
    symlinkSync(join(dir, "target-of-the-link"), join(dir, "dangling"));
    const [existing, dangling, fresh, nested, again] = ask([
      ["create", { path: "kept.txt", kind: "file" }],
      ["create", { path: "dangling", kind: "file" }],
      ["create", { path: "new.txt", kind: "file" }],
      ["create", { path: "deep/er/new.md", kind: "file" }],
      ["create", { path: "deep", kind: "dir" }],
    ]);
    assert.ok(failed("failed")(existing!));
    assert.match(existing!.message!, /already exists/);
    assert.ok(failed("failed")(dangling!));
    assert.deepEqual(fresh!.result, { path: "new.txt" });
    assert.deepEqual(nested!.result, { path: "deep/er/new.md" });
    assert.ok(failed("failed")(again!));
    assert.equal(readFileSync(join(dir, "kept.txt"), "utf8"), "keep me");
    assert.equal(
      existsSync(join(dir, "target-of-the-link")),
      false,
      "a link is not written through",
    );
    assert.equal(readFileSync(join(dir, "new.txt"), "utf8"), "");
    assert.equal(readFileSync(join(dir, "deep/er/new.md"), "utf8"), "");
  },
);

test(
  "rename moves a file and its buffer, refuses a name that is taken, and delete spares the project",
  { skip },
  () => {
    writeFileSync(join(dir, "a.txt"), "one");
    writeFileSync(join(dir, "b.txt"), "two");
    const [, renamed, taken, , removed, root] = ask([
      ["open", { path: "a.txt" }],
      ["rename", { from: "a.txt", to: "sub/c.txt" }],
      ["rename", { from: "b.txt", to: "sub/c.txt" }],
      ["create", { path: "gone.txt", kind: "file" }],
      ["delete", { path: "gone.txt" }],
      ["delete", { path: "." }],
    ]);
    assert.equal(renamed!.result.path, "sub/c.txt");
    assert.equal(renamed!.result.buffers[0].path, "sub/c.txt");
    assert.ok(failed("failed")(taken!));
    assert.deepEqual(removed!.result, { path: "gone.txt" });
    assert.ok(failed("failed")(root!));
    assert.equal(existsSync(join(dir, "a.txt")), false);
    assert.equal(readFileSync(join(dir, "sub/c.txt"), "utf8"), "one");
    assert.equal(readFileSync(join(dir, "b.txt"), "utf8"), "two");
  },
);

test(
  "params that are not what a call asks are refused with bad-params, and nothing runs",
  { skip },
  () => {
    writeFileSync(join(dir, "a.txt"), "x");
    const place = { anchor: 0, head: 0, top: 0 };
    const ui = { tabs: [], active: null, expanded: [], places: {}, recent: [], preview: [] };
    const answers = ask([
      ["open", {}],
      ["open", { path: "" }],
      ["open", { path: 5 }],
      ["save", { path: "a.txt" }],
      ["edit", { path: "a.txt", base: 1, edits: [[0, 0]] }],
      ["edit", { path: "a.txt", base: 1.5, edits: [] }],
      ["slice", { path: "a.txt", version: 1, from: -1 }],
      ["search", { query: "" }],
      ["create", { path: "x", kind: "link" }],
      ["buffer", { path: "a.txt", action: "explode" }],
      ["handoff", { kind: "quit" }],
      ["ui", { ui: { ...ui, places: undefined } }],
      ["ui", { ui: { ...ui, places: [] } }],
      ["ui", { ui: { ...ui, places: [place] } }],
      ["ui", { ui: { ...ui, places: { "a.txt": 5 } } }],
      ["ui", { ui: { ...ui, places: { "a.txt": { anchor: 0, head: 0 } } } }],
      ["ui", { ui: { ...ui, tabs: [1] } }],
      ["settings", { settings: { vim: "yes" } }],
      ["nothing-by-this-name", {}],
    ]);
    for (const [index, answer] of answers.entries()) {
      if (index === answers.length - 1) assert.ok(failed("not-found")(answer), `call ${index}`);
      else assert.ok(failed("bad-params")(answer), `call ${index}: ${JSON.stringify(answer)}`);
    }
    assert.equal(readFileSync(join(dir, "a.txt"), "utf8"), "x");
  },
);

test(
  "keys of the view's places that look like a trap are only keys, and the rest are given back as they came",
  { skip },
  () => {
    const place = { anchor: 1, head: 2, top: 3 };
    const places = JSON.parse(
      `{"__proto__": ${JSON.stringify(place)}, "constructor": ${JSON.stringify(place)}, "toString": ${JSON.stringify(place)}, "src/a.ts": ${JSON.stringify(place)}}`,
    );
    const ui = {
      tabs: ["src/a.ts"],
      active: "src/a.ts",
      expanded: [],
      places,
      recent: [],
      preview: [],
    };
    const [kept, boot, , bootAgain] = ask([
      ["ui", { ui }],
      ["boot", {}],
      ["ui", { ui: { ...ui, active: null, places: {} } }],
      ["boot", {}],
    ]);
    assert.deepEqual(kept!.result, {});
    // (The Node program leaves `__proto__` out, which zod does for a record; the Lua one keeps it as
    // the key it is. Neither lets it reach the base of every object.)
    assert.deepEqual(
      Object.keys(boot!.result.ui.places)
        .filter((key) => key !== "__proto__")
        .sort(),
      ["constructor", "src/a.ts", "toString"],
    );
    assert.deepEqual(boot!.result.ui.places["src/a.ts"], place);
    assert.equal(bootAgain!.result.ui.active, null);
    assert.deepEqual(bootAgain!.result.ui.places, {});
    assert.equal(({} as any).anchor, undefined, "nothing was put on the base of every object");
  },
);

test("boot tells what the view starts from, with null where there is nothing", { skip }, () => {
  const [boot] = ask([["boot", {}]]);
  const result = boot!.result;
  assert.equal(result.root, realpathSync(dir));
  assert.equal(result.rootName, dir.split("/").pop());
  assert.equal(result.git, null);
  assert.deepEqual(result.buffers, []);
  assert.deepEqual(result.problems, []);
  assert.equal(result.ui.active, null);
  assert.deepEqual(result.ui.places, {});
  assert.deepEqual(result.settings, {
    vim: false,
    wrap: false,
    fontSize: 13,
    lineNumbers: "on",
    tabSize: 2,
    insertSpaces: true,
    whitespace: false,
    autoSave: false,
    sidebar: true,
    sidebarWidth: 240,
  });
  assert.equal(result.handoff, true);
  assert.equal(result.closeQuits, false);
  assert.equal(result.remote, false);
  assert.equal(typeof result.hostname, "string");
  assert.equal(typeof result.version, "string");
});

test("the command line names the files to open, and where the cursor goes", { skip }, () => {
  writeFileSync(join(dir, "a.txt"), "one\ntwo\n");
  writeFileSync(join(dir, "x:5"), "a file that has a place's name\n");
  const boot = (...args: string[]) => ask([["boot", {}]], { args })[0]!.result;
  const at = boot("a.txt:2:3");
  assert.deepEqual(at.focus, { path: "a.txt", line: 2, column: 3 });
  assert.deepEqual(at.ui.tabs, ["a.txt"]);
  assert.equal(at.ui.active, "a.txt");
  assert.equal(at.closeQuits, true);
  assert.deepEqual(boot("+7", "a.txt").focus, { path: "a.txt", line: 7 });
  const named = boot("x:5");
  assert.equal(named.focus, undefined, "a file with that very name is that file");
  assert.deepEqual(named.ui.tabs, ["x:5"]);
  assert.equal(boot(".").closeQuits, false);
  assert.deepEqual(boot(".").ui.tabs, []);
});

test("--version and --help are answered without a terminal", { skip }, () => {
  const run = (...args: string[]) =>
    spawnSync(PROGRAM[0]!, [...PROGRAM.slice(1), ...args], { cwd: dir, env, encoding: "utf8" });
  const version = run("--version");
  assert.equal(version.status, 0);
  assert.match(version.stdout, /^\d+\.\d+\.\d+\n$/);
  assert.match(run("-h").stdout, /^usage: textpen/);
  const unknown = run("--nonsense");
  assert.equal(unknown.status, 2);
  assert.match(unknown.stderr, /unknown option --nonsense/);
});

test("started with no terminal at all, it says it needs one", { skip }, () => {
  writeFileSync(join(dir, "a.txt"), "x\n");
  const run = spawnSync(PROGRAM[0]!, [...PROGRAM.slice(1), "a.txt"], {
    cwd: dir,
    env,
    encoding: "utf8",
    input: "",
    timeout: 15_000,
  });
  assert.equal(run.status, 1);
  assert.match(run.stderr, /textpen: needs a terminal to edit in/);
});

test("settings are clamped and kept where the XDG directories say", { skip }, () => {
  const settings = {
    vim: true,
    wrap: true,
    fontSize: 99,
    lineNumbers: "relative",
    tabSize: 0,
    insertSpaces: false,
    whitespace: true,
    autoSave: true,
    sidebar: false,
    sidebarWidth: 10,
  };
  const [saved] = ask([["settings", { settings }]]);
  assert.equal(saved!.result.settings.fontSize, 28);
  assert.equal(saved!.result.settings.tabSize, 1);
  assert.equal(saved!.result.settings.sidebarWidth, 160);
  const file = JSON.parse(readFileSync(join(dir, "config", "textpen", "settings.json"), "utf8"));
  assert.equal(file.fontSize, 28);
  assert.equal(file.lineNumbers, "relative");
  const [boot] = ask([["boot", {}]]);
  assert.equal(boot!.result.settings.vim, true);
});

test(
  "edits count in UTF-16 units, as the editor does, and a file is written as it was read",
  { skip },
  () => {
    // "a", then ş (one unit), then an emoji (two units), then "z": the emoji is at 2..4.
    writeFileSync(join(dir, "u.txt"), "﻿aş😀z\r\nlast\r\n");
    const [opened, edited, , saved, again] = ask([
      ["open", { path: "u.txt" }],
      [
        "edit",
        {
          path: "u.txt",
          base: 1,
          edits: [
            [4, 5, "Z"],
            [2, 4, "<>"],
          ],
        },
      ],
      ["edit", { path: "u.txt", base: 2, edits: [[0, 99, "x"]] }],
      ["save", { path: "u.txt", version: 2 }],
      ["edit", { path: "u.txt", base: 1, edits: [[3, 3, "!"]] }],
    ]);
    assert.equal(opened!.result.text, "aş😀z\nlast\n");
    assert.equal(opened!.result.eol, "crlf");
    assert.equal(opened!.result.bom, true);
    assert.equal(edited!.result.version, 2);
    assert.equal(edited!.result.dirty, true);
    assert.ok(saved!.ok && saved!.result.saved === true);
    assert.ok(failed("out-of-sync")(again!), "an edit made on a version that is gone");
    assert.equal(readFileSync(join(dir, "u.txt"), "utf8"), "﻿aş<>Z\r\nlast\r\n");
  },
);

test("a file that cannot be text is not opened as one", { skip }, () => {
  writeFileSync(join(dir, "b.bin"), Buffer.from([1, 2, 0, 3]));
  writeFileSync(join(dir, "latin1.txt"), Buffer.from([0x63, 0x61, 0x66, 0xe9]));
  const [binary, latin1, directory, missing] = ask([
    ["open", { path: "b.bin" }],
    ["open", { path: "latin1.txt" }],
    ["open", { path: "." }],
    ["open", { path: "not/there/yet.md" }],
  ]);
  assert.equal(binary!.result.reason, "binary");
  assert.equal(latin1!.result.reason, "binary");
  assert.equal(directory!.result.reason, "directory");
  assert.equal(missing!.result.missing, true);
  assert.equal(missing!.result.text, "");
});

test(
  "a folder of thousands is cut at five thousand, and the files of a big project come a message's worth at a time",
  { skip, timeout: 60_000 },
  () => {
    mkdirSync(join(dir, "many"));
    for (let i = 0; i < 5100; i += 1)
      writeFileSync(join(dir, "many", `f${String(i).padStart(4, "0")}`), "");
    // 3,600 files whose paths are 300 characters: more than one message holds.
    const folder = `d${"x".repeat(98)}`;
    mkdirSync(join(dir, folder));
    const names: string[] = [];
    for (let i = 0; i < 3600; i += 1) {
      const name = `${String(i).padStart(4, "0")}${"y".repeat(196)}`;
      writeFileSync(join(dir, folder, name), "");
      names.push(`${folder}/${name}`);
    }
    const [listing, page] = ask([
      ["list", { dir: "many" }],
      ["files", {}],
    ]);
    assert.equal(listing!.result.entries.length, 5000);
    assert.equal(listing!.result.truncated, true);
    assert.ok(page!.result.next > 0 && page!.result.next < 3600 + 5100, "the first page is a part");
    assert.ok(JSON.stringify(page!.result).length <= 1024 * 1024, "a page fits a message");
    const [rest] = ask([["files", { from: page!.result.next }]]);
    const all = [...page!.result.files, ...rest!.result.files];
    assert.equal(new Set(all).size, 3600 + 5100, "every file, once");
    assert.equal(rest!.result.next, null);
    assert.ok(names.every((name) => all.includes(name)));
  },
);

test(
  "search finds words, whole words, patterns and refuses a pattern that is not one",
  { skip },
  () => {
    writeFileSync(join(dir, "one.ts"), "const order = 1;\nconst orders = [order];\n");
    writeFileSync(join(dir, "two.md"), "No Order here\n");
    writeFileSync(join(dir, "wide.txt"), `${"ş".repeat(5)}😀 order ${"x".repeat(300)} order\n`);
    const q = (query: string, o: Record<string, boolean> = {}): Call => ["search", { query, ...o }];
    const [plain, whole, pattern, bad, wide, none] = ask([
      q("order"),
      q("order", { caseSensitive: true, wholeWord: true }),
      q("orders?\\b", { regex: true, caseSensitive: true }),
      q("(", { regex: true }),
      q("order", { caseSensitive: true }),
      q("nothing-like-this"),
    ]);
    assert.equal(plain!.result.matches, 6);
    assert.equal(whole!.result.matches, 4);
    assert.deepEqual(
      whole!.result.files[0].matches.map((m: any) => [m.line, m.text.slice(m.start, m.end)]),
      [
        [1, "order"],
        [2, "order"],
      ],
    );
    assert.equal(pattern!.result.matches, 5);
    assert.ok(failed("failed")(bad!));
    assert.match(bad!.message!, /Not a regular expression/);
    const long = wide!.result.files.find((f: any) => f.path === "wide.txt").matches;
    // The first match is after five ş and an emoji (two units) and a space: unit 8, and the line fits.
    assert.deepEqual([long[0].start, long[0].end, long[0].offset], [8, 13, 0]);
    // A long line is cut around the match: the second is far along it.
    assert.equal(long[1].offset > 0, true);
    assert.equal(long[1].text.slice(long[1].start, long[1].end), "order");
    assert.ok(long[1].text.length <= 160);
    assert.equal(none!.result.matches, 0);
    assert.equal(none!.result.truncated, false);
  },
);

test(
  "the files of a project and the results of a search come in the order a listing has",
  { skip: skip || !LUA },
  () => {
    // (The Node program orders a search by plain `localeCompare`, which puts file10 before file9.)
    for (const name of ["file10.txt", "file9.txt", "B.txt", "a.txt"])
      writeFileSync(join(dir, name), "needle\n");
    const [search] = ask([["search", { query: "needle" }]]);
    assert.deepEqual(
      search!.result.files.map((f: any) => f.path),
      ["a.txt", "B.txt", "file9.txt", "file10.txt"],
    );
  },
);

test(
  "a git repository is read for the quick open and for the changes, and a slow git is given up on",
  { skip },
  () => {
    const git = (...args: string[]) => spawnSync("git", args, { cwd: dir, encoding: "utf8" });
    if (git("--version").status !== 0) return;
    git("init", "-q", "-b", "trunk");
    git("config", "user.email", "t@example.com");
    git("config", "user.name", "t");
    writeFileSync(join(dir, "kept.txt"), "1");
    writeFileSync(join(dir, ".gitignore"), "ignored.txt\n");
    git("add", ".");
    git("commit", "-q", "-m", "one");
    writeFileSync(join(dir, "kept.txt"), "2");
    writeFileSync(join(dir, "new file.txt"), "");
    writeFileSync(join(dir, "ignored.txt"), "");
    const [boot, files] = ask([
      ["boot", {}],
      ["files", {}],
    ]);
    assert.equal(boot!.result.git.branch, "trunk");
    assert.deepEqual(boot!.result.git.changes, {
      "kept.txt": "modified",
      "new file.txt": "untracked",
    });
    assert.deepEqual([...files!.result.files].sort(), [".gitignore", "kept.txt", "new file.txt"]);
    assert.equal(files!.result.next, null);
  },
);

test(
  "a git that does not answer is given up on after 15 seconds, and the files are walked instead",
  { skip, timeout: 60_000 },
  () => {
    mkdirSync(join(dir, "bin"));
    const stub = join(dir, "bin", "git");
    writeFileSync(stub, "#!/bin/sh\nexec sleep 40\n");
    chmodSync(stub, 0o755);
    mkdirSync(join(dir, "sub"));
    mkdirSync(join(dir, "node_modules"));
    writeFileSync(join(dir, "sub", "x.txt"), "");
    writeFileSync(join(dir, "node_modules", "skipped.js"), "");
    const started = Date.now();
    const [files] = ask([["files", {}]], {
      limit: 45,
      env: { PATH: `${join(dir, "bin")}:${process.env.PATH}` },
    });
    const took = Date.now() - started;
    assert.deepEqual([...files!.result.files].sort(), ["bin/git", "sub/x.txt"]);
    assert.ok(took >= 14_000 && took < 30_000, `it waited ${took} ms for git`);
  },
);

test(
  "the changes topic tells the disk at once, then what changed under a listed directory or in an open file",
  { skip, timeout: 60_000 },
  () => {
    writeFileSync(join(dir, "c.txt"), "first");
    // A moment into the run, another program writes: the open file changes, and a file appears in the
    // directory the view listed.
    spawn("sh", ["-c", "sleep 1.2; echo second > c.txt; echo new > new.txt"], {
      cwd: dir,
      stdio: "ignore",
    });
    const log = record(
      [
        ["list", { dir: "" }],
        ["open", { path: "c.txt" }],
      ],
      { hostArgs: ["--subscribe", "changes"], args: ["."], then: ["--then", "wait"], limit: 6 },
    );
    const pushes = [...log.matchAll(/view: <- push id=\d+ value=(.*)$/gm)].map((m) =>
      JSON.parse(m[1]!),
    );
    assert.ok(pushes.length >= 2, `two pushes:\n${log.slice(-1200)}`);
    assert.deepEqual(pushes[0], { buffers: [], dirs: [], git: null });
    assert.equal(pushes[1].git, undefined, "git said what it said before");
    assert.equal(pushes[1].buffers[0].path, "c.txt");
    assert.equal(pushes[1].buffers[0].version, 2, "a buffer without changes is read again");
    assert.deepEqual(pushes[1].dirs, [""]);
    // The directory is told again until the view lists it again; the file, once.
    for (const later of pushes.slice(2)) {
      assert.deepEqual(later.buffers, []);
      assert.deepEqual(later.dirs, [""]);
    }
  },
);

test("quitting by hangup keeps what is not saved as a draft", { skip }, () => {
  writeFileSync(join(dir, "a.txt"), "world\n");
  record(
    [
      ["open", { path: "a.txt" }],
      ["edit", { path: "a.txt", base: 1, edits: [[5, 5, "!"]] }],
    ],
    { then: ["--then", "hangup"], args: ["a.txt"] },
  );
  assert.equal(readFileSync(join(dir, "a.txt"), "utf8"), "world\n");
  const drafts = join(dir, "state", "textpen", "drafts");
  assert.equal(readdirSync(drafts).length, 1);
  const draft = JSON.parse(readFileSync(join(drafts, readdirSync(drafts)[0]!), "utf8"));
  assert.equal(draft.text, "world!\n");
  assert.equal(draft.path, join(realpathSync(dir), "a.txt"));
});

test(
  "a shell is given the project as its directory and told it is inside textpen, and the view comes back",
  { skip },
  () => {
    mkdirSync(join(dir, "proj"));
    const shell = join(dir, "shell");
    writeFileSync(
      shell,
      `#!/bin/sh\necho "$PWD $TEXTPEN_SHELL" > ${join(dir, "where.out")}\nexit 3\n`,
      { mode: 0o755 },
    );
    env.SHELL = shell;
    const log = record([["handoff", { kind: "shell" }]], {
      then: ["--on-view", "2", "--then", "interrupt"],
      args: ["proj"],
    });
    assert.equal((log.match(/view: started from the bundle/g) ?? []).length, 2);
    // (/tmp is a link on a Mac: the shell is in the same directory by whichever name.)
    assert.match(readFileSync(join(dir, "where.out"), "utf8"), /proj 1\n$/);
  },
);

test(
  "the person's own editor is run, with textpen passed over, in a terminal that shows no views",
  { skip },
  () => {
    writeFileSync(join(dir, "a.txt"), "world\n");
    env.VISUAL = "textpen";
    env.EDITOR = "cat -n";
    const run = spawnSync(
      HOST,
      ["--no-host", "--passthrough", "--limit", "15", "--", ...PROGRAM, "a.txt"],
      {
        cwd: dir,
        env,
        encoding: "utf8",
        timeout: 30_000,
      },
    );
    assert.match(run.stdout, /1\tworld/);
    assert.match(run.stderr, /exited with status 0/);
  },
);

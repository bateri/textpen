// The built program as the view meets it: the calls and the topic it gives the SDK, on a
// project of its own; what a terminal is given of it (its icons, the files of its view); and
// what it says of itself on the command line. Run against the built program (`npm run build`).

import { bundleFromDirectory } from "@pencere/sdk";
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  utimesSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { after, afterEach, beforeEach, test } from "node:test";

import { Textpen } from "../dist/server/app.js";
import { ICONS } from "../dist/server/icon.js";
import { CHANGES_EVERY, createProgram } from "../dist/server/program.js";
import { WIRE_BUDGET, wireSize } from "../dist/server/wire.js";

const main = new URL("../dist/server/main.js", import.meta.url).pathname;
const view = new URL("../dist/view/", import.meta.url);
const manifest = JSON.parse(readFileSync(new URL("../package.json", import.meta.url), "utf8"));

let dir: string;
let home: string;
let textpen: Textpen;
let program: ReturnType<typeof createProgram>;
const saved = { config: process.env.XDG_CONFIG_HOME, state: process.env.XDG_STATE_HOME };

beforeEach(() => {
  dir = mkdtempSync(join(tmpdir(), "textpen-program-"));
  home = mkdtempSync(join(tmpdir(), "textpen-home-"));
  // The program keeps its settings and drafts where these say, when it is made.
  process.env.XDG_CONFIG_HOME = join(home, "config");
  process.env.XDG_STATE_HOME = join(home, "state");
  writeFileSync(join(dir, "notes.md"), "# Notes\n");
  textpen = new Textpen(dir);
  program = createProgram(textpen);
});

afterEach(() => {
  rmSync(dir, { recursive: true, force: true });
  rmSync(home, { recursive: true, force: true });
});

after(() => {
  for (const [name, value] of [
    ["XDG_CONFIG_HOME", saved.config],
    ["XDG_STATE_HOME", saved.state],
  ] as const) {
    if (value === undefined) delete process.env[name];
    else process.env[name] = value;
  }
});

/** A call of the view, as the SDK hands it to the program. */
function ask(method: string, params?: unknown): Promise<any> {
  const handler = program.calls[method];
  assert.ok(handler, `there is a call ${method}`);
  return Promise.resolve(
    handler(params, { method, signal: new AbortController().signal, progress: () => {} }),
  );
}

const codeOf = (code: string) => (error: unknown) => (error as { code?: string }).code === code;

test("says the manifest's version", () => {
  assert.equal(
    execFileSync(process.execPath, [main, "--version"], { encoding: "utf8" }).trim(),
    manifest.version,
  );
});

test("boots the view on its project, with what is open", async () => {
  const boot = await ask("boot");
  assert.equal(boot.root, dir);
  assert.equal(boot.version, manifest.version);
  assert.equal(boot.handoff, false, "no terminal has asked for the pane yet");
  assert.deepEqual(boot.problems, []);
});

test("an edit made on a version is saved, and one made on another is refused", async () => {
  await ask("open", { path: "notes.md" });
  const edited = await ask("edit", { path: "notes.md", base: 1, edits: [[0, 0, "x "]] });
  assert.equal(edited.version, 2);
  await assert.rejects(
    ask("edit", { path: "notes.md", base: 1, edits: [[0, 0, "y"]] }),
    codeOf("out-of-sync"),
  );
  const result = await ask("save", { path: "notes.md", version: 2 });
  assert.equal(result.saved, true);
  assert.equal(readFileSync(join(dir, "notes.md"), "utf8"), "x # Notes\n");
});

test("a call that is not what the program asks is told so, and what the disk refuses is failed", async () => {
  await assert.rejects(ask("save", { path: "notes.md" }), codeOf("bad-params"));
  await assert.rejects(ask("edit", "not an object"), codeOf("bad-params"));
  await assert.rejects(ask("create", { path: "notes.md", kind: "file" }), codeOf("failed"));
});

test("a text too long for a message is read in pieces that each fit one", async () => {
  // Every character outside ASCII crosses as six bytes: three million of them are 18 MB.
  const text = ("ş".repeat(99) + "\n").repeat(30_000);
  writeFileSync(join(dir, "long.txt"), text);
  const opened = await ask("open", { path: "long.txt" });
  assert.equal(opened.text, "");
  assert.equal(opened.partial.length, text.length);
  const pieces: string[] = [];
  for (let from = 0; from < text.length;) {
    const piece = await ask("slice", { path: "long.txt", version: 1, from });
    assert.ok(wireSize(piece.text) <= WIRE_BUDGET, "a piece fits a message");
    assert.ok(piece.end > from);
    pieces.push(piece.text);
    from = piece.end;
  }
  assert.ok(pieces.length > 15, "it took many messages");
  assert.equal(pieces.join(""), text);
  await assert.rejects(
    ask("slice", { path: "long.txt", version: 9, from: 0 }),
    codeOf("out-of-sync"),
  );
});

test("the files of a big project are listed a message's worth at a time", async () => {
  const names: string[] = [];
  for (let d = 0; d < 20; d += 1) {
    const folder = join(dir, `dir-${String(d).padStart(2, "0")}-${"x".repeat(60)}`);
    mkdirSync(folder);
    for (let f = 0; f < 600; f += 1) {
      const name = `file-${String(f).padStart(4, "0")}-${"y".repeat(100)}.txt`;
      writeFileSync(join(folder, name), "");
      names.push(name);
    }
  }
  const files: string[] = [];
  let pages = 0;
  for (let from: number | null = 0; from !== null;) {
    const page: { files: string[]; next: number | null } = await ask("files", { from });
    assert.ok(wireSize(JSON.stringify(page)) <= 1024 * 1024, "a page fits a message");
    files.push(...page.files);
    pages += 1;
    from = page.next;
  }
  assert.ok(pages > 1, "it took more than one message");
  assert.equal(files.length, names.length + 1, "the files, and notes.md");
});

test("the changes topic says what changed on disk once, and stops when it is over", async () => {
  await ask("open", { path: "notes.md" });
  const pushed: any[] = [];
  const controller = new AbortController();
  const stop = await program.topics.changes!(
    {},
    {
      topic: "changes",
      signal: controller.signal,
      push: (value: unknown) => {
        pushed.push(value);
        return true;
      },
    },
  );
  assert.equal(typeof stop, "function");
  await new Promise((resolve) => setTimeout(resolve, 200));
  assert.equal(pushed.length, 1, "the first look is told at once");
  assert.deepEqual(pushed[0], { git: null, buffers: [], dirs: [] });

  writeFileSync(join(dir, "notes.md"), "# Changed\n");
  utimesSync(join(dir, "notes.md"), new Date(), new Date(Date.now() + 5000));
  await new Promise((resolve) => setTimeout(resolve, CHANGES_EVERY + 500));
  assert.equal(pushed.length, 2, "what changed is told once, and the quiet turns say nothing");
  assert.equal(pushed[1].buffers[0].path, "notes.md");
  assert.equal(pushed[1].git, undefined, "git said what it said before");

  controller.abort();
  (stop as () => void)();
  writeFileSync(join(dir, "notes.md"), "# Again\n");
  utimesSync(join(dir, "notes.md"), new Date(), new Date(Date.now() + 9000));
  await new Promise((resolve) => setTimeout(resolve, CHANGES_EVERY + 500));
  assert.equal(pushed.length, 2, "nothing is pushed to a subscription that is over");
});

test("handing the pane over asks the terminal's desk, and says when there is none", async () => {
  assert.deepEqual(await ask("handoff", { kind: "shell" }), { state: "unavailable" });
  textpen.desk = { shell: () => "scheduled" };
  assert.deepEqual(await ask("handoff", { kind: "shell" }), { state: "scheduled" });
  await assert.rejects(ask("handoff", { kind: "quit" }), codeOf("bad-params"));
});

test("quitting without saving leaves no draft behind", async () => {
  await ask("open", { path: "notes.md" });
  await ask("edit", { path: "notes.md", base: 1, edits: [[0, 0, "unsaved "]] });
  await ask("discard");
  textpen.ending();
  assert.equal(existsSync(join(home, "state", "textpen", "drafts")), false);
});

interface Icon {
  src: string;
  mimeType?: string;
  sizes?: string[];
  theme?: string;
}

/**
 * What a pencere terminal takes of an icon list (specification §7.3): a base64 PNG in a data:
 * URI, at most 64 KiB and 512 pixels a side, four of them at most; a picture for each ground,
 * at the 32 pixels a strip asks for.
 */
function checkIcons(icons: Icon[] | undefined, owner: string) {
  assert.ok(Array.isArray(icons) && icons.length > 0 && icons.length <= 4, `${owner} has icons`);
  for (const icon of icons) {
    const match = /^data:image\/png;base64,([A-Za-z0-9+/=]+)$/.exec(icon.src);
    assert.ok(match, `${owner}: an icon is a base64 PNG in a data: URI`);
    const bytes = Buffer.from(match[1]!, "base64");
    assert.deepEqual([...bytes.subarray(0, 8)], [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);
    assert.ok(bytes.length <= 64 * 1024, `${owner}: an icon is at most 64 KiB`);
    const [width, height] = [bytes.readUInt32BE(16), bytes.readUInt32BE(20)];
    assert.ok(width <= 512 && height <= 512, `${owner}: an icon is at most 512 pixels a side`);
    assert.equal(width, height, `${owner}: an icon is square`);
    assert.deepEqual(icon.sizes, [`${width}x${height}`]);
    assert.equal(icon.mimeType, "image/png");
  }
  for (const theme of ["dark", "light"]) {
    assert.ok(
      icons.some((icon) => icon.theme === theme && icon.sizes?.[0] === "32x32"),
      `${owner} has a 32-pixel icon for a ${theme} ground`,
    );
  }
}

test("gives its icon as a terminal takes one", () => {
  checkIcons(ICONS, "textpen");
});

test("gives its view as a bundle a terminal can keep", async () => {
  const bundle = await bundleFromDirectory(view);
  assert.equal(bundle.manifest.entry, "index.html");
  const entry = bundle.manifest.files.find((file) => file.path === "index.html");
  assert.equal(entry?.type, "text/html");
  assert.ok(bundle.manifest.files.length > 50, "each language is a file of its own");
  assert.equal(existsSync(new URL("../dist/single", import.meta.url)), false);
});

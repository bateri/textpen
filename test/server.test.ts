// The program's side: the buffers it holds for the view, the files it writes, the drafts
// it keeps, and the project it reads. Run against the built program (`npm run build:server`).

import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync, readdirSync, rmSync, utimesSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, beforeEach, test } from "node:test";

import { Buffers, OutOfSync } from "../dist/server/buffers.js";
import { Project } from "../dist/server/project.js";

let dir: string;
let state: string;
let project: Project;
let buffers: Buffers;

beforeEach(() => {
  dir = mkdtempSync(join(tmpdir(), "textpen-"));
  state = mkdtempSync(join(tmpdir(), "textpen-state-"));
  project = new Project(dir);
  buffers = new Buffers(project, state);
});

afterEach(() => {
  rmSync(dir, { recursive: true, force: true });
  rmSync(state, { recursive: true, force: true });
});

function opened(result: Awaited<ReturnType<Buffers["open"]>>) {
  assert.ok("text" in result, "the file opens as text");
  return result;
}

test("edits apply in order on the version they were made on", async () => {
  writeFileSync(join(dir, "a.txt"), "hello world\n");
  const a = opened(await buffers.open("a.txt"));
  assert.equal(a.version, 1);
  const info = buffers.edit("a.txt", 1, [
    [6, 11, "there"],
    [0, 5, "HELLO"],
  ]);
  assert.equal(info.version, 2);
  assert.equal(info.dirty, true);
  assert.equal(buffers.text("a.txt").text, "HELLO there\n");
  assert.throws(() => buffers.edit("a.txt", 1, [[0, 0, "x"]]), OutOfSync);
  assert.throws(() => buffers.edit("a.txt", 2, [[0, 99, "x"]]), OutOfSync);
});

test("an edit back to the saved text is not a change", async () => {
  writeFileSync(join(dir, "a.txt"), "abc");
  opened(await buffers.open("a.txt"));
  buffers.edit("a.txt", 1, [[3, 3, "d"]]);
  const info = buffers.edit("a.txt", 2, [[3, 4, ""]]);
  assert.equal(info.dirty, false);
});

test("a file is written with the line ends and the BOM it was read with", async () => {
  writeFileSync(join(dir, "w.txt"), "﻿one\r\ntwo\r\n");
  const w = opened(await buffers.open("w.txt"));
  assert.equal(w.text, "one\ntwo\n");
  assert.equal(w.eol, "crlf");
  assert.equal(w.bom, true);
  buffers.edit("w.txt", 1, [[8, 8, "three\n"]]);
  const saved = await buffers.save("w.txt", 2);
  assert.equal(saved.saved, true);
  assert.equal(readFileSync(join(dir, "w.txt"), "utf8"), "﻿one\r\ntwo\r\nthree\r\n");
  buffers.setEol("w.txt", "lf");
  assert.equal(buffers.info("w.txt")!.dirty, true);
  await buffers.save("w.txt", 2);
  assert.equal(readFileSync(join(dir, "w.txt"), "utf8"), "﻿one\ntwo\nthree\n");
});

test("a new file opens empty and comes to be when saved, folders and all", async () => {
  const n = opened(await buffers.open("deep/er/new.md"));
  assert.equal(n.missing, true);
  assert.equal(n.text, "");
  buffers.edit("deep/er/new.md", 1, [[0, 0, "# New\n"]]);
  await buffers.save("deep/er/new.md", 2);
  assert.equal(readFileSync(join(dir, "deep/er/new.md"), "utf8"), "# New\n");
});

test("a file changed on disk is read again, or, with changes here, is a conflict", async () => {
  const path = join(dir, "c.txt");
  writeFileSync(path, "first");
  opened(await buffers.open("c.txt"));
  writeFileSync(path, "second");
  utimesSync(path, new Date(), new Date(Date.now() + 5000));
  const [clean] = await buffers.changedOnDisk();
  assert.equal(clean!.conflict, false);
  assert.equal(buffers.text("c.txt").text, "second");

  buffers.edit("c.txt", clean!.version, [[0, 0, "my "]]);
  writeFileSync(path, "third");
  utimesSync(path, new Date(), new Date(Date.now() + 10_000));
  const [dirty] = await buffers.changedOnDisk();
  assert.equal(dirty!.conflict, true);
  assert.equal(buffers.text("c.txt").text, "my second");
  const refused = await buffers.save("c.txt", dirty!.version);
  assert.equal(refused.saved, false);
  assert.equal(readFileSync(path, "utf8"), "third");
  const forced = await buffers.save("c.txt", dirty!.version, true);
  assert.equal(forced.saved, true);
  assert.equal(readFileSync(path, "utf8"), "my second");
});

test("what is not saved survives the program as a draft, offered the next time", async () => {
  writeFileSync(join(dir, "d.txt"), "kept");
  opened(await buffers.open("d.txt"));
  buffers.edit("d.txt", 1, [[4, 4, " and changed"]]);
  buffers.flushDrafts();
  assert.equal(readdirSync(join(state, "drafts")).length, 1);

  const next = new Buffers(new Project(dir), state);
  const again = opened(await next.open("d.txt"));
  assert.equal(again.text, "kept");
  assert.equal(again.draft?.length, "kept and changed".length);
  const restored = next.draft("d.txt", true);
  assert.equal(restored.text, "kept and changed");
  await next.save("d.txt", restored.version);
  await new Promise((resolve) => setTimeout(resolve, 50));
  assert.deepEqual(readdirSync(join(state, "drafts")), []);
});

test("quitting without saving leaves no draft", async () => {
  writeFileSync(join(dir, "q.txt"), "x");
  opened(await buffers.open("q.txt"));
  buffers.edit("q.txt", 1, [[0, 0, "y"]]);
  buffers.discardAll();
  buffers.flushDrafts();
  assert.deepEqual(
    readdirSync(state)
      .filter((f) => f === "drafts")
      .flatMap((f) => readdirSync(join(state, f))),
    [],
  );
});

test("binary and huge files are not opened as text", async () => {
  writeFileSync(join(dir, "b.bin"), Buffer.from([1, 2, 0, 3]));
  const b = await buffers.open("b.bin");
  assert.ok("reason" in b && b.reason === "binary");
});

test("paths stay in the project for what a model may read", () => {
  assert.equal(project.name(join(dir, "src/a.ts")), "src/a.ts");
  assert.equal(project.name("/elsewhere/x"), "/elsewhere/x");
  assert.throws(() => project.inside("../outside"));
  assert.throws(() => project.inside("/etc/passwd"));
  assert.equal(project.inside("src/../a.ts"), join(dir, "a.ts"));
});

test("a listing has the folders first, then the files, by name", async () => {
  writeFileSync(join(dir, "b.txt"), "");
  writeFileSync(join(dir, "A.txt"), "");
  writeFileSync(join(dir, "file10"), "");
  writeFileSync(join(dir, "file9"), "");
  execFileSync("mkdir", [join(dir, "zdir")]);
  const listing = await project.list("");
  assert.deepEqual(
    listing.entries.map((e) => e.name),
    ["zdir", "A.txt", "b.txt", "file9", "file10"],
  );
});

test("search finds words, whole words and patterns, line by line", async () => {
  writeFileSync(join(dir, "one.ts"), "const order = 1;\nconst orders = [order];\n");
  writeFileSync(join(dir, "two.md"), "No Order here\n");
  const plain = await project.search({
    query: "order",
    regex: false,
    caseSensitive: false,
    wholeWord: false,
  });
  assert.equal(plain.matches, 4);
  const whole = await project.search({
    query: "order",
    regex: false,
    caseSensitive: true,
    wholeWord: true,
  });
  assert.equal(whole.matches, 2);
  assert.deepEqual(
    whole.files[0]!.matches.map((m) => [m.line, m.text.slice(m.start, m.end)]),
    [
      [1, "order"],
      [2, "order"],
    ],
  );
  const pattern = await project.search({
    query: "orders?\\b",
    regex: true,
    caseSensitive: true,
    wholeWord: false,
  });
  assert.equal(pattern.matches, 3);
  await assert.rejects(
    project.search({ query: "(", regex: true, caseSensitive: false, wholeWord: false }),
  );
});

test("git's word on the files: branch, changes, untracked", async (t) => {
  try {
    execFileSync("git", ["--version"], { stdio: "ignore" });
  } catch {
    t.skip("no git here");
    return;
  }
  const git = (...args: string[]) => execFileSync("git", args, { cwd: dir, stdio: "ignore" });
  git("init", "-q", "-b", "trunk");
  git("config", "user.email", "t@example.com");
  git("config", "user.name", "t");
  writeFileSync(join(dir, "kept.txt"), "1");
  git("add", ".");
  git("commit", "-q", "-m", "one");
  writeFileSync(join(dir, "kept.txt"), "2");
  writeFileSync(join(dir, "new file.txt"), "");
  const info = await new Project(dir).git(0);
  assert.equal(info?.branch, "trunk");
  assert.deepEqual(info?.changes, { "kept.txt": "modified", "new file.txt": "untracked" });
});

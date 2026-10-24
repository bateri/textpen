/**
 * The project: the directory textpen was started in (or on), its files, what git says
 * of them, and a search through them. Paths travel relative to the root and
 * `/`-separated; a file outside the root travels as its absolute path.
 */

import { execFile } from "node:child_process";
import { createHash } from "node:crypto";
import { lstat, open, readdir, stat } from "node:fs/promises";
import { basename, isAbsolute, join, relative, resolve, sep } from "node:path";

import type {
  Entry,
  FileIndex,
  GitInfo,
  GitState,
  Listing,
  SearchFile,
  SearchResult,
} from "./types.js";

/** Directories nobody means to browse or search: version control and installed packages. */
const SKIPPED = new Set([
  ".git",
  ".hg",
  ".svn",
  "node_modules",
  ".venv",
  "venv",
  "__pycache__",
  ".mypy_cache",
  ".pytest_cache",
  ".next",
  ".nuxt",
  ".turbo",
  ".cache",
  "target",
]);

const LIST_LIMIT = 5000;
const INDEX_LIMIT = 50_000;

/** A file this large is not searched, and not opened for editing. */
export const TEXT_LIMIT = 8 * 1024 * 1024;
const SEARCH_FILE_LIMIT = 2 * 1024 * 1024;
const SEARCH_MATCH_LIMIT = 2000;
const SEARCH_LINE_WIDTH = 160;

export class Project {
  readonly root: string;
  #git: { at: number; value: GitInfo | null; running?: Promise<GitInfo | null> } | undefined;
  #index: { at: number; value: FileIndex } | undefined;
  #isRepository: boolean | undefined;
  /** Directories the view listed, with their modification time then. */
  readonly #listed = new Map<string, number>();

  constructor(root: string) {
    this.root = resolve(root);
  }

  /** The absolute path of a path the view or the command line gave. */
  absolute(path: string): string {
    return isAbsolute(path) ? resolve(path) : resolve(this.root, path);
  }

  /** How a path travels: relative to the root inside it, absolute outside. */
  name(absolute: string): string {
    const rel = relative(this.root, absolute);
    if (rel === "") return "";
    if (rel.startsWith("..") || isAbsolute(rel)) return absolute;
    return sep === "/" ? rel : rel.split(sep).join("/");
  }

  async list(dir: string): Promise<Listing> {
    const absolute = this.absolute(dir);
    const items = await readdir(absolute, { withFileTypes: true });
    const truncated = items.length > LIST_LIMIT;
    const entries = await Promise.all(
      items.slice(0, LIST_LIMIT).map(async (item): Promise<Entry> => {
        const full = join(absolute, item.name);
        const path = this.name(full);
        if (item.isSymbolicLink()) {
          try {
            const target = await stat(full);
            return {
              name: item.name,
              path,
              kind: target.isDirectory() ? "dir" : "file",
              link: true,
              size: target.size,
            };
          } catch {
            return { name: item.name, path, kind: "file", link: true, size: 0 };
          }
        }
        if (item.isDirectory()) return { name: item.name, path, kind: "dir", size: 0 };
        let size = 0;
        try {
          size = (await lstat(full)).size;
        } catch {
          // Gone between the listing and now: listed with no size.
        }
        return { name: item.name, path, kind: "file", size };
      }),
    );
    entries.sort(
      (a, b) =>
        Number(b.kind === "dir") - Number(a.kind === "dir") ||
        a.name.localeCompare(b.name, undefined, { numeric: true, sensitivity: "base" }),
    );
    try {
      this.#listed.set(this.name(absolute), (await stat(absolute)).mtimeMs);
    } catch {
      // A directory that cannot be watched is listed again only when asked.
    }
    return { dir: this.name(absolute), entries, truncated };
  }

  /** The listed directories whose entries changed since they were listed (or that went). */
  async changedDirs(): Promise<string[]> {
    const changed: string[] = [];
    await Promise.all(
      [...this.#listed].map(async ([dir, mtime]) => {
        try {
          if ((await stat(this.absolute(dir))).mtimeMs !== mtime) changed.push(dir);
        } catch {
          this.#listed.delete(dir);
          changed.push(dir);
        }
      }),
    );
    if (changed.length > 0) this.#index = undefined;
    return changed;
  }

  /** Forgets what was learnt of the files: after the view created, renamed or removed one. */
  forget(): void {
    this.#index = undefined;
    this.#git = undefined;
  }

  /** Every file of the project, for the quick open: git's list in a repository, a walk elsewhere. */
  async files(): Promise<FileIndex> {
    const cached = this.#index;
    if (cached !== undefined && Date.now() - cached.at < 15_000) return cached.value;
    let value: FileIndex | undefined;
    if (await this.isRepository()) {
      try {
        const out = await run(
          "git",
          ["ls-files", "-z", "--cached", "--others", "--exclude-standard"],
          this.root,
          64 * 1024 * 1024,
        );
        const files = [...new Set(out.split("\0").filter(Boolean))];
        value = { files: files.slice(0, INDEX_LIMIT), truncated: files.length > INDEX_LIMIT };
      } catch {
        value = undefined;
      }
    }
    value ??= await this.#walk();
    this.#index = { at: Date.now(), value };
    return value;
  }

  async #walk(): Promise<FileIndex> {
    const files: string[] = [];
    const queue = [this.root];
    while (queue.length > 0 && files.length < INDEX_LIMIT) {
      const dir = queue.shift()!;
      let items;
      try {
        items = await readdir(dir, { withFileTypes: true });
      } catch {
        continue;
      }
      for (const item of items) {
        const full = join(dir, item.name);
        if (item.isDirectory()) {
          if (!SKIPPED.has(item.name)) queue.push(full);
        } else if (item.isFile() || item.isSymbolicLink()) {
          files.push(this.name(full));
        }
      }
    }
    return { files: files.slice(0, INDEX_LIMIT), truncated: files.length >= INDEX_LIMIT };
  }

  async isRepository(): Promise<boolean> {
    if (this.#isRepository === undefined) {
      try {
        await run("git", ["rev-parse", "--is-inside-work-tree"], this.root);
        this.#isRepository = true;
      } catch {
        this.#isRepository = false;
      }
    }
    return this.#isRepository;
  }

  /** What git says of the project: its branch and changed files. Asked at most every few seconds. */
  async git(maxAge = 2500): Promise<GitInfo | null> {
    const cached = this.#git;
    if (cached?.running) return cached.running;
    if (cached !== undefined && Date.now() - cached.at < maxAge) return cached.value;
    if (!(await this.isRepository())) return null;
    const running = readGit(this.root).catch(() => null);
    this.#git = { at: cached?.at ?? 0, value: cached?.value ?? null, running };
    const value = await running;
    this.#git = { at: Date.now(), value };
    return value;
  }

  async search(options: {
    query: string;
    regex: boolean;
    caseSensitive: boolean;
    wholeWord: boolean;
  }): Promise<SearchResult> {
    const started = Date.now();
    const pattern = matcher(options);
    const { files } = await this.files();
    const found: SearchFile[] = [];
    let matches = 0;
    let searched = 0;
    let truncated = false;
    const queue = files.filter((f) => !isAbsolute(f));
    const worker = async () => {
      while (queue.length > 0 && !truncated) {
        const path = queue.shift()!;
        const text = await readText(join(this.root, path), SEARCH_FILE_LIMIT);
        if (text === undefined) continue;
        searched += 1;
        const file = searchText(path, text, pattern, SEARCH_MATCH_LIMIT - matches);
        if (file === undefined) continue;
        found.push(file);
        matches += file.matches.length;
        if (matches >= SEARCH_MATCH_LIMIT) truncated = true;
      }
    };
    await Promise.all(Array.from({ length: 16 }, worker));
    found.sort((a, b) => a.path.localeCompare(b.path));
    return { files: found, matches, searched, truncated, ms: Date.now() - started };
  }
}

/** A key for a path that can name a file: the drafts and the projects are kept by it. */
export function keyOf(path: string): string {
  return createHash("sha256").update(path).digest("hex").slice(0, 24);
}

export function displayName(root: string): string {
  return basename(root) || root;
}

function matcher(options: {
  query: string;
  regex: boolean;
  caseSensitive: boolean;
  wholeWord: boolean;
}): RegExp {
  let source = options.regex ? options.query : options.query.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  if (options.wholeWord) source = `(?<![\\p{L}\\p{N}_])(?:${source})(?![\\p{L}\\p{N}_])`;
  try {
    return new RegExp(source, `gu${options.caseSensitive ? "" : "i"}`);
  } catch (error) {
    throw new Error(
      `Not a regular expression: ${error instanceof Error ? error.message : String(error)}`,
    );
  }
}

function searchText(
  path: string,
  text: string,
  pattern: RegExp,
  room: number,
): SearchFile | undefined {
  let file: SearchFile | undefined;
  const lines = text.split(/\r\n|\r|\n/);
  for (let index = 0; index < lines.length && room > 0; index += 1) {
    const line = lines[index]!;
    pattern.lastIndex = 0;
    let match: RegExpExecArray | null;
    while ((match = pattern.exec(line)) !== null && room > 0) {
      if (match[0].length === 0) {
        pattern.lastIndex += 1;
        continue;
      }
      const start = match.index;
      const end = start + match[0].length;
      // A long line is cut around the match, so that the list shows what matched.
      const offset = line.length <= SEARCH_LINE_WIDTH ? 0 : Math.max(0, start - 40);
      const shown = line.slice(offset, offset + SEARCH_LINE_WIDTH);
      file ??= { path, matches: [] };
      file.matches.push({
        line: index + 1,
        start: start - offset,
        end: Math.min(end - offset, shown.length),
        text: shown,
        offset,
      });
      room -= 1;
    }
  }
  return file;
}

/** A file's text, or undefined when it is too large, unreadable or binary. */
async function readText(path: string, limit: number): Promise<string | undefined> {
  let handle;
  try {
    handle = await open(path, "r");
    const info = await handle.stat();
    if (!info.isFile() || info.size > limit) return undefined;
    const bytes = Buffer.alloc(info.size);
    await handle.read(bytes, 0, info.size, 0);
    if (isBinary(bytes)) return undefined;
    return bytes.toString("utf8");
  } catch {
    return undefined;
  } finally {
    await handle?.close();
  }
}

/** A NUL in the first 8 KiB makes a file binary, as git and grep decide it. */
export function isBinary(bytes: Uint8Array): boolean {
  const end = Math.min(bytes.length, 8192);
  for (let i = 0; i < end; i += 1) if (bytes[i] === 0) return true;
  return false;
}

async function readGit(root: string): Promise<GitInfo> {
  // Where the root is in the repository, as git names it: a path git gives is made the
  // view's by taking this off, whatever links lie between the two (/var and /private/var).
  const prefix = (await run("git", ["rev-parse", "--show-prefix"], root)).trim();
  const out = await run(
    "git",
    ["status", "--porcelain=v2", "--branch", "-z", "--untracked-files=all"],
    root,
    32 * 1024 * 1024,
  );
  const info: GitInfo = { branch: "", ahead: 0, behind: 0, changes: {} };
  const tokens = out.split("\0");
  // git names paths from the top of the repository; the view names them from the root,
  // and does not show what lies outside it.
  const note = (path: string, state: GitState) => {
    if (path.startsWith(prefix)) info.changes[path.slice(prefix.length)] = state;
  };
  for (let i = 0; i < tokens.length; i += 1) {
    const token = tokens[i]!;
    if (token.startsWith("# branch.head ")) {
      info.branch = token.slice(14);
    } else if (token.startsWith("# branch.oid ") && info.branch === "") {
      info.branch = token.slice(13, 20);
    } else if (token.startsWith("# branch.ab ")) {
      const [ahead, behind] = token.slice(12).split(" ");
      info.ahead = Math.abs(Number(ahead) || 0);
      info.behind = Math.abs(Number(behind) || 0);
    } else if (token.startsWith("1 ") || token.startsWith("2 ")) {
      const parts = token.split(" ");
      const xy = parts[1]!;
      const path = parts.slice(token.startsWith("1 ") ? 8 : 9).join(" ");
      if (token.startsWith("2 ")) i += 1; // the original path follows
      note(path, stateOf(xy));
    } else if (token.startsWith("u ")) {
      note(token.split(" ").slice(10).join(" "), "conflict");
    } else if (token.startsWith("? ")) {
      note(token.slice(2), "untracked");
    }
  }
  if (info.branch === "(detached)") {
    try {
      info.branch = (await run("git", ["rev-parse", "--short", "HEAD"], root)).trim();
    } catch {
      // A repository with no commit yet keeps the word.
    }
  }
  return info;
}

function stateOf(xy: string): GitState {
  if (xy.includes("R")) return "renamed";
  if (xy[1] === "D" || xy[0] === "D") return "deleted";
  if (xy[0] === "A") return "added";
  return "modified";
}

function run(command: string, args: string[], cwd: string, maxBuffer = 1024 * 1024) {
  return new Promise<string>((resolvePromise, reject) => {
    execFile(
      command,
      args,
      { cwd, maxBuffer, timeout: 15_000, encoding: "utf8" },
      (error, stdout) => (error ? reject(error) : resolvePromise(stdout)),
    );
  });
}

/**
 * The open files. The program holds each one's text, not the view: what the view sends
 * its program travels through the terminal's input queue in frames of 511 bytes, so it
 * sends what changed, a few hundred milliseconds of typing at a time, and saving is a
 * word to the program rather than the whole file. The text the program holds is also
 * what survives the view — a shell, a restart of the terminal — and, written to a draft,
 * a ⌃C or a dropped connection.
 */

import { constants, mkdirSync, unlinkSync, writeFileSync } from "node:fs";
import { access, mkdir, readFile, stat, unlink, writeFile } from "node:fs/promises";
import { dirname, join } from "node:path";

import { TEXT_LIMIT, isBinary, keyOf } from "./project.js";
import type { Project } from "./project.js";
import type { BufferInfo, Eol, OpenedBuffer, Unopenable } from "./types.js";

/** One edit: the text from `from` to `to` (in UTF-16 units) becomes `insert`. */
export type Edit = [from: number, to: number, insert: string];

interface Open {
  path: string;
  absolute: string;
  text: string;
  /** The text as the file has it, without its BOM and with `\n` for every line end. */
  saved: string;
  version: number;
  eol: Eol;
  /** The line ends the file has: a buffer written with others differs from it. */
  savedEol: Eol;
  bom: boolean;
  mtime: number;
  missing: boolean;
  conflict: boolean;
  readonly: boolean;
  draft?: { at: number; text: string };
}

interface Draft {
  path: string;
  text: string;
  at: number;
}

export class OutOfSync extends Error {
  override name = "OutOfSync";
}

export class Buffers {
  readonly #buffers = new Map<string, Open>();
  readonly #drafts: string;
  readonly #timers = new Map<string, ReturnType<typeof setTimeout>>();

  constructor(
    readonly project: Project,
    stateDirectory: string,
  ) {
    this.#drafts = join(stateDirectory, "drafts");
  }

  get paths(): string[] {
    return [...this.#buffers.keys()];
  }

  info(path: string): BufferInfo | undefined {
    const buffer = this.#buffers.get(path);
    return buffer && infoOf(buffer);
  }

  all(): BufferInfo[] {
    return [...this.#buffers.values()].map(infoOf);
  }

  dirty(): BufferInfo[] {
    return this.all().filter((b) => b.dirty);
  }

  /** The buffer of a file, read from the disk if it is not open; a missing file opens empty. */
  async open(path: string): Promise<OpenedBuffer | Unopenable> {
    const absolute = this.project.absolute(path);
    const name = this.project.name(absolute);
    const open = this.#buffers.get(name);
    if (open) return { ...infoOf(open), text: open.text };

    const read = await readFileText(absolute, name);
    if ("reason" in read) return read;
    const buffer: Open = {
      path: name,
      absolute,
      text: read.text,
      saved: read.text,
      version: 1,
      eol: read.eol,
      savedEol: read.eol,
      bom: read.bom,
      mtime: read.mtime,
      missing: read.missing,
      conflict: false,
      readonly: read.readonly,
    };
    const draft = await this.#readDraft(absolute);
    if (draft && draft.text !== buffer.text) buffer.draft = { at: draft.at, text: draft.text };
    else if (draft) void this.#removeDraft(absolute);
    this.#buffers.set(name, buffer);
    return { ...infoOf(buffer), text: buffer.text };
  }

  /** A file's text as textpen has it: the open buffer's, or the disk's, opening nothing. */
  async peek(path: string): Promise<{ text: string; missing: boolean } | Unopenable> {
    const absolute = this.project.absolute(path);
    const open = this.#buffers.get(this.project.name(absolute));
    if (open) return { text: open.text, missing: open.missing };
    const read = await readFileText(absolute, this.project.name(absolute));
    return "reason" in read ? read : { text: read.text, missing: read.missing };
  }

  /** A piece of a buffer's text, for a view that reads a long one in pieces. */
  slice(path: string, version: number, from: number, to: number): string {
    const buffer = this.#get(path);
    if (version !== buffer.version) {
      throw new OutOfSync(`${path} is at version ${buffer.version}, not ${version}`);
    }
    return buffer.text.slice(from, to);
  }

  /** The current text of an open buffer. */
  text(path: string): OpenedBuffer {
    const buffer = this.#get(path);
    return { ...infoOf(buffer), text: buffer.text };
  }

  /** Applies the view's edits, made on version `base`, in order. */
  edit(path: string, base: number, edits: readonly Edit[]): BufferInfo {
    const buffer = this.#get(path);
    if (base !== buffer.version) {
      throw new OutOfSync(`${path} is at version ${buffer.version}, not ${base}`);
    }
    let text = buffer.text;
    for (const [from, to, insert] of edits) {
      if (!(from >= 0 && from <= to && to <= text.length)) {
        throw new OutOfSync(`An edit of ${path} reaches outside it`);
      }
      text = text.slice(0, from) + insert + text.slice(to);
    }
    buffer.text = text;
    buffer.version += 1;
    this.#scheduleDraft(buffer);
    return infoOf(buffer);
  }

  /**
   * Writes the buffer to its file. A file that changed on disk since it was read is not
   * overwritten unless `force`: the answer says so, and the view asks.
   */
  async save(
    path: string,
    version: number,
    force = false,
  ): Promise<{ saved: true; info: BufferInfo } | { saved: false; info: BufferInfo }> {
    const buffer = this.#get(path);
    if (version !== buffer.version) {
      throw new OutOfSync(`${path} is at version ${buffer.version}, not ${version}`);
    }
    if (!force && !buffer.missing) {
      const now = await mtimeOf(buffer.absolute);
      if (now !== undefined && now !== buffer.mtime) {
        buffer.conflict = true;
        return { saved: false, info: infoOf(buffer) };
      }
    }
    const body = (buffer.bom ? "\ufeff" : "") + withEol(buffer.text, buffer.eol);
    await mkdir(dirname(buffer.absolute), { recursive: true });
    await writeFile(buffer.absolute, body, "utf8");
    buffer.mtime = (await mtimeOf(buffer.absolute)) ?? Date.now();
    buffer.saved = buffer.text;
    buffer.savedEol = buffer.eol;
    buffer.missing = false;
    buffer.conflict = false;
    buffer.readonly = false;
    this.#cancelDraft(buffer);
    void this.#removeDraft(buffer.absolute);
    this.project.forget();
    return { saved: true, info: infoOf(buffer) };
  }

  /** Reads the file again, dropping the buffer's changes. */
  async revert(path: string): Promise<OpenedBuffer | Unopenable> {
    const buffer = this.#get(path);
    const read = await readFileText(buffer.absolute, buffer.path);
    if ("reason" in read) return read;
    Object.assign(buffer, {
      text: read.text,
      saved: read.text,
      version: buffer.version + 1,
      eol: read.eol,
      savedEol: read.eol,
      bom: read.bom,
      mtime: read.mtime,
      missing: read.missing,
      conflict: false,
      readonly: read.readonly,
    });
    delete buffer.draft;
    this.#cancelDraft(buffer);
    void this.#removeDraft(buffer.absolute);
    return { ...infoOf(buffer), text: buffer.text };
  }

  /** Keeps the buffer's changes over the file that changed on disk; the next save writes them. */
  async keep(path: string): Promise<BufferInfo> {
    const buffer = this.#get(path);
    buffer.mtime = (await mtimeOf(buffer.absolute)) ?? buffer.mtime;
    buffer.conflict = false;
    return infoOf(buffer);
  }

  /** Takes the draft an earlier textpen left, or drops it. */
  draft(path: string, take: boolean): OpenedBuffer {
    const buffer = this.#get(path);
    const draft = buffer.draft;
    delete buffer.draft;
    if (take && draft) {
      buffer.text = draft.text;
      buffer.version += 1;
    } else {
      void this.#removeDraft(buffer.absolute);
    }
    return { ...infoOf(buffer), text: buffer.text };
  }

  /** Changes the line ends the buffer is written with. */
  setEol(path: string, eol: Eol): BufferInfo {
    const buffer = this.#get(path);
    buffer.eol = eol;
    return infoOf(buffer);
  }

  close(path: string, discard: boolean): void {
    const buffer = this.#buffers.get(path);
    if (!buffer) return;
    if (isDirty(buffer) && !discard) {
      throw new Error(`${path} has changes that are not saved`);
    }
    this.#cancelDraft(buffer);
    if (discard) void this.#removeDraft(buffer.absolute);
    this.#buffers.delete(path);
  }

  /** After a rename on disk: the buffers of the file, or of the files under the directory, follow. */
  moved(from: string, to: string): void {
    for (const [path, buffer] of [...this.#buffers]) {
      const rest =
        path === from ? "" : path.startsWith(`${from}/`) ? path.slice(from.length) : null;
      if (rest === null) continue;
      this.#buffers.delete(path);
      buffer.absolute = this.project.absolute(to + rest);
      buffer.path = this.project.name(buffer.absolute);
      this.#buffers.set(buffer.path, buffer);
    }
  }

  /**
   * The buffers whose file changed on disk since it was read. One without changes of its
   * own is read again; one with changes is marked as in conflict, for the view to ask.
   */
  async changedOnDisk(): Promise<BufferInfo[]> {
    const changed: BufferInfo[] = [];
    await Promise.all(
      [...this.#buffers.values()].map(async (buffer) => {
        const now = await mtimeOf(buffer.absolute);
        if (now === undefined) {
          if (!buffer.missing && buffer.mtime !== 0) {
            buffer.missing = true;
            changed.push(infoOf(buffer));
          }
          return;
        }
        if (now === buffer.mtime && !buffer.missing) return;
        if (!isDirty(buffer)) {
          const read = await readFileText(buffer.absolute, buffer.path);
          if ("reason" in read) return;
          if (read.text !== buffer.text) buffer.version += 1;
          Object.assign(buffer, {
            text: read.text,
            saved: read.text,
            eol: read.eol,
            savedEol: read.eol,
            bom: read.bom,
            mtime: read.mtime,
            missing: false,
            conflict: false,
          });
        } else if (!buffer.conflict) {
          buffer.conflict = true;
          buffer.missing = false;
        } else {
          return;
        }
        changed.push(infoOf(buffer));
      }),
    );
    return changed;
  }

  /** Writes a draft of every buffer with changes, now: the program is about to end. */
  flushDrafts(): void {
    for (const buffer of this.#buffers.values()) {
      this.#cancelDraft(buffer);
      if (buffer.text === buffer.saved) continue;
      try {
        mkdirSync(this.#drafts, { recursive: true });
        writeFileSync(this.#draftFile(buffer.absolute), JSON.stringify(draftOf(buffer)));
      } catch {
        // Nowhere to keep it: the changes go with the program, as they would in any editor.
      }
    }
  }

  /** Drops every change that is not saved, and its draft: the person quit without saving. */
  discardAll(): void {
    for (const buffer of this.#buffers.values()) {
      this.#cancelDraft(buffer);
      buffer.saved = buffer.text;
      buffer.savedEol = buffer.eol;
      try {
        unlinkSync(this.#draftFile(buffer.absolute));
      } catch {
        // There was none.
      }
    }
  }

  #get(path: string): Open {
    const buffer = this.#buffers.get(path);
    if (!buffer) throw new OutOfSync(`${path} is not open`);
    return buffer;
  }

  #draftFile(absolute: string): string {
    return join(this.#drafts, `${keyOf(absolute)}.json`);
  }

  #scheduleDraft(buffer: Open): void {
    this.#cancelDraft(buffer);
    this.#timers.set(
      buffer.absolute,
      setTimeout(() => {
        this.#timers.delete(buffer.absolute);
        if (buffer.text === buffer.saved) {
          void this.#removeDraft(buffer.absolute);
          return;
        }
        void mkdir(this.#drafts, { recursive: true })
          .then(() => writeFile(this.#draftFile(buffer.absolute), JSON.stringify(draftOf(buffer))))
          .catch(() => {});
      }, 1500),
    );
  }

  #cancelDraft(buffer: Open): void {
    const timer = this.#timers.get(buffer.absolute);
    if (timer !== undefined) clearTimeout(timer);
    this.#timers.delete(buffer.absolute);
  }

  async #readDraft(absolute: string): Promise<Draft | undefined> {
    try {
      const draft = JSON.parse(await readFile(this.#draftFile(absolute), "utf8")) as Draft;
      return draft.path === absolute && typeof draft.text === "string" ? draft : undefined;
    } catch {
      return undefined;
    }
  }

  async #removeDraft(absolute: string): Promise<void> {
    try {
      await unlink(this.#draftFile(absolute));
    } catch {
      // There was none.
    }
  }
}

function isDirty(buffer: Open): boolean {
  return buffer.text !== buffer.saved || buffer.eol !== buffer.savedEol;
}

function infoOf(buffer: Open): BufferInfo {
  return {
    path: buffer.path,
    version: buffer.version,
    dirty: isDirty(buffer),
    eol: buffer.eol,
    bom: buffer.bom,
    mtime: buffer.mtime,
    missing: buffer.missing,
    conflict: buffer.conflict,
    readonly: buffer.readonly,
    ...(buffer.draft ? { draft: { at: buffer.draft.at, length: buffer.draft.text.length } } : {}),
  };
}

function draftOf(buffer: Open): Draft {
  return { path: buffer.absolute, text: buffer.text, at: Date.now() };
}

function withEol(text: string, eol: Eol): string {
  return eol === "crlf" ? text.replace(/\n/g, "\r\n") : text;
}

async function mtimeOf(path: string): Promise<number | undefined> {
  try {
    return (await stat(path)).mtimeMs;
  } catch {
    return undefined;
  }
}

interface Read {
  text: string;
  eol: Eol;
  bom: boolean;
  mtime: number;
  missing: boolean;
  readonly: boolean;
}

/** A file as text, with its line ends made `\n` and what they were remembered. */
async function readFileText(absolute: string, name: string): Promise<Read | Unopenable> {
  let info;
  try {
    info = await stat(absolute);
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === "ENOENT") {
      // A file that is not there yet: it opens empty and comes to be when it is saved.
      return { text: "", eol: "lf", bom: false, mtime: 0, missing: true, readonly: false };
    }
    return unopenable(name, "unreadable", 0, messageOf(error));
  }
  if (info.isDirectory()) return unopenable(name, "directory", 0, `${name} is a directory`);
  if (info.size > TEXT_LIMIT) {
    return unopenable(
      name,
      "too-large",
      info.size,
      `${name} is ${(info.size / 1024 / 1024).toFixed(1)} MB; textpen opens files up to ${TEXT_LIMIT / 1024 / 1024} MB`,
    );
  }
  let bytes: Uint8Array;
  try {
    bytes = await readFile(absolute);
  } catch (error) {
    return unopenable(name, "unreadable", info.size, messageOf(error));
  }
  if (isBinary(bytes)) {
    return unopenable(name, "binary", info.size, `${name} is not text`);
  }
  let raw: string;
  try {
    // The decoder takes a byte order mark off by itself; it is put back when the file is written.
    raw = new TextDecoder("utf-8", { fatal: true }).decode(bytes);
  } catch {
    return unopenable(name, "binary", info.size, `${name} is not UTF-8 text`);
  }
  const bom = bytes[0] === 0xef && bytes[1] === 0xbb && bytes[2] === 0xbf;
  const crlf = raw.match(/\r\n/g)?.length ?? 0;
  const lf = (raw.match(/\n/g)?.length ?? 0) - crlf;
  let readonly = false;
  try {
    await access(absolute, constants.W_OK);
  } catch {
    readonly = true;
  }
  return {
    text: raw.replace(/\r\n?/g, "\n"),
    eol: crlf > lf ? "crlf" : "lf",
    bom,
    mtime: info.mtimeMs,
    missing: false,
    readonly,
  };
}

function unopenable(
  path: string,
  reason: Unopenable["reason"],
  size: number,
  message: string,
): Unopenable {
  return { path, reason, size, message };
}

function messageOf(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

/** What one message to the view may carry of a text: the terminal's 16 MiB, with room to spare. */
const WIRE_BUDGET = 12 * 1024 * 1024;

/**
 * How many bytes a text takes in a message to the view: JSON's escapes, and pencere's
 * `\uXXXX` for every code unit outside ASCII, which a frame may not carry raw.
 */
function wireSize(text: string): number {
  let size = 0;
  for (let i = 0; i < text.length; i += 1) {
    const c = text.charCodeAt(i);
    if (c >= 0x7f) size += 6;
    else if (c === 0x22 || c === 0x5c || c === 0x0a || c === 0x09 || c === 0x0d) size += 2;
    else if (c < 0x20) size += 6;
    else size += 1;
  }
  return size;
}

/** A buffer as it can go to the view in one message: whole, or announced, to be read in pieces. */
export function sendable<T extends OpenedBuffer | Unopenable>(opened: T): T {
  if (!("text" in opened) || opened.text.length * 6 <= WIRE_BUDGET) return opened;
  if (wireSize(opened.text) <= WIRE_BUDGET) return opened;
  return { ...opened, text: "", partial: { length: opened.text.length } };
}

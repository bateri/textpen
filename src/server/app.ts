/**
 * The state of one textpen program: its project, its open files, what it remembers,
 * and who can take the pane from the view. The view's own state outlives the view: a
 * view that comes back from a shell, or that the terminal starts again, finds it here.
 */

import { readFileSync } from "node:fs";
import { mkdir, rename, rm, stat, writeFile } from "node:fs/promises";
import { homedir, hostname, userInfo } from "node:os";
import { dirname } from "node:path";

import { Buffers } from "./buffers.js";
import { Project, displayName } from "./project.js";
import { Store } from "./store.js";
import type { Boot, Changes, Entry, GitInfo, Returned, Settings, UiState } from "./types.js";

/** The package's version, from its manifest: a release changes it there and nowhere else. */
export const VERSION: string = JSON.parse(
  readFileSync(new URL("../../package.json", import.meta.url), "utf8"),
).version;

/**
 * Who can take the pane from the view: the program's main loop, which closes the view
 * and runs a shell in the terminal. Absent where no terminal shows the view (the harness).
 */
export interface Desk {
  shell(): "scheduled" | "busy";
}

export interface Focus {
  path: string;
  line?: number;
  column?: number;
}

export class Textpen {
  readonly project: Project;
  readonly buffers: Buffers;
  readonly store = new Store();
  settings: Settings;
  ui: UiState;
  desk: Desk | undefined;
  /** What the command line asked to open; told to the first view only. */
  focus: Focus | undefined;
  problems: string[] = [];
  closeQuits = false;
  /** How the shell the pane was handed to ended; told to the next view that boots, once. */
  returned: Returned | undefined;
  /** A hand-off the terminal could not make; told to the view with the next changes, once. */
  handoffFailed: string | undefined;

  constructor(root: string) {
    this.project = new Project(root);
    this.buffers = new Buffers(this.project, this.store.state);
    this.settings = this.store.settings();
    this.ui = this.store.ui(this.project.root);
  }

  /** Starts on the files the command line names, in place of the tabs of the last run. */
  startWith(paths: string[], focus: Focus | undefined): void {
    const tabs = paths.map((p) => this.project.name(this.project.absolute(p)));
    this.closeQuits = true;
    this.ui = {
      ...this.ui,
      tabs,
      active: focus ? this.project.name(this.project.absolute(focus.path)) : (tabs[0] ?? null),
    };
    if (focus)
      this.focus = { ...focus, path: this.project.name(this.project.absolute(focus.path)) };
  }

  async boot(): Promise<Boot> {
    const returned = this.returned;
    const focus = this.focus;
    this.returned = undefined;
    this.focus = undefined;
    let user = "";
    try {
      user = userInfo().username;
    } catch {
      // A user without an entry in the password file still edits files.
    }
    return {
      root: this.project.root,
      rootName: displayName(this.project.root),
      home: homedir(),
      user,
      hostname: hostname(),
      remote: Boolean(process.env.SSH_CONNECTION || process.env.SSH_TTY),
      version: VERSION,
      git: await this.project.git(0),
      settings: this.settings,
      ui: this.ui,
      buffers: this.buffers.all(),
      handoff: this.desk !== undefined,
      closeQuits: this.closeQuits && this.desk !== undefined,
      ...(returned ? { returned } : {}),
      ...(focus ? { focus } : {}),
      problems: this.problems,
    };
  }

  /** The view's own state, kept for the next view and the next run. */
  keep(ui: UiState): void {
    this.ui = ui;
    this.store.saveUi(this.project.root, ui);
  }

  /**
   * What happened to the files since the view was last told: git, the open files that
   * changed on disk, the listed directories that did. Asking marks them as told, so one
   * subscription asks.
   */
  async changes(): Promise<Omit<Changes, "git"> & { git: GitInfo | null }> {
    const handoffFailed = this.handoffFailed;
    this.handoffFailed = undefined;
    const [git, buffers, dirs] = await Promise.all([
      this.project.git(),
      this.buffers.changedOnDisk(),
      this.project.changedDirs(),
    ]);
    return { git, buffers, dirs, ...(handoffFailed ? { handoffFailed } : {}) };
  }

  saveSettings(settings: Settings): Settings {
    this.settings = this.store.saveSettings(settings);
    return this.settings;
  }

  async create(path: string, kind: Entry["kind"]): Promise<string> {
    const absolute = this.project.absolute(path);
    if (await exists(absolute)) throw new Error(`${path} already exists`);
    if (kind === "dir") {
      await mkdir(absolute, { recursive: true });
    } else {
      await mkdir(dirname(absolute), { recursive: true });
      await writeFile(absolute, "", { flag: "wx" });
    }
    this.project.forget();
    return this.project.name(absolute);
  }

  async rename(from: string, to: string): Promise<string> {
    const source = this.project.absolute(from);
    const target = this.project.absolute(to);
    if (source === target) return this.project.name(target);
    // A change of case alone is the same file on a disk that ignores case, and allowed.
    if (source.toLowerCase() !== target.toLowerCase() && (await exists(target))) {
      throw new Error(`${to} already exists`);
    }
    await mkdir(dirname(target), { recursive: true });
    await rename(source, target);
    this.buffers.moved(this.project.name(source), this.project.name(target));
    this.project.forget();
    return this.project.name(target);
  }

  async remove(path: string): Promise<void> {
    const absolute = this.project.absolute(path);
    if (absolute === this.project.root) throw new Error("The project itself is not removed");
    await rm(absolute, { recursive: true });
    this.project.forget();
  }

  /** The program ends: what is not saved is kept as drafts, and the view's state for next time. */
  ending(): void {
    this.buffers.flushDrafts();
    this.store.flush();
  }
}

async function exists(path: string): Promise<boolean> {
  try {
    await stat(path);
    return true;
  } catch {
    return false;
  }
}

/**
 * What the program and its view say to each other. The view imports these types; nothing
 * here runs.
 */

export type GitState = "modified" | "added" | "untracked" | "deleted" | "renamed" | "conflict";

export interface Entry {
  name: string;
  /** Relative to the root, `/`-separated; absolute for what lies outside it. */
  path: string;
  kind: "file" | "dir";
  /** A symbolic link, followed: `kind` is what it points to. */
  link?: boolean;
  size: number;
}

export interface Listing {
  dir: string;
  entries: Entry[];
  /** The directory has more entries than are listed. */
  truncated: boolean;
}

export interface GitInfo {
  branch: string;
  /** Commits ahead of and behind the upstream, when there is one. */
  ahead: number;
  behind: number;
  /** Changed paths, relative to the root, with what changed. */
  changes: Record<string, GitState>;
}

export type Eol = "lf" | "crlf";

/** What the view knows of a buffer without its text. */
export interface BufferInfo {
  /** The buffer's file: relative to the root, or absolute outside it. Also its id. */
  path: string;
  /** Its version: one more with every edit the program applies. */
  version: number;
  /** The text differs from the file's. */
  dirty: boolean;
  eol: Eol;
  bom: boolean;
  /** When the file on disk was last read or written, in ms; 0 for a file not on disk yet. */
  mtime: number;
  /** The file is not on disk: a new file, or one removed while it was open. */
  missing: boolean;
  /** The file changed on disk after it was read, and the buffer has changes of its own. */
  conflict: boolean;
  /** A draft from an earlier textpen that ended with this file unsaved. */
  draft?: { at: number; length: number };
  readonly: boolean;
}

export interface OpenedBuffer extends BufferInfo {
  text: string;
  /**
   * The text is too long for one message to the view (a terminal carries 16 MiB at most,
   * and every character outside ASCII travels as six bytes): `text` is empty, and the
   * view reads it with `textpen_slice`, in pieces of this version.
   */
  partial?: { length: number };
}

/** Why a file is not opened as text. */
export interface Unopenable {
  path: string;
  reason: "binary" | "too-large" | "directory" | "unreadable";
  size: number;
  message: string;
}

export type LineNumbers = "on" | "relative" | "off";

export interface Settings {
  vim: boolean;
  wrap: boolean;
  fontSize: number;
  lineNumbers: LineNumbers;
  tabSize: number;
  insertSpaces: boolean;
  whitespace: boolean;
  /** Save a file when the editor loses it: another tab, the shell, quitting. */
  autoSave: boolean;
  /** The file tree beside the editor, in a pane wide enough for it. */
  sidebar: boolean;
  sidebarWidth: number;
}

export const DEFAULT_SETTINGS: Settings = {
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
};

/** Where the cursor and the scroll were in a file. */
export interface Place {
  anchor: number;
  head: number;
  top: number;
}

/** The view's own state, which the program keeps for the next view and the next run. */
export interface UiState {
  tabs: string[];
  active: string | null;
  expanded: string[];
  places: Record<string, Place>;
  /** Files opened lately, the latest first: the quick-open's order with nothing typed. */
  recent: string[];
  preview: string[];
}

export const EMPTY_UI: UiState = {
  tabs: [],
  active: null,
  expanded: [],
  places: {},
  recent: [],
  preview: [],
};

export interface Returned {
  /** The shell the pane was handed to, how it ended. */
  exitCode: number | null;
  duration: number;
  error?: string;
}

export interface Boot {
  root: string;
  rootName: string;
  home: string;
  user: string;
  hostname: string;
  /** Whether the program runs on this machine, as far as it can tell: no ssh in its environment. */
  remote: boolean;
  version: string;
  git: GitInfo | null;
  settings: Settings;
  ui: UiState;
  buffers: BufferInfo[];
  /** Whether the pane can be handed to a shell: the program has a terminal of its own. */
  handoff: boolean;
  /** Whether the program can end itself from the view (it was started in a terminal). */
  quit: boolean;
  /**
   * The command line named the files to edit (as `$EDITOR` is run): closing the last of
   * the tabs ends textpen, so that what waits for it goes on.
   */
  closeQuits: boolean;
  returned?: Returned;
  /** What the command line asked to open, and at which line. */
  focus?: { path: string; line?: number; column?: number };
  /** Paths given on the command line that could not be opened. */
  problems: string[];
}

export interface PollResult {
  at: number;
  git: GitInfo | null;
  /** Buffers whose file changed on disk, or went, since the view last heard of them. */
  buffers: BufferInfo[];
  /** Directories that changed since they were listed. */
  dirs: string[];
}

export interface SearchMatch {
  line: number;
  /** The match within `text`, in UTF-16 units. */
  start: number;
  end: number;
  /** The line, cut around the match when it is long. */
  text: string;
  /** Where `text` begins in the line. */
  offset: number;
}

export interface SearchFile {
  path: string;
  matches: SearchMatch[];
}

export interface SearchResult {
  files: SearchFile[];
  matches: number;
  searched: number;
  truncated: boolean;
  ms: number;
}

export interface FileIndex {
  files: string[];
  truncated: boolean;
}

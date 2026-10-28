# What the program and the page say to each other

textpen is two halves in one file: the program (`src/program/`, Rust, on the `pencere` crate) and
the page it shows (`src/view/`, Lua, on pencere's kit). They talk only through the calls and the
topic written here, in pencere's six messages. Both halves are written to this file; the program's
tests hold it to it, and the page's tests hold the page to it.

## Rules for every call

- **Places are code points.** Every offset, length and range in a text — an edit, a slice, a
  cursor, a search match, a draft's length — counts Unicode scalar values (what the kit's editor
  counts), never bytes and never UTF-16 units. A place inside a character cannot happen.
- **Lines and columns** that come from the command line are 1-based; `Place.top` is a 0-based line.
- **`null` and absent are different.** A field written `T | null` is always there and may be
  `null`; a field written `?` may be left out and is never `null`. What the page sends is held to
  this: a required field that is missing, an extra `null`, a wrong type is answered `bad-params`
  and the call does nothing. Extra fields the program does not know are ignored.
- **The page is not trusted.** Every call's params are checked before it runs (`bad-params`, with
  words that say where: `tabs.0: expected a string`).
- **Errors** are `{ code, message }`. The codes the page acts on:
  - `bad-params` — the call was not run.
  - `out-of-sync` — a buffer call named a version the program no longer holds, or an edit reaches
    outside the text. The page reads the buffer again (`buffer` with `text`).
  - `failed` — the disk or the project would not do it; `message` is for the person.
  - `internal` — the program failed; `message` is for the person.
- **One message is at most 1 MiB** each way (pencere counts the UTF-8 of the JSON before its
  escapes). Nothing the program answers grows with the project: a long text, a big directory, the
  file index and a long search come in pieces or are cut and say so. The program fills a message
  to a quarter of that at most (256 KiB), which the page reads within the time it has for one call
  (250 ms). The page keeps each call it sends to the same quarter, as writing it out takes as long:
  an `edit` carries at most 64 Ki code points of inserted text (256 KiB at most).
- **Buffer calls are answered in the order they were sent.** `open`, `edit`, `slice`, `save`,
  `buffer`, `discard` and `quit` run one at a time, in the page's order (the program's lane for buffers),
  so the page may send an `edit` and then a `save` of the version it will make without waiting.
  Other calls run at the same time as each other and as the buffer calls.

## Shapes

```
GitState    = "modified" | "added" | "untracked" | "deleted" | "renamed" | "conflict"
Entry       = { name: string, path: string, kind: "file" | "dir", link?: true, size: number }
              -- path: relative to the root, "/"-separated; absolute outside it.
              -- link: a symbolic link, followed (kind is what it points to).
Listing     = { dir: string, entries: Entry[], truncated: boolean }
GitInfo     = { branch: string, ahead: number, behind: number, changes: { [path]: GitState } }
Eol         = "lf" | "crlf"
BufferInfo  = {
  path: string,          -- relative to the root, or absolute outside it; also the buffer's id
  version: number,       -- one more with every change the program applies
  dirty: boolean,        -- the text differs from the file's
  eol: Eol, bom: boolean,
  mtime: number,         -- ms; 0 for a file not on disk yet
  missing: boolean,      -- not on disk: new, or removed while open
  conflict: boolean,     -- changed on disk after it was read, and the buffer has changes of its own
  draft?: { at: number, length: number },   -- a draft an earlier textpen left; length in code points
  readonly: boolean,
}
Opened      = BufferInfo & { text: string, partial?: { length: number } }
              -- partial: the text is too long for one message; text is "", and the page reads it
              -- with `slice` (length in code points).
Unopenable  = { path: string, reason: "binary" | "too-large" | "directory" | "unreadable",
                size: number, message: string }
LineNumbers = "on" | "relative" | "off"
Whitespace  = "none" | "boundary" | "selection" | "trailing" | "all"     -- the kit's names
Settings    = {
  wrap: boolean, lineNumbers: LineNumbers, tabSize: number, insertSpaces: boolean,
  whitespace: Whitespace, indentGuides: boolean,
  autoSave: boolean,     -- save a file when the editor loses it: another tab, the shell, quitting
  sidebar: boolean, sidebarWidth: number,
}
              -- defaults: wrap false, lineNumbers "on", tabSize 2, insertSpaces true,
              -- whitespace "selection", indentGuides true, autoSave false, sidebar true,
              -- sidebarWidth 240.
              -- Gone from the web editor's: vim (no vim mode), fontSize (the terminal's text size
              -- is the page's; textpen has no zoom of its own). A settings file from an older
              -- textpen is read: unknown keys are dropped, `whitespace: true/false` is "all"/"none".
Place       = { anchor: number, head: number, top: number }   -- code points; top a 0-based line
UiState     = {
  tabs: string[], active: string | null, expanded: string[],
  places: { [path]: Place },
  recent: string[],      -- opened lately, the latest first: the quick open's order with nothing typed
  preview: string[],     -- the files shown as their Markdown preview
}
Returned    = { exitCode: number | null, duration: number, error?: string }
              -- the shell the pane was handed to: its status (null: ended by a signal), ms it ran
Boot        = {
  root: string, rootName: string, home: string, user: string, hostname: string,
  remote: boolean,       -- ssh in the program's environment
  version: string,
  git: GitInfo | null,
  settings: Settings, ui: UiState, buffers: BufferInfo[],
  handoff: boolean,      -- the pane can be handed to a shell
  closeQuits: boolean,   -- the command line named the files (as $EDITOR is run): closing the last
                         -- tab ends textpen
  returned?: Returned,
  focus?: { path: string, line?: number, column?: number },   -- 1-based; column in code points
  problems: string[],    -- paths on the command line that could not be opened
}
Changes     = {
  git?: GitInfo | null,  -- left out when it is what it was last told
  buffers: BufferInfo[], -- open files that changed on disk or went
  dirs: string[],        -- listed directories that changed
  handoffFailed?: string,
}
SearchMatch = { line: number, start: number, end: number, text: string, offset: number }
              -- line 1-based; start/end the match within text, code points; text the line, cut
              -- around the match when long; offset where text begins in the line, code points
SearchFile  = { path: string, matches: SearchMatch[] }
SearchResult= { files: SearchFile[], matches: number, searched: number, truncated: boolean, ms: number }
FilesPage   = { files: string[], truncated: boolean, next: number | null, generation: number }
```

## Calls

| call       | params                                                                                   | result                                                                                                                                                                                                                         |
| ---------- | ---------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `boot`     | `{}`                                                                                     | `Boot`. `returned` and `focus` are told once.                                                                                                                                                                                  |
| `ui`       | `{ ui: UiState }`                                                                        | `{}`. Kept for the next view and the next run.                                                                                                                                                                                 |
| `settings` | `{ settings: Settings }`                                                                 | `{ settings: Settings }` as kept.                                                                                                                                                                                              |
| `handoff`  | `{ kind: "shell" }`                                                                      | `{ state: "scheduled" \| "busy" \| "unavailable" }`. When scheduled, the answer reaches the page before the view closes; the page is shown again where it was when the shell ends, and `boot` tells `returned`.                |
| `list`     | `{ dir: string }`                                                                        | `Listing`, cut to fit one message.                                                                                                                                                                                             |
| `files`    | `{ from?: integer ≥ 0, generation?: integer }`                                           | `FilesPage`: the file index for the quick open, a message's worth from `from`. The pages of one listing are of the index its first page (no `from`) was taken from: each after it names that page's `generation`, and is `out-of-sync` where another listing was begun since (the page reads the files again from the start).|
| `create`   | `{ path: string, kind: "file" \| "dir" }`                                                | `{ path }`. Exclusive: an existing path is `failed`.                                                                                                                                                                           |
| `rename`   | `{ from: string, to: string }`                                                           | `{ path, buffers: BufferInfo[] }`. Open buffers under `from` follow. A rename by case alone is allowed for any letter. `failed` for the project or a directory it is in, however reached (a link on the way), for a directory into itself, and onto the name of an open file with changes not saved.                                                                                                         |
| `delete`   | `{ path: string }`                                                                       | `{ path }`. `failed` for the project or a directory it is in, however reached.                                                                                                                                                  |
| `open`     | `{ path: string }`                                                                       | `Opened \| Unopenable`. A file not on disk opens empty, `missing`; a pipe, a socket or a device is `Unopenable` (`unreadable`).                                                                                                                                                             |
| `edit`     | `{ path, base: integer, edits: [from, to, insert][] }`                                   | `BufferInfo`. Edits made on version `base`, applied in order, each on the text the one before left.                                                                                                                            |
| `slice`    | `{ path, version: integer, from: integer ≥ 0 }`                                          | `{ text, end }`: the piece of a long text from `from`; the next begins at `end`.                                                                                                                                               |
| `save`     | `{ path, version: integer, force?: boolean }`                                            | `{ saved: boolean, info: BufferInfo }`. A file changed on disk (or made since the buffer opened `missing`) is not overwritten unless `force`: `saved` false and `info.conflict`.                                                                                           |
| `buffer`   | `{ path, action, discard?: boolean }`                                                    | by `action`: `text` → `Opened`; `revert` → `Opened \| Unopenable`; `keep` → `BufferInfo`; `restore` / `drop` (the draft) → `Opened`; `lf` / `crlf` → `BufferInfo`; `close` → `{ path }` (`failed` if dirty and not `discard`). |
| `discard`  | `{}`                                                                                     | `{}`. Quitting without saving: nothing unsaved is kept as a draft.                                                                                                                                                             |
| `quit`     | `{}`                                                                                     | `{}`, or no answer: the program closes its view once the buffer calls sent before it are done, and textpen ends, what is not saved kept as drafts (none after `discard`). `failed` where the terminal cannot close the view.     |
| `search`   | `{ query: string (≥ 1), regex?: boolean, caseSensitive?: boolean, wholeWord?: boolean }` | `SearchResult`, cut to fit one message. The pattern is the Rust `regex` crate's: back-references and lookaround are refused (`failed`, with words), so a search ends in time linear in the text.                               |

A page of the kit has no message of the host's own to close its view with (the protocol, Section
10.6: a kit view that wants the program to end sends it a `call`, and the program ends or dismisses
the view), so textpen ends by `quit`: the page saves or discards first, keeps its state with `ui`,
then asks.

## Topic

`changes`, no params: one `Changes` at once, then one only when something differs from what it
told. A buffer whose file changed and that has no changes of its own is read again by the program
(its `version` goes up: the page reads its text with `buffer` `text`); one with changes is marked
`conflict` for the page to ask.

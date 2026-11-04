# What the program and the page say to each other

textpen is two halves in one file: the program (`src/program/`, Rust, on the `pencere` crate) and
the page it shows (`src/view/`, Lua, on pencere's kit). They talk only through the calls and the
topic written here, in pencere's six messages. Both halves are written to this file; the program's
tests hold it to it, and the page's tests hold the page to it.

## Rules for every call

- **Places in a text are bytes.** Every offset, length and range in a buffer's text — an edit, a
  slice, a cursor (`Place`), a draft's length — counts bytes of its UTF-8 from 0, as the kit's editor
  says them to a page (from 1: the page takes one off). The page cannot say how many characters the
  text an edit takes out had, and the program, which holds that text, needs no count of them. A place
  inside a character is refused (`out-of-sync` for an edit), never rounded. What a person reads in a
  line — the column a command line names, a search match in the line shown — counts code points of
  that line (below), and the page turns it into bytes where it puts the cursor there.
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
  - `exists` — an `upload` found a file of that name that the page did not say to replace; nothing
    was written, and `message` names the file.
  - `internal` — the program failed; `message` is for the person.
- **One message is at most 1 MiB** each way (pencere counts the UTF-8 of the JSON before its
  escapes). Nothing the program answers grows with the project: a long text, a big directory, the
  file index and a long search come in pieces or are cut and say so. The program fills a message
  to a quarter of that at most (256 KiB), which the page reads within the time it has for one call
  (250 ms). The page keeps each call it sends to the same quarter, as writing it out takes as long:
  an `edit` carries at most 64 KiB of inserted text (the page cuts a longer one).
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
GitInfo     = { branch: string, ahead: number, behind: number, changes: { [path]: GitState },
              index: number, head: string, renames?: { [path]: string }, ignored?: string[] }
              -- index: when git's index file was last written (ms; 0 if not said): staging or unstaging
              -- moves it, so the page asks `diff` again for the files it marks.
              -- head: the commit the project is at (git's `# branch.oid`; "" before the first commit): a
              -- commit moves it, and the page asks `compare` again for the files it shows against it.
              -- renames: for a file git has renamed (or copied), the path it had, both under the root.
              -- ignored: what git ignores, under the root: a folder that matches stands for all in it,
              -- which is not named. 64 KiB of paths at most; those past that are not told.
Eol         = "lf" | "crlf"
BufferInfo  = {
  path: string,          -- relative to the root, or absolute outside it; also the buffer's id
  version: number,       -- one more with every change the program applies
  dirty: boolean,        -- the text differs from the file's
  eol: Eol, bom: boolean,
  mtime: number,         -- ms; 0 for a file not on disk yet
  missing: boolean,      -- not on disk: new, or removed while open
  conflict: boolean,     -- changed on disk after it was read, and the buffer has changes of its own
  draft?: { at: number, length: number },   -- a draft an earlier textpen left: at in ms (Unix time), length in bytes
  readonly: boolean,
}
Opened      = BufferInfo & { text: string, partial?: { length: number } }
              -- partial: the text is too long for one message; text is "", and the page reads it
              -- with `slice` (length in bytes).
Unopenable  = { path: string, reason: "binary" | "too-large" | "directory" | "unreadable",
                size: number, message: string }
LineNumbers = "on" | "relative" | "off"
Whitespace  = "none" | "boundary" | "selection" | "trailing" | "all"     -- the kit's names
Settings    = {
  wrap: boolean, lineNumbers: LineNumbers, tabSize: number, insertSpaces: boolean,
  whitespace: Whitespace, indentGuides: boolean,
  autoSave: boolean,     -- save a file when the editor loses it: another tab, the shell, quitting
  sidebar: boolean, sidebarWidth: number,
  diffDecorations: boolean,   -- mark the lines that differ from git's index (VS Code's scm.diffDecorations)
}
              -- defaults: wrap false, lineNumbers "on", tabSize 2, insertSpaces true,
              -- whitespace "selection", indentGuides true, autoSave false, sidebar true,
              -- sidebarWidth 240, diffDecorations true.
              -- Gone from the web editor's: vim (no vim mode), fontSize (the terminal's text size
              -- is the page's; textpen has no zoom of its own). A settings file from an older
              -- textpen is read: unknown keys are dropped, `whitespace: true/false` is "all"/"none".
Place       = { anchor: number, head: number, top: number }   -- bytes from 0; top a 0-based line
UiState     = {
  tabs: string[], active: string | null, expanded: string[],
  places: { [path]: Place },
  recent: string[],      -- opened lately, the latest first: the quick open's order with nothing typed
  preview: string[],     -- the files shown as their Markdown preview
  changesTree: boolean,  -- the panel of changes stands its files under their folders
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
  focus?: { path: string, line?: number, column?: number },   -- 1-based; column in code points of the line
  problems: string[],    -- paths on the command line that could not be opened
}
Changes     = {
  git?: GitInfo | null,  -- left out when it is what it was last told
  buffers: BufferInfo[], -- open files that changed on disk or went
  dirs: string[],        -- listed directories that changed
  handoffFailed?: string,
}
Hunk        = { line: number, count: number, old: string[], cut?: true }
              -- one place the buffer differs from what git's index keeps of the file. line: the first
              -- line of it in the buffer (0-based); count: how many lines of the buffer (0: lines were
              -- only taken away, before `line`); old: the lines the index has there, with no line end;
              -- cut: some of old is not told.
Diffed      = { version: number, hunks: Hunk[] | null, more?: true }
              -- of the text at `version`. hunks null: nothing is marked (see `diff`).
Commit      = { id: string, subject: string }
              -- the last commit (HEAD): its full id and the first line of its message.
Block       = { oldLine: number, oldCount: number, newLine: number, newCount: number,
                oldWords?: [number, number][], newWords?: [number, number][] }
              -- one place the file differs from the last commit, lines from 0. oldLine and oldCount are
              -- the lines the commit has there, newLine and newCount the lines of the buffer. The side
              -- that has no lines (count 0) is where they would stand: oldCount 0 is lines only added,
              -- newCount 0 lines only taken away; with both the lines were changed, and where the counts
              -- differ the side with fewer lines is shorter by that many. A run of changed lines is
              -- cut into blocks, so that a line that was changed is paired with the line it became
              -- (the likest, in order; one line that became one is always a pair) and lines with no
              -- likeness to one on the other side are blocks of their own. A pair is a block of one line
              -- either side; its oldWords and newWords are the words that differ, `[from, to)` in bytes
              -- of that line (from 0, on characters' edges): a word is a run of letters and digits, a run
              -- of white space or any other character alone. Left out where the line is long (2 000
              -- bytes) or the message is full.
Compared    = { version: number, commit: Commit | null,
                why?: "binary" | "too-large" | "too-many" | "outside",
                base?: { id: string | null, length: number, path: string },
                blocks?: Block[] }
              -- of the text at `version`. With `why`, there is no comparison (see `compare`); else `base`
              -- and `blocks` are there.
SearchMatch = { line: number, start: number, end: number, text: string, offset: number }
              -- line 1-based; start/end the match within text, code points of text; text the line, cut
              -- around the match when long; offset where text begins in the line, code points
SearchFile  = { path: string, matches: SearchMatch[] }
SearchResult= { files: SearchFile[], matches: number, searched: number, truncated: boolean, ms: number }
Picture     = { name: string, size: number, width: number, height: number }
              -- name: what the page draws it by (`sha256:` and the digest of the file), the terminal
              -- holds the file; size: the file's bytes; width, height: its pixels as it is shown
              -- (a photograph held on its side is as wide as it is tall).
Refused     = { refused: "too-large", size: number } | { refused: "not-a-picture" }
              -- too-large: a picture past 8 MiB or past 4096 pixels either way; size is the file's.
              -- not-a-picture: not there, not a file, not named .png .jpg .jpeg .svg, not a picture of
              -- the three formats (by its bytes) or one that says no size, outside the project (`from`).
Place       = { name: string, exists: boolean, free: string }
              -- name: the name as the program takes it (only its last part, nothing a path could be
              -- made of). exists: taken in the folder (a link to nothing too) or by one of `also`.
              -- free: the first name beside it that is not taken, counting up from 2 ("foto 2.png", then
              -- "foto 3.png" where that is taken too), or with `dash` from 1 ("foto-1.png", then
              -- "foto-2.png"); `name` itself where it is not taken.
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
| `edit`     | `{ path, base: integer, edits: [from, to, insert][] }`                                   | `BufferInfo`. Edits made on version `base`, applied in order, each on the text the one before left; `from` and `to` are bytes, where a character starts.                                                                                                                            |
| `slice`    | `{ path, version: integer, from: integer ≥ 0 }`                                          | `{ text, end }`: the piece of a long text from the byte `from`; the next begins at the byte `end`.                                                                                                                                               |
| `save`     | `{ path, version: integer, force?: boolean }`                                            | `{ saved: boolean, info: BufferInfo }`. A file changed on disk (or made since the buffer opened `missing`) is not overwritten unless `force`: `saved` false and `info.conflict`.                                                                                           |
| `buffer`   | `{ path, action, discard?: boolean }`                                                    | by `action`: `text` → `Opened`; `revert` → `Opened \| Unopenable`; `keep` → `BufferInfo`; `restore` / `drop` (the draft) → `Opened`; `lf` / `crlf` → `BufferInfo`; `close` → `{ path }` (`failed` if dirty and not `discard`). |
| `discard`  | `{}`                                                                                     | `{}`. Quitting without saving: nothing unsaved is kept as a draft.                                                                                                                                                             |
| `quit`     | `{}`                                                                                     | `{}`, or no answer: the program closes its view once the buffer calls sent before it are done, and textpen ends, what is not saved kept as drafts (none after `discard`). `failed` where the terminal cannot close the view.     |
| `diff`     | `{ path: string }`                                                                       | `Diffed`: where the open buffer `path` differs from `git show :path`, the file as git's **index** keeps it (a change staged is not a difference, as VS Code's). Both texts are the program's buffer text: line ends `\n`, no byte order mark, so a file with CRLF line ends or a mark is not one hunk. `hunks` is `null`, and nothing is marked, for a file outside the project, a symbolic link, one git does not keep (untracked), a project that is not in a repository, a file that is not UTF-8 text, one that is not on disk yet, or a text of more than 2 MiB or 50 000 lines on either side; `out-of-sync` where `path` is not open. What is told fits one message: at most 2 000 hunks (`more`), and of each at most 64 lines (8 Ki code points) of `old`, 192 Ki code points together (`cut`: the hunk is marked, its old lines are not all there). It runs beside the buffer calls and not in their lane: the page asks after its edit was answered, and takes the answer only when `version` is the buffer's. |
| `compare`  | `{ path: string }`                                                                       | `Compared`: the open buffer `path` against the file as the last commit (HEAD) keeps it, staged and unstaged changes alike. `base.id` is the commit's blob (null where HEAD has no such file: new, untracked, or no commit yet, and the old text is empty), `base.length` the bytes of its text and `base.path` the path it has in HEAD (the one it had before a rename git knows of; else `path`). Both texts are the program's buffer text: line ends `\n`, no byte order mark. A file that is not on disk (`missing`) is compared as it is, empty. `why`: `binary` (the commit's file is not UTF-8 text), `too-large` (a text of more than 2 MiB or 50 000 lines on either side), `too-many` (more than 2 000 blocks, or blocks that do not fit one message: no part of the comparison is told), `outside` (a file outside the project, a symbolic link, or a project that is not in a repository). `commit` is null before the first commit (and for `outside`). Words are told as long as the blocks and the words fit 256 KiB together; past that the blocks go on without them. `out-of-sync` where `path` is not open. It runs beside the buffer calls and not in their lane: the page asks after its edit was answered, and takes the answer only when `version` is the buffer's. Where git cannot read the commit the call fails, in its words: that is not a file the commit has not. |
| `compare-base` | `{ path: string, id: string, from: integer ≥ 0 }`                                    | `{ text, end }`, as `slice`: the piece of the commit's text of `path` (the old text of `compare`, whose `base.id` is `id`) from the byte `from`, at most 256 KiB of whole characters; the next begins at the byte `end`. `out-of-sync` where the commit's file is no longer the blob `id` (a commit was made since: the page asks `compare` again). |
| `picture`  | `{ path: string, owner: string, from?: string }`                                         | `Picture \| Refused`. The picture at `path` (the project's, or absolute) is read, measured and given to the terminal to hold, for `owner` (a tab: a path), and the page draws it by `name`; the bytes never reach the page. With `from`, a Markdown file's path, `path` is as that file wrote it: relative to its folder, `%20` read, `?` and `#` cut, and only a file in the project, links followed, is read (an address, a path from the top of the disk, or one that climbs out of the project is `not-a-picture`). `failed` where the terminal holds no files for the view (the page is not at `lua` 0.1): the page then opens the file as any other. It runs beside the buffer calls and not in their lane. |
| `release`  | `{ owner: string }`                                                                      | `{}`. `owner` holds no picture any more: the files nobody else holds are let go. Asked of an owner that holds none, it does nothing. |
| `place`    | `{ dir: string, name: string, also?: string[], dash?: boolean }`                          | `Place`. Whether `name` is taken in the folder `dir` (a folder of the project; `failed` for any other), or by one of `also` (the names of uploads under way), and the next free name. |
| `upload`   | `{ dir: string, name: string, handle: string, size: integer ≥ 0, replace?: boolean }` | `{ path, size }`. The file the person gave the page (its `handle`, which the terminal made of it) is received into the folder `dir` of the project, as `name` (made safe: its last part only). The same wherever textpen runs: the terminal moves the bytes through a pipe in its folder on the program's machine where it keeps one (pencere §8.11), else in its messages, a MiB at a time. Told how far it has got with a `progress` as the bytes come (`done` bytes of `total`, the `size` the page was handed). A part beside the file takes the bytes and is given the file's name when the last is in; `exists` where the name is taken and `replace` is not true, whether it was when the call began or came to be while the bytes were on their way (nothing is written over it); `failed` where `dir` is not a folder of the project, or the name is a folder's. The page cancels the call to give the file up: the part is deleted, as it is when the view ends or the program is interrupted. Runs beside the other calls and not in the lane of the buffers. |
| `download` | `{ path: string, handle: string }`                                                        | `{ size }`. The file `path` of the project, as it is on its disk, is written to the place the person chose in the Save panel (`handle`, which the terminal made of it), through the terminal's folder where it keeps one, else a MiB at a time, telling `progress` as it goes. `failed` where `path` is not a file. |
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

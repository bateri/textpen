# textpen's program

The program of textpen, in Lua, for the runner of the `pencere` command. `make build` packs
`src/program/` (this directory), `src/assets/` (the icons a terminal draws beside the program's
name; `node tools/icon.mjs` makes them) and the view, `src/view` as vite built it into `dist/view`,
into the runner as one file, `dist/textpen`. The calls it answers are the ones the page makes, with
the params and results of `src/view/lib/types.ts`. Nothing generated is under `src/`.

The rule of the directories is one sentence: **`api/` knows the page; `project/`, `state/` and
`terminal/` do not.** They take and return plain Lua values and never build a message: a call, a
subscription, a push, a `code`. What the page is told is made in `api/` from what they hold.

| file                        | what it is                                                                                                  |
| --------------------------- | ----------------------------------------------------------------------------------------------------------- |
| `app.lua`                   | the entry: the command line, the state, `serve` with the calls and the topic of `api/`, the way out         |
| `api/files.lua`             | the calls `list`, `files`, `create`, `rename`, `delete`; a listing is cut to fit one message                |
| `api/buffers.lua`           | the calls `open`, `edit`, `slice`, `save`, `buffer`, `discard`; a long text is announced and read in pieces |
| `api/search.lua`            | the call `search`                                                                                           |
| `api/session.lua`           | the calls `boot`, `ui`, `settings`, `handoff`                                                               |
| `api/changes.lua`           | the topic `changes`: the disk and git, as they change                                                       |
| `api/call.lua`              | how a call is answered: its params checked, what the work raises told with a code the view acts on          |
| `api/params.lua`            | each call says what it asks: what does not fit is answered `bad-params` and never run                       |
| `api/pieces.lua`            | what one message to the view may carry                                                                      |
| `project/tree.lua`          | the project on disk: listing, the index of every file, create, rename, remove, the order of names           |
| `project/git.lua`           | git with a time limit: the repository, its files, its branch and changes                                    |
| `project/watch.lua`         | which of the listed directories changed                                                                     |
| `project/search.lua`        | a search through the files of the project                                                                   |
| `project/paths.lua`         | paths by their text alone                                                                                   |
| `state/session.lua`         | the state of the program: its project, open files, settings, what is told once                              |
| `state/buffers.lua`         | the open files, their versions, drafts                                                                      |
| `state/store.lua`           | settings and each project's open files, where the XDG directories say                                       |
| `state/defaults.lua`        | the defaults the view knows                                                                                 |
| `terminal/command_line.lua` | what textpen was asked to open                                                                              |
| `terminal/handoff.lua`      | the pane given to a shell and taken back, and to the person's own editor                                    |
| `version.lua`               | the version: the one place it is said, which `make release-prepare` sets                                    |

## Size (macOS arm64, 2026-10-24)

`dist/textpen` is 6.2 MB (6,198,450 bytes): the runner 3.9 MB and the view 2.4 MB.
`textpen --version` takes 15 ms.

## Its tests

`test/e2e.test.ts` and `test/calls.test.ts` run the program in a pane under `pencere-fake-host`
and read what the page was told off the host's record: the calls and their answers, `bad-params`,
the order of a listing, an exclusive create, UTF-16 places, git and its time limit, a hangup, the
drafts, the shell. `make test-binary` runs them on `dist/textpen`; `TEXTPEN_BIN` names another
file. A result is cut at 100 characters in the host's record unless `PENCERE_FAKE_GIST` says
otherwise (the tests set it), and a host that cannot show whole results stops the run.

## Details that are not what a person would guess

- **The order of names** is one function for the tree and the search: numbers as numbers, capitals
  with small letters, ties by bytes. Letters with accents are not folded (`é` is after `z`).
- **A search pattern** is the syntax of the Rust `regex` crate, not JavaScript's: `\d`, `\w` and
  `\b` are Unicode ones, and back-references and lookaround are refused with a message, so a
  search ends in time linear in the text. A file that is not UTF-8 is searched with its bad bytes
  replaced, as Node's decoder did.
- **A search** runs in the program's one thread and lets the other calls in between every 25
  files.
- **`active: null`** in the view's state: Lua cannot tell a field that is `null` from one that is
  left out, so a missing `active` is taken as `null`; the other fields are all required.
- **A shell that cannot start** is told to the view (`returned.error`) and not written on the
  terminal for a second and a half.
- **A hangup while the shell has the pane** does not run the program's own code (the runner has no
  signal handler for Lua); the drafts are written when the shell is started, and again when the
  program ends.
- **The `changes` topic** looks every two seconds while it is subscribed.
- **A file name that is not UTF-8** is left out of a listing.
- **Renaming by case alone** (`Ö.txt` to `ö.txt`) is allowed only for ASCII letters: Luau's
  `string.lower` is ASCII-only, so on a disk that ignores case the other is refused as taken.
- **`--dev`** is refused with a pointer to `pencere dev`, which runs the program from its files and
  shows the view again when one under `src/` changes.

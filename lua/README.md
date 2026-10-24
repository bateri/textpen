# textpen's program in Lua

An experiment: the program of textpen (`src/server`, TypeScript on `@pencere/sdk`) written again in
Lua, on the Lua runner of the `pencere` command, so that textpen is one native file with its web
page unchanged. The page (`src/view`, built by vite into `dist/view`) is the same page and speaks
the same calls, with the same params and results: nothing in `src/view` was touched.

```sh
# in a pencere checkout with the Lua runner:  cargo build --release -p pencere-cli
make lua PENCERE=../pencere-lua/target/release/pencere    # dist/textpen-lua, one file
dist/textpen-lua src/server.ts:42                          # run it, in a pane of a terminal with views
```

`make lua` builds the view, copies `dist/view` to `lua/web` (git-ignored), and has `pencere build`
pack `lua/` as an app: `app.lua`, `lib/*.lua`, `assets/` (the four icons) and `web/`. The app lives
in `lua/` because `pencere build` packs every file under every folder of the directory it is given;
from the root it would take `src/`, `test/` and `docs/` with it.

| file              | what it is                                                                                                          |
| ----------------- | ------------------------------------------------------------------------------------------------------------------- |
| `app.lua`         | `main.ts` and `handoff.ts`: the command line, `serve`, the shell and the person's own editor, the way out           |
| `lib/program.lua` | `program.ts`: the calls and the `changes` topic                                                                     |
| `lib/check.lua`   | what zod did: each call says what it asks, what does not fit is `bad-params`                                        |
| `lib/textpen.lua` | `app.ts`: the state of the program                                                                                  |
| `lib/buffers.lua` | `buffers.ts`: the open files, their versions, drafts                                                                |
| `lib/project.lua` | `project.ts`: listing, git, the quick-open index, search                                                            |
| `lib/store.lua`   | `store.ts`: settings and each project's open files                                                                  |
| `lib/wire.lua`    | `wire.ts`: what one message to the view may carry                                                                   |
| `lib/paths.lua`   | Node's `path`, for the few functions used                                                                           |
| `lib/types.lua`   | the defaults the view knows                                                                                         |
| `lib/version.lua` | the version (a built app has no `package.json` beside it; `tools/release.mjs` keeps this and the manifest the same) |

## Size (macOS arm64, 2026-10-24)

|                                                         | raw     | `gzip -9` |
| ------------------------------------------------------- | ------- | --------- |
| `dist/textpen-lua` (runner 4.3 MB + view 2.3 MB)        | 6.6 MB  | 2.5 MB    |
| `deno compile` of the Node program (only what it needs) | 79.8 MB | 30.7 MB   |
| the `scriptc` experiment                                | 5.1 MB  | 2.2 MB    |

The runner without the patterns of `pencere.regex` is 3.1 MB; the patterns (`fancy-regex`, lookaround
included) are 1.2 MB of the runner. `make lua` takes 1.5 s from a built view (the view is vite's 1 s;
`pencere build` is 0.1 s). `textpen-lua --version` takes 15 ms.

## Run its tests

The end-to-end tests run the program in a pane under `pencere-fake-host`. `TEXTPEN_BIN` says which
program; with it (or `TEXTPEN_REQUIRE_E2E=1`) a stand-in or a program that is not there fails the run
in place of skipping it.

```sh
( cd ../pencere-lua && cargo build -p pencere-fake-host )
export PENCERE_FAKE_HOST=$PWD/../pencere-lua/target/debug/pencere-fake-host
TEXTPEN_BIN=dist/textpen-lua node --test test/e2e.test.ts test/parity.test.ts
```

`test/parity.test.ts` reads what the program answers off the host's record and holds it to what the
Node program does: the order of a listing, an exclusive create, `bad-params`, a trap in the keys of
`places`, UTF-16 places, git and its time limit, a hangup, the shell. It runs on either program.

## What is not the same

- **`--dev`** is not in the Lua build: the files of a built app are in the executable. `pencere dev lua`
  runs the program from its files and shows the view again when one under `lua/` changes.
- **The order of names** is one function for the tree and the search (numbers as numbers, capitals
  with small letters, ties by bytes). The Node program orders a search by plain `localeCompare`, so
  `file10` comes before `file9` there. Letters with accents are not folded (`é` is after `z`).
- **A search pattern** is the syntax of the Rust `fancy-regex` crate (lookaround and back-references
  included), not JavaScript's: `\d`, `\w` and `\b` are Unicode ones. A file that is not UTF-8 is searched with its bad bytes replaced, as Node's decoder does.
- **A search** runs in the program's one thread and lets the other calls in between every 25 files;
  the Node one reads 16 files at a time.
- **`active: null`** in the view's state: Lua cannot tell a field that is `null` from one that is left
  out, so a missing `active` is taken as `null`; the other fields are all required, as with zod.
- **A shell that cannot start** is told to the view (`returned.error`) and not written on the terminal
  for a second and a half.
- **A hangup while the shell has the pane** does not run the program's own code (the runner has no
  signal handler for Lua); the drafts are written when the shell is started, and again when the
  program ends.
- **The `changes` topic** looks every two seconds while it is subscribed, as the Node program does.

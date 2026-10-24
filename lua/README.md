# textpen's program

The program of textpen, in Lua, for the runner of the `pencere` command. `make build` packs this
directory (`app.lua`, `lib/*.lua`, `assets/` and `web/`, the built view) into the runner as one
file, `dist/textpen`. The page it shows is `src/view`, built by vite into `dist/view` and copied
here as `web/` (git-ignored) by `make web`; the calls it answers are the ones the page makes, with
the params and results of `src/view/lib/types.ts`.

The program lives in `lua/` and not at the root because `pencere build` packs every file under
every folder of the directory it is given: from the root it would take `src/`, `test/` and
`docs/` with it.

| file              | what it is                                                                                                |
| ----------------- | --------------------------------------------------------------------------------------------------------- |
| `app.lua`         | the command line, `serve`, the shell and the person's own editor, the way out                             |
| `lib/program.lua` | the calls and the `changes` topic                                                                         |
| `lib/check.lua`   | each call says what it asks: what does not fit is answered `bad-params` and never run                     |
| `lib/textpen.lua` | the state of the program                                                                                  |
| `lib/buffers.lua` | the open files, their versions, drafts                                                                    |
| `lib/project.lua` | listing, git, the quick-open index, search                                                                |
| `lib/store.lua`   | settings and each project's open files                                                                    |
| `lib/wire.lua`    | what one message to the view may carry                                                                    |
| `lib/paths.lua`   | paths by their text alone                                                                                 |
| `lib/types.lua`   | the defaults the view knows                                                                               |
| `lib/version.lua` | the version (a built program has no `package.json` beside it; `tools/release.mjs` keeps the two the same) |
| `assets/`         | the icons a terminal draws beside the program's name (`node tools/icon.mjs` makes them)                   |

## Size (macOS arm64, 2026-10-24)

`dist/textpen` is 6.6 MB (6,619,186 bytes): the runner 4.3 MB and the view 2.3 MB. The runner
without the patterns of `pencere.regex` is 3.1 MB; the patterns (`fancy-regex`, lookaround
included) are 1.2 MB of it. `textpen --version` takes 15 ms.

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
- **A search pattern** is the syntax of the Rust `fancy-regex` crate (lookaround and
  back-references included), not JavaScript's: `\d`, `\w` and `\b` are Unicode ones. A file that
  is not UTF-8 is searched with its bad bytes replaced, as Node's decoder did.
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
- **`--dev`** is refused with a pointer to `pencere dev lua`, which runs the program from its
  files and shows the view again when one under `lua/` changes.

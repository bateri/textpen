# textpen's program

The program of textpen, in Rust, on the [`pencere`](https://github.com/bateri/pencere) crate.
`cargo build --release` makes `target/release/textpen`, one file with the page (`src/view/`, a page
of pencere's kit) and the pictures (`src/assets/`) in it: `build.rs` carries them in and
`pencere::app!()` is them. Under `pencere dev` the program reads them from `src/` instead and shows
the page again when one changes. The calls it answers and the topic it feeds are the ones of
`docs/calls.md`, which the page is written to as well.

The rule of the directories is one sentence: **`api/` knows the page; `project/`, `state/` and
`terminal/` do not.** They take and return plain Rust values and never build a message: a call, a
subscription, a push, a `code`. What the page is told is made in `api/` from what they hold.

| file                       | what it is                                                                                                  |
| -------------------------- | ----------------------------------------------------------------------------------------------------------- |
| `main.rs`                  | the entry: the command line, the state, `serve` with the calls and the topic of `api/`, the way out        |
| `api/mod.rs`               | how a call is answered: its params read, what the work could not do told with a code the page acts on       |
| `api/params.rs`            | each call reads what it asks: what does not fit is answered `bad-params`, with where, and never run         |
| `api/pieces.rs`            | what one message to the page may carry                                                                      |
| `api/files.rs`             | the calls `list`, `files`, `create`, `rename`, `delete`; a listing is cut to fit one message                |
| `api/buffers.rs`           | the calls `open`, `edit`, `slice`, `save`, `buffer`, `discard`; a long text is announced and read in pieces |
| `api/search.rs`            | the call `search`                                                                                           |
| `api/session.rs`           | the calls `boot`, `ui`, `settings`, `handoff`                                                               |
| `api/changes.rs`           | the topic `changes`: the disk and git, as they change                                                       |
| `project/tree.rs`          | the project on disk: listing, the index of every file, create, rename, remove, the order of names           |
| `project/git.rs`           | git with a time limit: the repository, its files, its branch and changes                                    |
| `project/watch.rs`         | which of the listed directories changed                                                                     |
| `project/search.rs`        | a search through the files of the project                                                                   |
| `project/paths.rs`         | paths by their text alone                                                                                   |
| `state/session.rs`         | the state of the program: its project, open files, settings, what is told once                              |
| `state/buffers.rs`         | the open files, their versions, drafts                                                                      |
| `state/store.rs`           | settings and each project's open files, where the XDG directories say                                       |
| `state/defaults.rs`        | the settings and the page's state, their defaults and the names of their choices                            |
| `state/later.rs`           | work done a moment after it was last asked for: a draft, the page's state                                   |
| `terminal/command_line.rs` | what textpen was asked to open                                                                              |
| `terminal/machine.rs`      | who runs textpen and where: the user, the machine, ssh                                                      |
| `terminal/handoff.rs`      | the pane given to the person's own editor in a terminal that shows no views                                 |

The version is the package's (`Cargo.toml`).

## Size (macOS arm64, 2026-10-28)

`target/release/textpen` is 3.7 MB (3,718,112 bytes, symbols not stripped) with the page as it was
that day. `textpen --version` takes under 10 ms.

## Its tests

`cargo test` runs the program's own tests and the ones in `tests/`, which run the program Cargo
built in a pane under `pencere-fake-host`, pencere's stand-in terminal, and read what the page was
told off its record: the calls and their answers, `bad-params`, the order of a listing, an exclusive
create, places in code points (with characters of two, three and four bytes), git and its time
limit, a hangup, the drafts, a long text in pieces, a big directory, the limits of a search, the
command line. The stand-in is the one `FAKE_HOST` names, or
`../pencere/target/debug/pencere-fake-host` (`cargo build -p pencere-fake-host` in a pencere
checkout); `make fake-host` builds one of the pencere `Cargo.lock` takes, in `.cache/pencere`. One
that is not there, or that cannot show a whole result (`PENCERE_FAKE_GIST`), fails the run.
Nothing is skipped.

## Details that are not what a person would guess

- **Places are code points**: an edit, a slice, a search match, a draft's length. The text is held
  as UTF-8; a place inside a character cannot be named.
- **The buffer calls take one lock**, so that one runs at a time; the page waits for each answer
  before it relies on the order. The order of calls sent together comes with the next pencere, whose
  lanes run a program's calls in the order they were sent.
- **`null` and absent are different** in what the page sends: a field that may be left out is
  refused when it is `null`, and `active` is there, a path or `null`. A key of `places` is a key and
  nothing more, `__proto__` included.
- **The order of names** is one function for the tree and the search: numbers as numbers, capitals
  with small letters, ties by bytes. Letters with accents are not folded (`é` is after `z`). A
  directory of more than five thousand entries is sorted and then cut: the page has the first five
  thousand in that order.
- **A search pattern** is the syntax of the Rust `regex` crate, not JavaScript's: `\d`, `\w` and
  `\b` are Unicode ones, and back-references and lookaround are refused with a message, so a search
  ends in time linear in the text. A file that is not UTF-8 is searched with its bad bytes replaced;
  a file over 2 MiB is not searched; a search stops at 2,000 matches.
- **A search** runs on a thread of its own, as every call does, and stops when the page gives it up.
- **The `changes` topic** looks every two seconds while it is subscribed: a `stat` of each listed
  directory and open file, and `git status` at most every two and a half seconds. A watch of the
  system's (`notify`) would not see git's own state change, nor a disk shared over the network, and
  would hold a watch for every directory the page ever listed.
- **git** is asked with `GIT_OPTIONAL_LOCKS=0`, so that the program's `git status` never takes the
  index's lock from a person running git beside the editor; one that takes 15 seconds is given up
  on, and the files are walked instead.
- **A file name that is not UTF-8** is left out of a listing, the index and git's changes.
- **Renaming** is refused when the new name is another file. A rename by case alone (`Ö.txt` to
  `ö.txt`) is allowed for any letter: on a disk that ignores case the new name is the same file.
- **Removing** the project, or a directory the project is in (`..`), is refused.
- **Drafts** are written a second and a half after the typing stops, and when the program ends,
  however it ends: ⌃C, a hangup (pencere's signal handling gives the program back its control, and
  the drafts are written then), the view closed. A draft and a project's state are named by the
  start of the SHA-256 of their path, as the textpen before this one named them, so what it left is
  found.
- **A settings file of an older textpen** is read: `vim` and `fontSize` are dropped, and
  `whitespace: true/false` is `"all"`/`"none"`.
- **The pane is not handed to a shell** yet: `handoff` answers `unavailable` and `boot` says
  `handoff: false` until pencere can close a view, give the terminal to a shell and take it back.
- **The command line**: a file named after a directory is from the current directory, as a shell
  names it, not from the project. `problems` is always empty: what cannot be opened is told when the
  page opens it.
- **`--dev`** is refused with a pointer to `pencere dev`, which builds the program and shows the
  page again when one of its files changes.

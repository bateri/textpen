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
| `api/tests.rs`             | the calls in a conversation in the same process: calls sent together, the pane lent and given back          |
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
| `terminal/handoff.rs`      | the pane lent to a shell and taken back, and to the person's own editor where no views are shown            |

The version is the package's (`Cargo.toml`).

## Size (macOS arm64, 2026-10-28)

`target/release/textpen` is 3.7 MB (3,718,112 bytes, symbols not stripped) with the page as it was
that day. `textpen --version` takes under 10 ms.

## Its tests

`cargo test` runs the program's own tests and the ones in `tests/`. Among its own, `api/tests.rs`
makes the page's calls in a `pencere::Conversation`, which serves the handlers by `serve`'s code
with no terminal: a hundred edits sent together and made in their order, the pane lent after the
answer that asks for it, and the page shown again told how the shell ended. The ones in `tests/` run
the program Cargo built in a pane under `pencere-fake-host`, pencere's stand-in terminal, and read
what the page was told off its record: the calls and their answers, `bad-params`, the order of a
listing, an exclusive create, places in code points (with characters of two, three and four bytes),
git and its time limit, a hangup, the drafts, a long text in pieces, a big directory, the limits of
a search, the command line, and the shell: its directory, its environment and its first line, the
editor shown again and told how it ended, a shell that cannot start, a hangup while it has the pane. The stand-in is the one `FAKE_HOST` names, or
`../pencere/target/debug/pencere-fake-host` (`cargo build -p pencere-fake-host` in a pencere
checkout); `make fake-host` builds one of the pencere `Cargo.lock` takes, in `.cache/pencere`. One
that is not there, or that cannot show a whole result (`PENCERE_FAKE_GIST`), fails the run.
Nothing is skipped.

## Details that are not what a person would guess

- **Places are code points**: an edit, a slice, a search match, a draft's length. The text is held
  as UTF-8; a place inside a character cannot be named.
- **The buffer calls are a lane**, `buffers`: they run one at a time in the order the page sent
  them, so an `edit` and the `save` of the version it makes may go together. The other calls run
  beside them, and beside each other; the open files are behind one lock all the same, which the
  `changes` topic, `rename`, `boot` and the drafts take too.
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
- **The pane lent to a shell**: `handoff` writes the drafts, then asks pencere to lend the terminal
  (`ViewControl::hand_off`) and answers `scheduled`; the library sends that answer before it closes
  the view, so nothing waits for it to go. The shell (`$SHELL`, or `/bin/sh`) runs in the project's
  directory as the terminal's foreground job, after a first line that says where it is, with
  `TEXTPEN_SHELL=1` and the switches that keep iTerm2's and WezTerm's rc integrations quiet. When it
  ends the view is shown again and the next `boot` tells `returned`, once; it waits up to five
  seconds for it, since the view shown again can ask before the thread that waited for the shell
  has kept it. A second `handoff` while the shell has the pane is `busy`. A terminal that cannot
  close a view is told on `changes` (`handoffFailed`), and from then on `handoff` is `unavailable`
  and `boot` says `handoff: false`.
- **A hangup while the shell has the pane** is passed on to the shell by pencere, and `serve` ends
  once it has: the drafts are written then, as for any hangup. A view that does not come back after
  the shell for another reason (the terminal did not answer, or would not show it) is not replaced
  with the person's own editor: textpen asks on the terminal whether to try again, keeping what is
  not saved either way.
- **The person's own editor**, in a terminal that shows no views, is lent the terminal the same
  way: its own process group, the terminal's foreground, ⌃C and ⌃Z its own.
- **The command line**: a file named after a directory is from the current directory, as a shell
  names it, not from the project. `problems` is always empty: what cannot be opened is told when the
  page opens it.
- **`--dev`** is refused with a pointer to `pencere dev`, which builds the program and shows the
  page again when one of its files changes.

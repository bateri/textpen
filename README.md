# textpen

A code editor in the terminal pane: the project's files as a tree, tabs, search across
them, and the text in the colours and the font of your terminal — with the keyboard or
the mouse, on your machine or on a server over plain `ssh`.

![textpen: a TypeScript file in bateri's theme, with the project's files and what git says of them](docs/editor.png)

textpen is a [pencere](https://github.com/bateri/pencere) program. Run it in a terminal
that hosts pencere views ([bateri](https://github.com/bateri/bateri)) and the pane becomes
the editor, the way `vim` takes the screen and gives it back; quit and the shell is where
it was. Run it anywhere else and your own `$EDITOR` opens instead.

## What it does

**The files of the project**, as a tree with what git says of each — changed, new, in
conflict — and of the folders that hold them; which files are open and which have changes
that are not saved. Make, rename, move and delete files and folders from it. It follows the
file you are in.

**An editor** ([CodeMirror](https://codemirror.net)) that reads well over a hundred
languages, each loaded when a file in it is first opened; Markdown with the code in its
fences read in their own languages. Multiple cursors, folding, bracket matching, find and
replace in the file with a count of the matches, go to a line, comment lines out, move and
copy them. Vim keys if you want them (`:w`, `:q`, `:wq`, `:qa`).

**Every colour is the terminal's.** A terminal gives its view a handful of colours — its
background and foreground, a dimmed one, an accent, and the colours it says success, danger,
warning and information with — and textpen draws code with those, as the theme's author
chose them to read on that background: keywords in the accent, strings in the success
colour, numbers in the warning one, types in the information one. Change the terminal's
theme and the editor follows, live.

**Search in the project's files**, as words, whole words or patterns, with the matches by
file; go through them from anywhere with `F4` and `⇧F4`.

**A preview of Markdown**, beside the text in a wide pane and in its place in a narrow one.

**Nothing is lost to a ⌃C.** textpen keeps what you have not saved, and when it ends — a
⌃C, a dropped connection — it writes a draft; open the file again and it offers to bring
the changes back. A file another program changes is read again when you have no changes of
your own, and when you do, textpen asks which to keep, and does not write over it without
asking.

**A shell in the pane.** `⌘J` hands the pane to a shell in the project's directory; `exit`
brings the editor back, where you left it, unsaved changes and all.

**It fits the pane.** In a wide pane the files are beside the editor, the edge between them
dragged where you want it; in a narrow one (80×24) they slide over it with `⌘B`, and the
status line keeps what still fits.

|                                                                 |                                                           |
| --------------------------------------------------------------- | --------------------------------------------------------- |
| ![Go to a file, in Tokyo Night](docs/palette.png)               | ![Search in the project, in Gruvbox](docs/search.png)     |
| ![Markdown and its preview, in a light theme](docs/preview.png) | ![An 80×24 pane in Solarized Light](docs/light-80x24.png) |

## The keyboard, and the mouse

Everything the mouse does, the keys do too: the palette has every command, the status line
says what the keys do where you are, and `F1` lists them all. The keys a terminal keeps for
itself never reach a view — in bateri `⌘P`, `⌘W`, `⌘O`, `⌘T`, `⌘N`, `⌘D` and `⌘1`–`⌘9` are
its tabs, splits and windows — so textpen's commands are on keys it leaves free, and every
one is in the palette. No command is on `⌥` and a letter: on a Mac that is how many layouts
type `@`, `#`, `[` or `{`.

|                            |                                                                           |
| -------------------------- | ------------------------------------------------------------------------- |
| `⌘K`                       | go to a file (`name:12` goes to a line); `⌘K` again, or `>`, the commands |
| `F1`                       | every command                                                             |
| `⌘E`                       | the open files, by when you last saw them                                 |
| `⌘S`, `⌥⌘S`                | save, save all                                                            |
| `⌃W`, `⇧⌘T`                | close the tab, open the one closed last                                   |
| `⌃PgUp` `⌃PgDn`, `⌃1`–`⌃9` | the previous and next tab, a tab by its place                             |
| `⌘B`, `⇧⌘E`, `F6`          | the files: show or hide, go to them, between them and the editor          |
| `⌘F`, `⌥⌘F`, `⇧⌘F`         | find, replace, search in the project                                      |
| `⌘L`                       | go to a line                                                              |
| `⌃G`, `⇧⌘L`, `⌃⇧↑` `⌃⇧↓`   | add the next match, every match, a cursor above or below                  |
| `⌘/`, `⌥↑` `⌥↓`, `⇧⌘K`     | comment, move the line, delete it                                         |
| `⇧⌘V`                      | the preview of a Markdown file                                            |
| `⌘+` `⌘−` `⌘0`             | bigger, smaller, actual text                                              |
| `⌘J`                       | a shell in the project, in this pane                                      |
| `⌃Q`                       | quit, asking about what is not saved                                      |

The tree has the keys once you click a row in it, or press `⇧⌘E` or `F6`; then `j` `k` and
the arrows move, `h` `l` fold and unfold, `⏎` opens, `a` makes a file and `A` a folder, `r`
renames or moves, `d` deletes, `y` copies the path, `.` opens the menu of the row. `Esc`
goes back to the editor from anywhere. The keys the status line shows are buttons too: a
click does what the key does.

`⌃C` always reaches textpen and ends it, whatever has the keyboard; what is not saved is
kept as a draft. On Linux and Windows, `⌘` is `Ctrl`.

## Run it

It needs Node 22 where it runs.

To see the editor you need a terminal that hosts pencere views. bateri is the first, and no
release of it has them yet: until one does, build bateri from its `main` branch. In any other
terminal textpen opens your own `$EDITOR`.

```sh
npm install -g textpen
textpen                             # or, without installing it: npx textpen
textpen src/server.ts:42            # a file, at a line (`+42 file` works too)
textpen ~/projects/lantern          # a folder as the project
```

With no file named, textpen opens on the folder you are in, with the files that were open
there the last time. Settings are kept in `~/.config/textpen/`, drafts and each project's
open files in `~/.local/state/textpen/` (or where `XDG_CONFIG_HOME` and `XDG_STATE_HOME`
say).

**As your `$EDITOR`.** `export EDITOR=textpen` and `git commit` opens the message in the
pane: save it and close the tab (or `:wq`), and git goes on. Opened on files named on the
command line, textpen ends when the last of them is closed. In a terminal without pencere it
passes the file to `$VISUAL` or `$EDITOR` — skipping itself — or to `vi`.

**Over ssh.** Install it on the server and run it there, in a pane of a terminal that hosts
pencere: nothing listens on a port, the editor travels in the terminal's own stream, and it
runs on your machine in a sandbox that reaches nothing but its program. Its files cross the
stream once (about 2 MB); after that the terminal keeps them.

## How it works

- `src/server` is textpen's program, and **holds the text of every open file**. The editor
  asks it with a handful of calls (open, edit, save, list, search…) and hears from it on one
  topic, what changed on disk and in git. What the editor sends travels through the terminal's
  input queue, in frames of 511 bytes and messages of at most 1 MiB, so it never sends a file:
  it sends what changed, a few hundred milliseconds of typing at a time, as replacements made
  on a version of the text the program checks; saving is a word to the program. A message the
  other way is at most 1 MiB too, and every character outside ASCII crosses as six bytes, so a
  long text, a long folder and the files of a big project come to the editor in pieces. Holding
  the text is also what lets it outlive the view — a shell, a restart of the terminal — and
  become a draft when the program ends.
- `src/server/main.ts` is the one place that knows about the terminal: `serve` from
  `@pencere/sdk` opens the view in the pane; a `ViewControl` closes it for the shell and the
  next view opens where the last one was.
- `src/view` is the editor: React, CodeMirror and [Tabler icons](https://tabler.io/icons).
  `src/view/lib/page.ts` is all it knows of the protocol: it calls and subscribes, is told the
  terminal's colours, font and size, and asks to be closed or to open a link. It tells the
  program where it is (tabs, folders, cursors) every two seconds when that changed.

## Work on it

```sh
git clone https://github.com/bateri/textpen && cd textpen
npm install
make check          # types, build, formatting, and the tests: the program, the view's edits,
                    # and the editor in a pane under pencere-fake-host when a pencere
                    # checkout is beside this one
npm run harness     # http://localhost:5198 — the editor in a stand-in host, on demo/
npx vite build --watch & npm run dev   # in a pencere terminal: the editor is shown again
                                       # each time its build changes
```

How a version is released is in [RELEASING.md](RELEASING.md), and what each one changed in
[CHANGELOG.md](CHANGELOG.md).

The harness plays the terminal in a browser: it frames the editor with hot reload, tells it
the colours of a terminal theme (bateri's, Tokyo Night, Gruvbox, Dracula, Solarized and
more) and the size of a pane (80×24 up), and passes its calls and subscriptions to the real
program, which edits `demo/` — a small shop to look at — or the folder `TEXTPEN_ROOT` names. It keeps its
settings and drafts in `.scratch/`. In its address, `theme=` and `size=` choose the
terminal, `bare` shows the pane alone (for screenshots), `built` shows the view `npm run
build` wrote, and `ui=` (JSON) opens with those tabs and folders.

## License

textpen is free software: you can redistribute it and/or modify it under the terms of the
[GNU General Public License](LICENSE) as published by the Free Software Foundation, either
version 3 of the License, or (at your option) any later version. It is distributed in the
hope that it will be useful, but WITHOUT ANY WARRANTY; see the license for details.

# Changelog

Every change a user of textpen would notice, newest first. The format is
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[semantic versioning](https://semver.org/): while textpen is `0.x`, a minor release may
change what the previous one did.

## [Unreleased]

### Added

- **Drop a file on the tree to put it in the project.** Files dragged from Finder over the page mark
  where they would go and say so ("Upload to docs/"): a folder's row is that folder, a file's row
  is its folder, and the empty room in the tree is the project's own folder. Dropped, each file is
  a faint row in that folder at once, with how far it has got at its end ("42%") and a Cancel button
  under the pointer; Cancel, ⌃C and closing the view delete what was received, so no half file is
  left. Where the name is taken, one question is asked, Replace or Keep Both (the new file is `foto
  2.png`), and Esc gives the file up; no file is written over another unasked, even one that came
  to be there while the bytes were on their way. A folder dropped is not uploaded ("Folders can't
  be uploaded yet"); the files dropped with it are. A picture dropped alone is shown in a tab from
  the file you gave before it is all in. A file dropped on the editor is uploaded beside the file
  shown, the same on this machine as over ssh. The program has
  the calls `place` and `upload` for it (`docs/calls.md`), and the page needs a terminal that opens
  it at `lua` 0.1 (bateri does).
- **Upload Files and Download from the keyboard.** The right-click menu of a folder in the tree, and
  the command palette where a folder is chosen in the tree, have Upload Files: the terminal's Open
  panel, and what you choose goes into that folder as a dropped file does. A file's menu, and the
  palette where a file is chosen (or shown), have Download: the Save panel proposing the file's
  name, the file written where you choose, and a note, "Downloaded foto.png". The program has the
  call `download` for it (`docs/calls.md`).
- **Files go through the terminal's folder, not its stream.** Where the terminal keeps a folder on
  textpen's machine (bateri does, here and on a server it reaches with kablo), a file you drop,
  paste or choose, and a file you download, cross a pipe in it at the speed of the disk or the
  line. textpen asks the same wherever it runs; a terminal that keeps no folder still carries the
  file in its stream.
- **Paste a picture into a Markdown file.** `⌘V` with a picture on the pasteboard, while you edit a
  Markdown file, puts the picture beside the file as `image.png` (`image-1.png`, `image-2.png`
  where the name is taken, so nothing is written over) and writes `![](image.png)` where the caret
  is. The preview holds a place for the picture, with how far it has got, until it is whole, then
  draws it. A picture pasted in any other file goes nowhere. It uses the program's `place`
  (with `dash`) and `upload` calls (`docs/calls.md`).
- **A file on its way waits out a pause, and says when it has stopped.** The terminal pauses for a
  few seconds now and then while a file crosses it (4.4 s at the longest measured): the row keeps its
  percent and shows no error. With no word for 30 seconds the row says "Upload stalled" and has a
  Retry button, which asks the terminal for the file's bytes again from the start (what was received
  is not kept; the old attempt's part is another file and is deleted when it ends). A screen reader
  reads a row as "Uploading, 42 percent", and "Upload stalled". In a narrow pane (80×24) the name of a
  row gives way first, its last 22 points faded, and what is said at its end fades over its last 22
  points where it does not fit (a stalled row with Cancel under the pointer); the whole of each is
  in the tip.
- **A picture opens in its tab.** A click on a PNG, a JPEG or an SVG in the tree shows it fitted into
  the tab (smaller in its proportions where it does not fit, never made larger), with how large the
  file is and its pixels under it ("48.2 KB · 640 × 480"). A picture of more than 8 MiB, or of more
  than 4096 pixels either way, is not drawn: the tab says "Too large to preview" with the file's
  size, and offers Close Tab. A file named like a picture that is none (a text, a JPEG that is
  cut short, an SVG that says no size) opens as it did, and so does any other picture format. The
  program reads and measures the file (the sizes are in the first bytes, no library is added) and
  gives it to the terminal to hold while the tab is open, which needs a terminal that opens the
  page at `lua` 0.1 (bateri does; with one that does not, a picture is the placeholder it was). The
  program has the calls `picture` and `release` for it (`docs/calls.md`).
- **A Markdown preview draws the pictures it names.** `![](docs/x.png)` is the picture, fitted to the
  width of the text, where `docs/x.png` is a PNG, a JPEG or an SVG in the project, found from the
  Markdown file's own folder; one that is not there, is past the limits above, is out of the project
  (a path that climbs out of it, or leads out by a link), or is an address, is its words, as it was.
- **The editor has a right-click menu**, as VS Code's: Cut, Copy, Paste, Select All, Toggle Line
  Comment and Select All Occurrences, each with its keys, then Find in Project (the selection, or
  the word at the caret), Revert Change (on a line git has changed) and Command Palette. A right
  click outside the selection puts the caret there; `⇧F10` opens the same menu at the caret.
- **The lines that differ from git's index are marked in the editor**, as VS Code marks them: a bar
  down the gutter (between the numbers and the buttons that fold) along the lines changed (blue)
  and added (green), a small red triangle where lines were taken away, and the same marks on the
  editor's right edge so that they are found in a long file. A click on one, or `⌥F5` and `⇧⌥F5`
  (go to the next and the previous change), opens a peek under the change with the lines as they
  were (on red) and as they are (on green), Revert Change (an edit that can be undone), the change
  before and after, and `Esc` to close it. The original is git's index, so a staged change is not
  marked and neither is a file git does not keep; the marks follow what you type, and the setting
  `Changes in the gutter` turns them off. A file with CRLF line ends or a byte order mark is compared
  as textpen holds it, not as one change from its first line to its last. The program has a
  `diff` call for it (`docs/calls.md`), the `changes` topic tells when git's index was written, and
  the Settings have `diffDecorations`.

### Changed

- The editor's cursor and selection are drawn as your terminal draws its own (in bateri: the
  cursor's shape, its rounding and glow, how it blinks and glides, the rounded selection), and
  follow it when you change those settings; a terminal that does not say how leaves the editor's
  own look.
- The tree shows whether a folder is open by its icon alone, with no arrow beside it, and the
  pointer stays a hand over its rows when one is clicked.
- The tree scrolls across when a name is wider than the sidebar, where it cut the name short with
  an ellipsis: every name is whole, what git says of a file stays at the sidebar's edge (or right
  after a name that goes past it), and each level of folders is indented further, 16 points where
  it was 12.
- Dragging the sidebar's edge keeps up with the pointer in a project with many folders open: while
  the edge is held the tree makes only the rows in view, and a move that leaves the edge where it
  is draws nothing.
- The numbers of the lines keep more room to their left, away from the tree.
- The status line tells a selection of more than 256 KiB, or one over more than 256 carets, in bytes
  ("1.2 MB selected"), where it counts the characters of what is selected up to that: the editor
  says its places in bytes, and a text that large is not read to be counted. Where the caret was
  when a file was left is kept in bytes too: one kept before comes back that many bytes into the
  file, a little earlier than it was where the text has accents before it.
- The editor is a page of pencere's kit, in Lua, and the text is in the terminal's own editor, drawn
  in its font, in place of a web page with CodeMirror. It reads 33 languages in place of well over
  a hundred: JavaScript, TypeScript, JSX, TSX, Rust, Go, C, C++, C#, Java, Kotlin, Swift, PHP,
  Python, Ruby, the shell, Lua, HTML, XML, Markdown, JSON, YAML, TOML, INI and `.env`, CSS, SCSS,
  Less, SQL, PostgreSQL, SQLite, the Dockerfile, the Makefile and a diff, each with its own comments, indentation
  (Python's after a colon, Ruby's and Lua's by their words) and folding. Each open file keeps what
  can be undone, its cursors and its folds while another is shown. The tooltips, menus and the list
  of commands are the terminal's own.
- The quick open keeps up with a project of tens of thousands of files, the list filling in as it
  is searched, and a long Markdown file's preview is read where you changed it and shown to its
  end, a screen of it at a time.
- A file's language is told by its name, its extension or its first line (`#!/usr/bin/env python3`),
  and every one of the 33 is in Change Language.
- The other places of what is selected are marked in the text. Select Line, Join Lines, Duplicate
  Line Up, Insert Line Below, Split Line, Transpose Characters, Delete to Start and End of Line
  and Fold All are commands, in the palette (`⌘I`, `⌃J` and `⌃⌥[` are the keys of three of them);
  Delete Line is also on `⌃⇧K`.
- A box opened with something in it (the find bar on the words you selected, the palette on `>`
  and a query, the search, the dialog that renames a file) has that selected, so that what you
  type takes its place; renaming selects the file's name, not its folders or its extension.
  Backspace in a palette with nothing typed goes back to the files.
- A file that changed on disk while you had no changes of your own is changed where it differs, not
  loaded again whole: what you did before in it can still be undone.
- A file opened at a place (a match of the search, `name:12`) has that line in the middle of the
  editor.
- The unsaved changes an earlier run left say how long ago they were kept ("7 minutes ago"), and
  what is waited for turns.
- The status line loses the branch when the repository is removed, where it kept the old one.
- The shortcuts of the commands are written as the terminal's own system writes them (`⇧⌘K` on a
  Mac, `Super+Shift+K` elsewhere), in the palette, the menus and the status line.
- What is long — a text, a folder, the file index, a search — comes to the editor in pieces of a
  quarter of a message (256 KiB), which it reads in the time it has for one: a folder shows as many
  of its entries as fit in one, a few thousand, where it showed five thousand.
- The source has one layout: the program in `src/program/` (Rust, by what each file does), the
  editor in `src/view/` and the icons in `src/assets/`.
- textpen is one native file, its program written in Rust on the pencere crate, with the editor
  inside it (about 4 MB): macOS on Apple silicon and Linux on `arm64` and `x86_64`, as files of
  the GitHub release. Node is no longer needed where it runs, nor to build it, run it or test it
  (`cargo build`, `make check`), and textpen is no longer published on npm: `npm install -g
  textpen` and `npx textpen` stop at the last release there, which is deprecated with a pointer to
  the files.
- The setting of which spaces the editor shows is one of five, as the terminal's editor names
  them: none, at the edges of words (`boundary`), in what is selected (`selection`, the default),
  at the ends of lines (`trailing`) and all, in place of on and off. A new setting, `indentGuides`
  (on by default), shows the guides of indentation. A settings file of an older textpen is read:
  `whitespace` on is all and off is none.
- Renaming a file or folder by case alone (`Ö.txt` to `ö.txt`) works for every letter, not only
  the ASCII ones; a rename onto another file that is there is refused.
- Deleting the project's own folder, or a folder the project is in, is refused.
- Files named on the command line after a folder are found from the current directory, as your
  shell names them, and not from inside that folder: `textpen src src/main.rs` opens the project
  `src` on `src/main.rs`.
- The shell in the pane, and your own editor in a terminal that shows no views, run as the
  terminal's foreground job, as a shell runs a command: ⌃C and ⌃Z are theirs. A connection that
  drops while the shell has the pane ends the shell, and then textpen, which keeps what is not
  saved as drafts.
- Speaks pencere's one protocol (a call, a subscription, a response, a push) in place of MCP
  Apps. The editor is the same one, in the terminal's colours and font, in a terminal that
  hosts the new views; it no longer opens in one that only speaks MCP Apps.
- The program tells the editor when files change on disk or in git, as it happens, in place of
  the editor asking every two seconds.
- A pattern in the project search is a Rust regular expression, not a JavaScript one: `\d`, `\w`
  and `\b` mean Unicode digits, letters and word edges, and back-references and lookaround are
  refused with a message, so a search ends in time linear in the text.
- Names are ordered with numbers as numbers and capitals with small letters, in the tree and in
  a search alike; a search used to put `file10` before `file9`. Letters with accents are not
  folded: `é` comes after `z`.
- A file whose name is not valid UTF-8 is not shown in the tree and is not searched.
- Search in the project reads the files one at a time, on a thread of its own, in place of
  sixteen at a time.
- Labels, hints and messages no longer end in an ellipsis: a menu item or a command reads "Rename"
  or "New File", and a wait says "Searching" or "Reading the folder", the same whole phrase
  wherever it is shown. A name or a line cut to fit its room still ends in one.

### Removed

- The MCP server: `show_editor`, `list_files` and `read_file`, the view for other MCP Apps
  hosts, and `npx -y textpen` as a stdio server for an MCP client. Started with no terminal at
  all, textpen says it needs one.
- `--dev`, which showed the view again when its built files changed (`make dev`, which runs
  `pencere dev`, does that for the program's files), and `TEXTPEN_TRACE`, the trace of the turns
  with the terminal.
- The `vim` and `fontSize` settings, and `⌘+`, `⌘−` and `⌘0`: there is no vim mode, and the text
  is the size the terminal gives it (its own keys make it larger or smaller), with no zoom of
  textpen's own. A settings file that has them is read without them.

### Fixed

- A line picked in a right-click menu is the line of the menu you were shown. The page took it from
  the menu it had asked for last, and a terminal shows one menu at a time, so a pick could run a line
  of a menu that was never up.

## [0.1.0] - 2026-10-22

### Added

- The project's files as a tree, with what git says of each file and of the folders that hold
  them, which files are open and which have changes not saved; make, rename, move, delete
  and copy the path of files and folders from it. It follows the file you are in.
- An editor (CodeMirror) for well over a hundred languages, each loaded when a file in it is
  first opened, and Markdown with its fenced code read in the code's own language; multiple
  cursors, folding, find and replace with a count of the matches, go to a line, comment,
  move and delete lines, and vim keys if you want them.
- Tabs, the open files by when you last saw them (`⌘E`), a palette for files, lines and every
  command (`⌘K`, `F1`), search in the project's files as words, whole words or patterns, and a
  preview of Markdown beside the text or in its place.
- Colours and font from the terminal's theme, the code's too, following a change of theme
  live; a layout that fits the pane, down to 80×24, with the files sliding over the editor in
  a narrow one.
- Nothing is lost to a `⌃C` or a dropped connection: what is not saved becomes a draft, offered
  the next time the file is opened. A file changed on disk is read again, or, with changes of
  your own, textpen asks which to keep.
- A shell in the pane (`⌘J`), in the project's directory: `exit` brings the editor back
  where it was, unsaved changes and all.
- As your `$EDITOR`: opened on files, it ends when the last of them is closed; in a terminal
  without pencere it runs `$VISUAL`, `$EDITOR` or `vi` instead.
- Everything by keyboard and by mouse alike, on keys a terminal leaves to its views, and none
  on `⌥` and a letter.
- As an MCP server: `show_editor` shows the editor in any MCP Apps host, and a model can list
  the project's files and read them as textpen has them; what writes is the person's.
- An icon, for a terminal to draw beside textpen's name above the view.
- Speaks version 1 of the pencere wire, through `@pencere/sdk` 0.1.

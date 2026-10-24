# Changelog

Every change a user of textpen would notice, newest first. The format is
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[semantic versioning](https://semver.org/): while textpen is `0.x`, a minor release may
change what the previous one did.

## [Unreleased]

### Changed

- textpen is one native file, built from Lua by pencere's runner, with the editor inside it
  (about 6 MB): macOS on Apple silicon and Linux on `arm64` and `x86_64`, as files of the
  GitHub release. Node is no longer needed where it runs, and textpen is no longer published on
  npm: `npm install -g textpen` and `npx textpen` stop at the last release there, which is
  deprecated with a pointer to the files.
- Speaks pencere's one protocol (a call, a subscription, a response, a push) in place of MCP
  Apps. The editor is the same one, in the terminal's colours and font, in a terminal that
  hosts the new views; it no longer opens in one that only speaks MCP Apps.
- The program tells the editor when files change on disk or in git, as it happens, in place of
  the editor asking every two seconds.
- A pattern in the project search is a Rust regular expression (lookaround and back-references
  work), not a JavaScript one: `\d`, `\w` and `\b` mean Unicode digits, letters and word edges.
- Names are ordered with numbers as numbers and capitals with small letters, in the tree and in
  a search alike; a search used to put `file10` before `file9`. Letters with accents are not
  folded: `é` comes after `z`.
- A file whose name is not valid UTF-8 is not shown in the tree and is not searched.
- Search in the project reads the files one at a time, on one thread, in place of sixteen at a
  time.

### Removed

- The MCP server: `show_editor`, `list_files` and `read_file`, the view for other MCP Apps
  hosts, and `npx -y textpen` as a stdio server for an MCP client. Started with no terminal at
  all, textpen says it needs one.
- `--dev`, which showed the view again when its built files changed (`pencere dev lua` does that
  for the program's files), and `TEXTPEN_TRACE`, the trace of the turns with the terminal.

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

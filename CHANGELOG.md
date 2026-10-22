# Changelog

Every change a user of textpen would notice, newest first. The format is
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[semantic versioning](https://semver.org/): while textpen is `0.x`, a minor release may
change what the previous one did.

## [Unreleased]

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

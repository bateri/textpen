# Releasing

textpen is released as three files on a GitHub release tagged `v<x.y.z>` here: the program for
macOS on Apple silicon (`arm64`) and for Linux on `arm64` and `x86_64`, named
`textpen-<version>-<target>`, with a `SHA256SUMS` that names the three. There is no Intel Mac file:
bateri, the terminal that shows the editor, runs on Apple silicon only. It is not published on npm.
Every step a machine can check is in `tools/release.mjs`, behind the `make` targets below. This
file is the order of the steps and the reasons for them.

## Versions and the changelog

textpen follows [semantic versioning](https://semver.org/). While it is `0.x`, a minor release
may change what the previous one did.

Every change a user would notice goes under `## [Unreleased]` in `CHANGELOG.md`
([Keep a Changelog](https://keepachangelog.com/en/1.1.0/)), in the same commit as the change. A
release only moves that section under a version and a date; it does not write it.

## What a release needs

Each of the three files is the Rust program with its page (`src/view/`) and pictures
(`src/assets/`) carried in it, built by cargo for one target. So a release needs:

- Rust 1.88, the toolchain the gate runs with, on a machine of each target: a Mac on Apple silicon,
  and Linux on `arm64` and on `x86_64`, each with its target added (`rustup target add
  aarch64-unknown-linux-musl`, and so on) and, on Linux, `musl-tools`. Each file is built where it
  runs, as sqlpen's are: no emulation and no cross toolchain.
- pencere as `Cargo.lock` takes it: cargo builds it into each file from that source, a commit of
  `bateri/pencere` while textpen needs what no release of it has. Nothing of pencere's is
  downloaded ready-made.
- Node, for `tools/release.mjs`: it is the one step of a release that is not cargo's. (The other
  Node file, `tools/icon.mjs`, draws the icons and is not part of a release.)
- `gh`, logged in, to make the GitHub release.

The files are built on the maintainers' machines and not by a workflow: an executable that a
workflow built is one that a pushed tag changes without a maintainer's decision. CI runs the gate
(`make check`) on Linux and macOS on every push and pull request; a release runs it again here.

## A release

From a clean `main`:

1. `make release-prepare VERSION=0.2.0` sets the version in `Cargo.toml` and `Cargo.lock` (the
   program says that version: `textpen --version`), and turns `## [Unreleased]` into
   `## [0.2.0] - <today>` with a new, empty `## [Unreleased]` above it. Read the diff, then commit it
   as `Release textpen 0.2.0`.
2. From that commit, on a clean tree, on a machine of each target: `make dist
   TARGET=<target>`, which builds `dist/release/textpen-0.2.0-<target>`. Gather the three in
   `dist/release` on the machine that releases, then `make sums` writes `SHA256SUMS`. The file names
   carry the version, so a `dist/release` built before the version moved is refused; nothing can
   tell which source a file of the right version was built from, so build them after the commit
   and not before.
3. `make release` runs the gate (`make check`, with `FAKE_HOST` naming pencere's stand-in terminal
   if it is not where the Makefile looks: `make fake-host` builds it), then the release checks, then
   makes the annotated tag `v0.2.0` with the changelog section as its message. Nothing is pushed.
4. `make publish` pushes `main` and the tag in one atomic push, then makes the GitHub release
   with the changelog section as its notes, the three files and `SHA256SUMS`.
5. Check it: `gh release view v0.2.0`.

If a check fails, nothing has left the machine: fix it, amend or add a commit, and run the step
again. A tag that was made but not pushed can be deleted with `git tag -d <tag>`. A pushed tag is
never moved; a broken release is followed by a new version. If the push went and `gh` failed, the
tag is on `origin` and the release is not: make it by hand with `gh release create v0.2.0
--verify-tag` and the files of `dist/release`.

### What the release checks refuse

- a version that is not after the last, or a `Cargo.toml` that disagrees with the newest changelog
  section;
- an `## [Unreleased]` section that still holds changes, or an empty release section;
- a branch other than `main`, a working tree that is not clean, a `main` that has diverged from
  `origin/main`;
- a tag that already exists, here or on `origin`, or a release that already exists on GitHub;
- a `dist/release` that is not exactly the three files of this version and `SHA256SUMS`, with the
  sum of each as listed, or whose executable for this machine does not say this version.

## The npm package

textpen 0.1.0 is on npm. Nothing is published there any more. At the first release of the native
file, a maintainer deprecates the old package by hand, so that whoever installs it is told where
the files are:

```sh
npm deprecate textpen "textpen is now a single native file, without Node: take it from https://github.com/bateri/textpen/releases"
```

npm asks for the second factor, so run it in a terminal. The trusted publisher set up for
`release.yml` on npmjs.com, if there is one, is removed in the package's settings: the workflow
is gone.

## Who releases

`make publish` makes the release under the project's name. Only a maintainer runs it. Everything
before it (prepare, commit, dist, tag) can be undone locally.

# Releasing

A release is three files on a GitHub release tagged `v<x.y.z>`, `textpen-<version>-<target>`
for macOS on Apple silicon and Linux on arm64 and x86_64, with a `SHA256SUMS`. There is no Intel
Mac file, because bateri runs on Apple silicon only. People install it with `install.sh` (see the
README).

Each file is pencere's Lua runner for one target with textpen packed into it: the program, the
pictures, and the editor as vite built it. `make dist` makes all three with the `pencere` command
of the version in `tools/pencere-version`, which fetches each target's runner from its own
release. To build with a newer pencere, change that file.

npm is a tool of the build only: it installs, from the lockfile, what vite builds the editor with.
Nothing is published on npm, and the version is not in `package.json`.

## Versions and the changelog

[Semantic versioning](https://semver.org/); while it is `0.x`, a minor release may change behaviour.
Every change a user would notice goes under `## [Unreleased]` in `CHANGELOG.md`, in the same commit
as the change.

## A release

1. `make release-prepare VERSION=<x.y.z>` sets the version in `src/program/version.lua` and dates
   the changelog. Commit it as `Release textpen <x.y.z>` and push it to `main`.
2. The gate (`check.yml`) runs on the push. When it is green, the release workflow
   (`release.yml`) starts.
3. If the version has no tag yet, the workflow checks the version and changelog, builds the three
   files with their sums and provenance, pushes the tag, and writes a **draft** GitHub release.
4. The run waits in the `release` environment. GitHub notifies the reviewers; one of them opens
   the run and chooses **Review deployments → Approve**. The workflow then checks the files again
   and makes the draft public as **Latest**. Rejected, nothing is published.
5. Check it: `gh release view v<x.y.z>`, and run `install.sh` on a machine.

**If something fails.** Before the tag, nothing is public: fix it and push again. A tag is never
moved; a broken release is followed by a new version. A release that stopped after its tag is
finished by running the workflow by hand on `main` (**Run workflow**, or
`gh workflow run release.yml`).

**Trying it.** `make rehearse` runs the release checks on any branch. **Run workflow** with
**rehearse** builds on any branch and publishes nothing.

### What the checks refuse

- a version that is not after the last, or a `version.lua` that disagrees with the changelog;
- a non-empty `## [Unreleased]`, or an empty release section;
- a commit whose gate is not green, a tag on another commit, or a release that is public already;
- files that are not exactly the three executables of this version with correct sums and this
  workflow's attestation.

## Settings the workflow relies on

GitHub → Settings → Environments → `release`: **Required reviewers**, and deployments from `main`
only. There is no secret or token. The publishing job refuses to run if the environment has no
reviewers, because GitHub would otherwise create it empty and publish without asking.

## The old npm package

textpen 0.1.0, the Node program, is on npm and is no longer updated. To point its users here, a
maintainer runs once (npm asks for the second factor); the publishing job reminds them until it is
done:

```sh
npm deprecate textpen "textpen is now a single native file, without Node: take it from https://github.com/bateri/textpen/releases"
```

The trusted publisher set up for the old `release.yml` on npmjs.com, if there is one, is removed in
the package's settings.

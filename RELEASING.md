# Releasing

textpen is published to npm as `textpen`, and each release is tagged `v<x.y.z>` here.
Every step a machine can check is in `tools/release.mjs`, behind the `make` targets below. This
file is the order of the steps and the reasons for them.

## Versions and the changelog

textpen follows [semantic versioning](https://semver.org/). While it is `0.x`, a minor release
may change what the previous one did.

Every change a user would notice goes under `## [Unreleased]` in `CHANGELOG.md`
([Keep a Changelog](https://keepachangelog.com/en/1.1.0/)), in the same commit as the change. A
release only moves that section under a version and a date; it does not write it.

## A release

From a clean `main`:

1. `make release-prepare VERSION=0.2.0` sets the version in `package.json` and the lockfile and
   turns `## [Unreleased]` into `## [0.2.0] - <today>` with a new, empty `## [Unreleased]`
   above it. Read the diff, then commit it as `Release textpen 0.2.0`.
2. `make release` runs the gate (`make check`), then the release checks, then makes the annotated
   tag `v0.2.0` with the changelog section as its message. Nothing is pushed.
3. `make publish` pushes `main` and the tag in one atomic push. From here the release workflow
   (`.github/workflows/release.yml`) takes over:
   - it checks that the tag names the version `package.json` and the changelog carry, on `main`;
   - it runs the gate on Linux;
   - it stages the package on npm through trusted publishing, which also records where it was
     built (provenance);
   - it writes the GitHub release from the changelog section.
4. `make approve` once the workflow has staged the version. It is not public until a maintainer
   approves it with 2FA: the target shows the staged version (who staged it, its shasum) and
   approves it, and npm asks for the second factor, so run it in a terminal. The same can be
   done on npmjs.com, under the package's **Staged Packages** tab.
5. Check it: `npm view textpen version`.

If a check fails, nothing has left the machine: fix it, amend or add a commit, and run the step
again. A tag that was made but not pushed can be deleted with `git tag -d <tag>`. A pushed tag is
never moved; a broken release is followed by a new version.

When `@pencere/sdk` has a new minor version, the range textpen depends on may not admit it:
textpen moves its range in a commit of its own, and the next release says so in its changelog.

Until the SDK that speaks the one protocol is released, textpen depends on it by a path
(`file:../pencere-protocol/sdk/typescript`), which nobody who installs the package can resolve:
the release checks refuse a version with a dependency that is not from the registry, so the
first release after the SDK's moves it to a version, in a commit of its own.

### What the release checks refuse

- a version that is not after the last, or a `package.json` that disagrees with the newest
  changelog section;
- an `## [Unreleased]` section that still holds changes, or an empty release section;
- a branch other than `main`, a working tree that is not clean, a `main` that has diverged from
  `origin/main`;
- a tag that already exists, here or on `origin`;
- a dependency that is a path and not a version from the registry;
- a package without its `LICENSE`, `README.md`, `CHANGELOG.md`, server or view, or one that
  carries its sources, tests or harness. The package is checked from `npm pack`, which builds it
  first.

## The first release

npm learns about a package from its first publish, and a trusted publisher can only be set up for
a package that exists. Staging a package npm does not know would publish a public placeholder, so
the workflow refuses to. The first release is therefore published from the maintainer's machine,
with the same steps:

1. The release commit, with the changelog's first section dated.
2. `make release`.
3. `make publish-local` runs `npm publish` (npm asks for the second factor, so run it in a
   terminal), waits until npm lists the version, then pushes `main` and the tag. The workflow
   sees that the version is already on npm, stages nothing, and writes the GitHub release.

npm processes a new package for a few minutes before it lists it, so the push waits, up to 15
minutes. If the workflow's `npm` job still finds nothing, it fails with "not on npm yet" and the
GitHub release is not written: once `npm view textpen version` answers, run what failed again
(`gh run rerun <run-id> --failed`).

Then, on npmjs.com, in the package's settings:

- **Publishing access**, once the first release is out: require two-factor authentication and
  disallow bypass tokens. It does not expire.
- **Trusted publisher**, on the day of the second release, just before `make publish`: GitHub
  Actions, organization `bateri`, repository `textpen`, workflow `release.yml`, no environment,
  with "Allow npm publish" and "Allow npm dist-tag" left off, so the workflow can only stage and
  never make a version public by itself. npm stops a trusted publisher that has not published
  within two days of being made, and one that has stopped cannot be edited: it is deleted and made
  again. So it is made when a release is about to use it, not after the first.

From then on a version becomes public only through the workflow and a maintainer's approval.

## Who releases

`make publish` and `make publish-local` publish textpen under the project's name. Only a
maintainer runs them. The approval on npm (`make approve`) is a second, separate decision.
Everything before them (prepare, commit, tag) can be undone locally.

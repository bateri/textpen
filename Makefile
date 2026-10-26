# textpen is one native file: pencere's Lua runner with the program (src/program/, Lua), the
# pictures it ships (src/assets/) and the editor (src/view/, built by vite into dist/view) inside
# it. The gate and the steps of a release are here; RELEASING.md says in what order and why.
.PHONY: setup view build check check-page test-binary test-tools dist release-prepare rehearse

# `pencere` is the command that builds textpen: a release of it, which pencere's installer puts in
# ~/.local/bin (curl -fsSL https://raw.githubusercontent.com/bateri/pencere/main/cli/install.sh | sh;
# PENCERE_VERSION=<it> for the version in tools/pencere-version, the one releases are built with),
# or one built in a pencere checkout (cargo build --release -p pencere-cli). The stand-in terminal
# the end-to-end tests play textpen in has no release: it is built in a pencere checkout (cargo
# build -p pencere-fake-host). Where they are not where the Makefile looks, say so:
#
#   make check PENCERE=../pencere/target/release/pencere \
#              FAKE_HOST=../pencere/target/debug/pencere-fake-host
PENCERE ?= $(shell command -v pencere 2>/dev/null || echo ../pencere/target/release/pencere)
FAKE_HOST ?= ../pencere/target/debug/pencere-fake-host

# setup installs what builds the editor and runs the tests: npm is a tool of the build here,
# nothing of it is in what textpen ships.
setup:
	npm ci

# view builds the editor, the page the program shows, into dist/view.
view:
	npm run build

# build makes dist/textpen, the program for this machine, as one file: src/program/ and
# src/assets/, with the view as vite built it (--web) in place of src/view/, which is its source.
build: view
	@test -x "$(PENCERE)" || { echo "build: the pencere command is not at $(PENCERE): install it (see PENCERE at the top of the Makefile), then make build PENCERE=<it>"; exit 1; }
	"$(PENCERE)" build --web dist/view --name textpen -o dist/textpen

# check-page is the part of the gate that needs no pencere: types, formatting and the tests of
# the page's side (its end of the protocol, its edits, its quick open).
check-page:
	npm run typecheck
	npx prettier --check .
	npm test

# test-binary runs the program, dist/textpen, in a pane under the stand-in terminal: the calls
# of the view, the drafts, the shell and the command line. Nothing in it is skipped; a program or
# a stand-in that is not there stops the run.
test-binary:
	@test -x "$(FAKE_HOST)" || { echo "test-binary: pencere-fake-host is not at $(FAKE_HOST): build it in a pencere checkout (cargo build -p pencere-fake-host), then make check FAKE_HOST=<it>"; exit 1; }
	PENCERE_FAKE_HOST="$(FAKE_HOST)" TEXTPEN_BIN=dist/textpen node --test test/e2e.test.ts test/calls.test.ts

# test-tools runs the tests of the installer (install.sh) against a stand-in for GitHub.
test-tools:
	node --test test/install.test.mjs

# check is the gate: everything check-page does, the installer's tests, then the program built
# and held to what the view expects of it.
check: check-page test-tools build test-binary

# dist makes the program for the three targets textpen ships (Apple silicon Macs and both Linux
# architectures; bateri, the terminal that shows the editor, runs on Apple silicon only) into
# dist/release, with the SHA256SUMS that names them. `pencere build --target` fetches the runner of
# each target from the release of the `pencere` command's own version and checks it against that
# release's sums, so no target's toolchain is needed here. The release workflow runs it.
TARGETS = aarch64-apple-darwin aarch64-unknown-linux-musl x86_64-unknown-linux-musl
PACKAGE_VERSION = $(shell node tools/release.mjs version)

dist: view
	@test -x "$(PENCERE)" || { echo "dist: the pencere command is not at $(PENCERE): see PENCERE at the top of the Makefile"; exit 1; }
	rm -rf dist/release
	mkdir -p dist/release
	@for target in $(TARGETS); do \
		"$(PENCERE)" build --web dist/view --name textpen --target $$target -o dist/release/textpen-$(PACKAGE_VERSION)-$$target || exit 1; \
	done
	cd dist/release && { sha256sum textpen-* 2>/dev/null || shasum -a 256 textpen-*; } > SHA256SUMS
	@cat dist/release/SHA256SUMS
	node tools/release.mjs dist dist/release

# release-prepare sets the version and dates the changelog's Unreleased section.
release-prepare:
	@test -n "$(VERSION)" || { echo "usage: make release-prepare VERSION=<x.y.z>"; exit 2; }
	node tools/release.mjs prepare $(VERSION)

# rehearse runs the checks of a release of the prepared version, on any branch. It changes nothing:
# a release is the commit of release-prepare pushed to main, which the release workflow tags,
# builds and drafts once the gate is green, and publishes when a maintainer approves (RELEASING.md).
rehearse:
	node tools/release.mjs rehearse

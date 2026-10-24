# textpen is one native file: pencere's Lua runner with the program (src/program/, Lua), the
# pictures it ships (src/assets/) and the editor (src/view/, built by vite into dist/view) inside
# it. The gate and the steps of a release are here; RELEASING.md says in what order and why.
.PHONY: setup view build check check-page test-binary dist release-prepare release publish

# `pencere` is the command of a pencere checkout, built there (cargo build --release -p
# pencere-cli); the stand-in terminal the end-to-end tests play textpen in is built there too
# (cargo build -p pencere-fake-host). Until the checkout beside this one has the Lua runner, both
# are in ../pencere-lua:
#
#   make check PENCERE=../pencere-lua/target/release/pencere \
#              FAKE_HOST=../pencere-lua/target/debug/pencere-fake-host
PENCERE ?= ../pencere/target/release/pencere
FAKE_HOST ?= ../pencere/target/debug/pencere-fake-host

setup:
	npm ci

# view builds the editor, the page the program shows, into dist/view.
view:
	npm run build

# build makes dist/textpen, the program for this machine, as one file: src/program/ and
# src/assets/, with the view as vite built it (--web) in place of src/view/, which is its source.
build: view
	@test -x "$(PENCERE)" || { echo "build: the pencere command is not at $(PENCERE): build it in a pencere checkout (cargo build --release -p pencere-cli), then make build PENCERE=<it>"; exit 1; }
	"$(PENCERE)" build --web dist/view --name textpen -o dist/textpen

# check-page is the part of the gate that needs no pencere checkout: types, formatting and the
# tests of the page's side (its end of the protocol, its edits, its quick open). CI runs this and
# `make view`, and says that it does not run the rest.
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

# check is the gate: everything check-page does, then the program built and held to what the
# view expects of it.
check: check-page build test-binary

# dist makes the program for the three targets textpen ships (Apple silicon Macs and both Linux
# architectures; bateri, the terminal that shows the editor, runs on Apple silicon only) into
# dist/release, with the
# SHA256SUMS that names them. The runners come from a pencere checkout's `make dist`
# (RUNNERS=<its dist/>), one for each target, of the version of the `pencere` command. When
# pencere has a release, `pencere build --target T` fetches them itself and --runner goes away.
TARGETS = aarch64-apple-darwin aarch64-unknown-linux-musl x86_64-unknown-linux-musl
RUNNERS ?= ../pencere/dist
PACKAGE_VERSION = $(shell node -p "require('./package.json').version")
RUNNER_VERSION = $(shell "$(PENCERE)" --version | sed 's/^pencere //')

dist: view
	@test -x "$(PENCERE)" || { echo "dist: the pencere command is not at $(PENCERE): see PENCERE at the top of the Makefile"; exit 1; }
	rm -rf dist/release
	mkdir -p dist/release
	@for target in $(TARGETS); do \
		runner="$(RUNNERS)/pencere-$(RUNNER_VERSION)-$$target"; \
		test -f "$$runner" || { echo "dist: no runner for $$target at $$runner: make dist in a pencere checkout, then make dist RUNNERS=<its dist/>"; exit 1; }; \
		"$(PENCERE)" build --web dist/view --name textpen --target $$target --runner "$$runner" -o dist/release/textpen-$(PACKAGE_VERSION)-$$target || exit 1; \
	done
	cd dist/release && shasum -a 256 textpen-* > SHA256SUMS
	@cat dist/release/SHA256SUMS

# release-prepare sets the version and dates the changelog's Unreleased section.
release-prepare:
	@test -n "$(VERSION)" || { echo "usage: make release-prepare VERSION=<x.y.z>"; exit 2; }
	node tools/release.mjs prepare $(VERSION)

# release runs the gate and the release checks (dist/release holds the three files of this
# version, made after the release commit), then tags. Nothing is pushed.
release: check
	node tools/release.mjs tag

# publish pushes main and the tag, then makes the GitHub release with the three files and
# SHA256SUMS. Publishing: run on the maintainer's word.
publish:
	node tools/release.mjs publish

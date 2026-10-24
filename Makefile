# The gate and the steps of a release; RELEASING.md says in what order and why.
.PHONY: setup check release-prepare release publish publish-local approve

setup:
	npm ci

# check is the gate: types, formatting, the build, and the tests against what it built: the
# program, the view's edits and its end of the protocol, and textpen in a pane under
# pencere-fake-host when a pencere checkout beside this one has built it.
check:
	npm run typecheck
	npx prettier --check .
	npm test

# release-prepare sets the version and dates the changelog's Unreleased section.
release-prepare:
	@test -n "$(VERSION)" || { echo "usage: make release-prepare VERSION=<x.y.z>"; exit 2; }
	node tools/release.mjs prepare $(VERSION)

# release runs the gate and the release checks, then tags. Nothing is pushed.
release: check
	node tools/release.mjs tag

# publish pushes main and the tag; the release workflow stages the package and a
# maintainer approves it on npm with 2FA. Publishing: run on the maintainer's word.
publish:
	node tools/release.mjs push

# publish-local publishes from this machine, then pushes: the first release, before the
# registry knows the package and its trusted publisher.
publish-local:
	node tools/release.mjs local

# approve makes public the version the release workflow staged on npm, with 2FA.
# Publishing: run on the maintainer's word.
approve:
	node tools/release.mjs approve

# lua builds textpen's program in Lua, on pencere's Lua runner, as one file: the runner, the
# program (lua/) and the view (dist/view) in dist/textpen-lua. `pencere` is the command of a pencere
# checkout with the Lua runner (cargo build --release -p pencere-cli there; PENCERE=… says where).
PENCERE ?= pencere
.PHONY: lua
lua:
	npm run build:view
	rm -rf lua/web
	cp -R dist/view lua/web
	$(PENCERE) build lua -o dist/textpen-lua

# check-lua runs the end-to-end and parity tests on the program of dist/textpen-lua, in a pane
# under pencere-fake-host (built in the pencere checkout: cargo build -p pencere-fake-host).
FAKE_HOST ?= ../pencere/target/debug/pencere-fake-host
.PHONY: check-lua
check-lua: lua
	PENCERE_FAKE_HOST=$(FAKE_HOST) TEXTPEN_BIN=dist/textpen-lua node --test test/e2e.test.ts test/parity.test.ts

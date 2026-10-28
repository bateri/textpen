# textpen is one native file: the program (src/program/, Rust, on the pencere crate) with the page
# (src/view/, a page of pencere's kit) and the pictures (src/assets/) carried in it by build.rs. The
# gate, `make dev` and the steps of a release are here; RELEASING.md says in what order and why.
# Nothing here needs Node but the release steps, which run tools/release.mjs.
.PHONY: pin fmt lint test test-page shots check build pencere-src pencere fake-host pencere-ref dev dist sums release-prepare release publish

# pencere is pinned in one place, Cargo.lock (Cargo.toml names the commit): the crate the program is
# built on is of it, and so are the `pencere` command `make dev` runs textpen under and the stand-in
# terminal the program's tests run it in, which are built from it in .cache/pencere. To work against
# a checkout you are changing, say so; one that is not at Cargo.lock's ref is said to be so, and used:
#
#   make dev PENCERE_SRC=../pencere
PENCERE_REF = $(shell sed -n 's|^source = "git+https://github.com/bateri/pencere?rev=[0-9a-f]*\#\([0-9a-f]\{40\}\)"$$|\1|p' Cargo.lock | head -n 1)
PENCERE_CACHE = .cache/pencere
PENCERE_SRC ?= $(PENCERE_CACHE)
PENCERE ?= $(PENCERE_SRC)/target/release/pencere
PENCERE_GIT = https://github.com/bateri/pencere

# The stand-in terminal the program's tests run textpen in (FAKE_HOST): by default the one built in
# the pencere checkout beside this one. `make fake-host` builds one at Cargo.lock's pencere instead:
#
#   make fake-host && make check FAKE_HOST=.cache/pencere/target/debug/pencere-fake-host
FAKE_HOST ?= ../pencere/target/debug/pencere-fake-host

# pin: main is built on a pencere anyone can fetch, the commit Cargo.lock names. A branch that waits
# for a pencere not pushed yet takes it from a checkout on this machine with a [patch] of Cargo.toml;
# that section is refused on main and on what is to be merged into it.
pin:
	@if grep -q '^\[patch' Cargo.toml; then \
		branch="$$(git rev-parse --abbrev-ref HEAD 2>/dev/null)"; \
		if [ "$$branch" = main ] || [ "$${GITHUB_REF:-}" = refs/heads/main ] || [ "$${GITHUB_BASE_REF:-}" = main ]; then \
			echo "pin: Cargo.toml takes pencere from a [patch]: main is built on a pushed pencere (move the rev in Cargo.toml, drop the patch)"; exit 1; \
		fi; \
		echo "pin: Cargo.toml takes pencere from a [patch] ($$branch): main will refuse it"; \
	fi

fmt:
	cargo fmt

lint:
	cargo fmt -- --check
	cargo clippy --locked --all-targets -- -D warnings

# The program's tests: its own, and the ones that run it in a pane under the stand-in terminal. Nothing
# in them is skipped; a stand-in that is not there stops the run and says which.
test:
	FAKE_HOST="$(FAKE_HOST)" cargo test --locked

# The page's tests: src/view/ run as a terminal runs it, on pencere's host with no screen
# (pencere-drive), with the program's own handlers in the same process (src/program/page_tests.rs).
# Each compiles and runs the kit's interpreter, so they are `cargo test`'s ignored ones, run here; a
# few at a time, for each call of the page is held to the kit's 250 ms as a terminal holds it.
test-page:
	cargo test --locked --bin textpen page_tests:: -- --ignored --test-threads=2

# shots draws the pictures of docs/ from the page as it is, in the fonts of this machine (macOS: SF
# Mono, or Menlo, and SF): not part of the gate, which needs no font of the system.
shots:
	cargo test --locked --bin textpen page_shots:: -- --ignored

# check is the gate: the pin of pencere, formatting, lints, the program's tests, the page's tests.
check: pin lint test test-page

# build makes target/release/textpen, the program for this machine, with the page in it.
build:
	cargo build --locked --release

# pencere-src makes PENCERE_SRC ready: fetched at Cargo.lock's ref when it is the cache (one checkout,
# moved to another ref when Cargo.lock moves, so that cargo builds again only what changed).
pencere-src:
	@command -v cargo >/dev/null || { echo "pencere: cargo is not installed: pencere is built from its source (https://rustup.rs)"; exit 1; }
	@ref="$(PENCERE_REF)"; test -n "$$ref" || { echo "pencere: Cargo.lock takes no pencere from $(PENCERE_GIT) at a commit"; exit 1; }; \
	if [ "$(PENCERE_SRC)" = "$(PENCERE_CACHE)" ]; then \
		if [ "$$(cat $(PENCERE_CACHE).ref 2>/dev/null)" != "$$ref" ]; then \
			echo "pencere: fetching $(PENCERE_GIT) at $$ref into $(PENCERE_CACHE)"; \
			{ test -d "$(PENCERE_CACHE)/.git" || git init -q "$(PENCERE_CACHE)"; } && \
			git -C "$(PENCERE_CACHE)" fetch -q --depth 1 "$(PENCERE_GIT)" "$$ref" && \
			git -C "$(PENCERE_CACHE)" checkout -q --detach --force FETCH_HEAD && \
			echo "$$ref" > "$(PENCERE_CACHE).ref" || exit 1; \
		fi; \
	else \
		head=$$(git -C "$(PENCERE_SRC)" rev-parse HEAD 2>/dev/null); \
		[ "$$head" = "$$ref" ] || \
			echo "pencere: $(PENCERE_SRC) is at $${head:-no commit}, not at $$ref (Cargo.lock) that textpen is built with"; \
	fi

# pencere builds the `pencere` command in PENCERE_SRC.
pencere: pencere-src
	cd "$(PENCERE_SRC)" && cargo build --release -p pencere-cli

# fake-host builds the stand-in terminal in PENCERE_SRC.
fake-host: pencere-src
	cd "$(PENCERE_SRC)" && cargo build -p pencere-fake-host

# pencere-ref says which pencere Cargo.lock takes.
pencere-ref:
	@echo "$(PENCERE_REF)"

# dev runs textpen from its files in this pane, in a terminal that hosts pencere (bateri): the page
# is shown again when a file of src/view/ or src/assets/ changes, and the program built and run again
# when one of it does. It opens on this directory as its project.
dev: pencere
	"$(PENCERE)" dev

# dist makes the program for one target (TARGET, a Rust target triple) into dist/release, as
# textpen-<version>-<target>: Apple silicon Macs and both Linux architectures, the Linux ones linked
# statically with musl (bateri, the terminal that shows the editor, runs on Apple silicon only).
# Each is built on a machine of its own target, with that target added to the toolchain; `make sums`
# then names the three where they are together.
TARGETS = aarch64-apple-darwin aarch64-unknown-linux-musl x86_64-unknown-linux-musl
PACKAGE_VERSION = $(shell sed -n 's/^version = "\(.*\)"$$/\1/p' Cargo.toml | head -n 1)

dist:
	@test -n "$(TARGET)" || { echo "usage: make dist TARGET=<one of: $(TARGETS)>"; exit 2; }
	cargo build --locked --release --target $(TARGET)
	mkdir -p dist/release
	cp target/$(TARGET)/release/textpen dist/release/textpen-$(PACKAGE_VERSION)-$(TARGET)

# sums writes the SHA256SUMS of the three files of dist/release.
sums:
	cd dist/release && { sha256sum textpen-* 2>/dev/null || shasum -a 256 textpen-*; } > SHA256SUMS
	@cat dist/release/SHA256SUMS

# release-prepare sets the version and dates the changelog's Unreleased section.
release-prepare:
	@test -n "$(VERSION)" || { echo "usage: make release-prepare VERSION=<x.y.z>"; exit 2; }
	node tools/release.mjs prepare $(VERSION)

# release runs the gate and the release checks (dist/release holds the three files of this
# version and their SHA256SUMS, made after the release commit), then tags. Nothing is pushed.
release: check
	node tools/release.mjs tag

# publish pushes main and the tag, then makes the GitHub release with the three files and
# SHA256SUMS. Publishing: run on the maintainer's word.
publish:
	node tools/release.mjs publish

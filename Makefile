# textpen is one native file: the program (src/program/, Rust, on the pencere crate) with the page
# (src/view/) and the pictures (src/assets/) carried in it by build.rs. The gate and the steps of a
# release are here; RELEASING.md says in what order and why.
.PHONY: setup view check-page fmt lint test build check dist sums release-prepare release publish

setup:
	npm ci

# view builds the web editor of the earlier textpen into dist/view; the program does not carry it.
view:
	npm run build

# check-page is the part of the gate that needs neither Rust nor a pencere checkout: the types,
# formatting and tests of the page's side that are still in TypeScript. CI runs this.
check-page:
	npm run typecheck
	npx prettier --check .
	npm test

fmt:
	cargo fmt

lint:
	cargo fmt -- --check
	cargo clippy --locked --all-targets -- -D warnings

# test runs the program's tests: its own, and the ones that run it in a pane under pencere's
# stand-in terminal, which is built in a pencere checkout (cargo build -p pencere-fake-host). Nothing
# in them is skipped; a stand-in that is not there stops the run.
FAKE_HOST ?= ../pencere/target/debug/pencere-fake-host

test:
	FAKE_HOST="$(FAKE_HOST)" cargo test --locked

# build makes target/release/textpen, the program for this machine, with the page in it.
build:
	cargo build --locked --release

# check is the gate: the page's checks, then the program's lints and tests.
check: check-page lint test

# dist makes the program for one target (TARGET, a Rust target triple) into dist/release, as
# textpen-<version>-<target>: Apple silicon Macs and both Linux architectures, the Linux ones linked
# statically with musl (bateri, the terminal that shows the editor, runs on Apple silicon only).
# Each is built on a machine of its own target; `make sums` then names the three where they are
# together.
TARGETS = aarch64-apple-darwin aarch64-unknown-linux-musl x86_64-unknown-linux-musl
PACKAGE_VERSION = $(shell sed -n 's/^version = "\(.*\)"$$/\1/p' Cargo.toml | head -n 1)

dist:
	@test -n "$(TARGET)" || { echo "usage: make dist TARGET=<one of: $(TARGETS)>"; exit 2; }
	cargo build --locked --release --target $(TARGET)
	mkdir -p dist/release
	cp target/$(TARGET)/release/textpen dist/release/textpen-$(PACKAGE_VERSION)-$(TARGET)

# sums writes the SHA256SUMS of the files of dist/release.
sums:
	cd dist/release && { sha256sum textpen-* 2>/dev/null || shasum -a 256 textpen-*; } > SHA256SUMS
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

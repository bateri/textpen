// Releases textpen: one script for every step a machine can check, so a release is
// the same sequence every time. RELEASING.md is the procedure, this file the checks.
//
//   prepare <version>   set the version, move Unreleased into a dated section of CHANGELOG.md
//   check               everything a release needs, without changing anything
//   tag                 check, then create the annotated tag v<version>
//   publish             push main and the tag, then make the GitHub release with the four files
//   notes [<version>]   the CHANGELOG section of a version
//
// It uses nothing but Node, git and gh.

import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { existsSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const NAME = "textpen";

/** The systems textpen is built for: the targets of pencere's runner (the Makefile's TARGETS). */
const TARGETS = [
  "aarch64-apple-darwin",
  "x86_64-apple-darwin",
  "aarch64-unknown-linux-musl",
  "x86_64-unknown-linux-musl",
];

/** Where `make dist` puts the files of a release. */
const RELEASE_DIR = join(root, "dist", "release");

const SEMVER = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-([0-9A-Za-z.-]+))?$/;
const RELEASED = /^## \[(\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?)\] - (\d{4}-\d{2}-\d{2})$/;

class Refusal extends Error {}
const refuse = (what) => {
  throw new Refusal(what);
};

/** Runs a command and returns its output; one run with `stdio: "inherit"` returns "". */
function run(command, args, options = {}) {
  const output = execFileSync(command, args, {
    cwd: root,
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"],
    ...options,
  });
  return (output ?? "").trim();
}

function succeeds(command, args, options = {}) {
  try {
    run(command, args, options);
    return true;
  } catch {
    return false;
  }
}

const tagOf = (version) => `v${version}`;

// ---- the manifest's version ------------------------------------------------

function versionOf() {
  return JSON.parse(readFileSync(join(root, "package.json"), "utf8")).version;
}

function setVersion(version) {
  const path = join(root, "package.json");
  const manifest = JSON.parse(readFileSync(path, "utf8"));
  manifest.version = version;
  writeFileSync(path, `${JSON.stringify(manifest, null, 2)}\n`);
  // The lockfile records the package's own version twice: at its top and as its root package.
  const lockPath = join(root, "package-lock.json");
  const lock = JSON.parse(readFileSync(lockPath, "utf8"));
  lock.version = version;
  if (lock.packages?.[""] === undefined) refuse("package-lock.json has no root package");
  lock.packages[""].version = version;
  writeFileSync(lockPath, `${JSON.stringify(lock, null, 2)}\n`);
  // The program is one file with no manifest beside it: it carries its version in its own source.
  writeFileSync(
    LUA_VERSION,
    `-- The version, as package.json has it. The program is one file with no manifest beside it,\n-- so a release changes it here too (tools/release.mjs keeps the two the same).\nreturn "${version}"\n`,
  );
}

/** The version the program says of itself. */
const LUA_VERSION = join(root, "lua", "lib", "version.lua");

function programVersion() {
  return /return "([^"]+)"/.exec(readFileSync(LUA_VERSION, "utf8"))?.[1];
}

// ---- the changelog ---------------------------------------------------------

const changelogPath = join(root, "CHANGELOG.md");

/** The changelog as its heading lines and the sections below them. */
function sections() {
  const lines = readFileSync(changelogPath, "utf8").split("\n");
  const found = [];
  lines.forEach((text, index) => {
    if (text.startsWith("## ")) found.push({ heading: text, start: index });
  });
  return found.map((section, i) => ({
    ...section,
    end: i + 1 < found.length ? found[i + 1].start : lines.length,
    body: lines
      .slice(section.start + 1, i + 1 < found.length ? found[i + 1].start : lines.length)
      .join("\n")
      .trim(),
  }));
}

function unreleased() {
  const first = sections()[0];
  if (first === undefined || first.heading !== "## [Unreleased]") {
    refuse(`CHANGELOG.md: the first section must be "## [Unreleased]"`);
  }
  return first;
}

function releasedSection(version) {
  const section = sections().find((s) => RELEASED.exec(s.heading)?.[1] === version);
  if (section === undefined) refuse(`CHANGELOG.md: no "## [${version}] - YYYY-MM-DD" section`);
  if (section.body === "") refuse(`CHANGELOG.md: the section of ${version} is empty`);
  return section;
}

function compare(a, b) {
  const pa = SEMVER.exec(a);
  const pb = SEMVER.exec(b);
  for (let i = 1; i <= 3; i++) {
    const d = Number(pa[i]) - Number(pb[i]);
    if (d !== 0) return d;
  }
  // A pre-release sorts before the release it leads to.
  if (pa[4] === undefined) return pb[4] === undefined ? 0 : 1;
  if (pb[4] === undefined) return -1;
  return pa[4] < pb[4] ? -1 : pa[4] > pb[4] ? 1 : 0;
}

// ---- commands ----------------------------------------------------------------

function prepare(version) {
  if (!SEMVER.test(version ?? "")) refuse(`"${version}" is not a version (x.y.z)`);
  const current = versionOf();
  if (compare(version, current) <= 0) refuse(`${version} is not after ${current}`);
  const top = unreleased();
  if (top.body === "") refuse("CHANGELOG.md: Unreleased is empty, nothing to release");
  if (sections().some((s) => RELEASED.exec(s.heading)?.[1] === version)) {
    refuse(`CHANGELOG.md already has a section for ${version}`);
  }
  const date = new Date().toISOString().slice(0, 10);
  const lines = readFileSync(changelogPath, "utf8").split("\n");
  lines.splice(top.start, 1, "## [Unreleased]", "", `## [${version}] - ${date}`);
  writeFileSync(changelogPath, lines.join("\n"));
  setVersion(version);
  console.log(`${NAME} ${current} -> ${version}`);
  console.log(`next: review, commit ("Release ${NAME} ${version}"), then make release`);
}

function check() {
  const version = versionOf();
  const tag = tagOf(version);

  if (!SEMVER.test(version ?? "") || version === "0.0.0") {
    refuse(`package.json is at "${version}": run make release-prepare first`);
  }
  if (programVersion() !== version) {
    refuse(
      `lua/lib/version.lua says ${programVersion()}, package.json ${version}: the program would report the wrong one`,
    );
  }
  if (unreleased().body !== "") {
    refuse("CHANGELOG.md: Unreleased is not empty; prepare moves it into the release");
  }
  const section = sections().find((s) => RELEASED.test(s.heading));
  if (section === undefined || RELEASED.exec(section.heading)[1] !== version) {
    refuse(`CHANGELOG.md: the newest section is not ${version}`);
  }
  releasedSection(version);

  if (run("git", ["branch", "--show-current"]) !== "main") refuse("not on main");
  if (run("git", ["status", "--porcelain"]) !== "") refuse("the working tree is not clean");
  run("git", ["fetch", "--quiet", "origin", "main", "--tags"]);
  if (!succeeds("git", ["merge-base", "--is-ancestor", "origin/main", "HEAD"])) {
    refuse("main has diverged from origin/main");
  }
  if (succeeds("git", ["rev-parse", "--verify", "--quiet", `refs/tags/${tag}`])) {
    refuse(`the tag ${tag} exists`);
  }
  const released = run("git", ["tag", "--list", "v*"])
    .split("\n")
    .map((name) => name.slice(1))
    .filter((v) => SEMVER.test(v));
  const last = released.sort(compare).at(-1);
  if (last !== undefined && compare(version, last) <= 0) {
    refuse(`${version} is not after ${last}, the last release`);
  }
  if (run("git", ["ls-remote", "--tags", "origin", `refs/tags/${tag}`]) !== "") {
    refuse(`the tag ${tag} exists on origin`);
  }

  const files = releaseFiles(version);
  console.log(
    `${NAME} ${version} is ready to tag as ${tag}: ${files.length} files in dist/release`,
  );
  return { version, tag, section, files };
}

/** The executable of a release for a target. */
const fileOf = (version, target) => `${NAME}-${version}-${target}`;

/** The target of the machine this runs on, for the one executable it can run. */
function ownTarget() {
  const arch = { arm64: "aarch64", x64: "x86_64" }[process.arch];
  if (arch === undefined) return undefined;
  if (process.platform === "darwin") return `${arch}-apple-darwin`;
  if (process.platform === "linux") return `${arch}-unknown-linux-musl`;
  return undefined;
}

/**
 * The files `make dist` made for this version: the four executables and the SHA256SUMS that names
 * exactly them, each with the sum it lists. The names carry the version, so a dist/release made
 * before the version moved is refused; the one executable this machine can run says its version.
 */
function releaseFiles(version) {
  if (!existsSync(RELEASE_DIR)) refuse(`no dist/release: make dist, after the release commit`);
  const expected = TARGETS.map((target) => fileOf(version, target));
  const found = readdirSync(RELEASE_DIR).sort();
  const wanted = [...expected, "SHA256SUMS"].sort();
  if (found.join("\n") !== wanted.join("\n")) {
    refuse(
      `dist/release holds ${found.join(", ") || "nothing"} and a release of ${version} is ${wanted.join(", ")}: make dist again, after the release commit`,
    );
  }
  const listed = new Map(
    readFileSync(join(RELEASE_DIR, "SHA256SUMS"), "utf8")
      .split("\n")
      .filter((line) => line.trim() !== "")
      .map((line) => {
        const [sum, name] = line.split(/\s+/);
        return [name.replace(/^\*/, ""), sum];
      }),
  );
  if ([...listed.keys()].sort().join("\n") !== [...expected].sort().join("\n")) {
    refuse("dist/release/SHA256SUMS does not name exactly the four executables");
  }
  for (const name of expected) {
    const sum = createHash("sha256")
      .update(readFileSync(join(RELEASE_DIR, name)))
      .digest("hex");
    if (listed.get(name) !== sum)
      refuse(`${name} is not the file SHA256SUMS lists: make dist again`);
  }
  const own = ownTarget();
  if (own !== undefined) {
    const said = run(join(RELEASE_DIR, fileOf(version, own)), ["--version"]);
    if (said !== version) refuse(`${fileOf(version, own)} says it is ${said}: make dist again`);
  }
  return [...expected, "SHA256SUMS"];
}

function tag() {
  const { tag: tagName, section, version } = check();
  // Verbatim: the default cleanup would take the section's "### Added" lines for comments.
  const message = `${NAME} ${version}\n\n${section.body}\n`;
  run("git", ["tag", "--annotate", "--cleanup=verbatim", tagName, "--message", message]);
  console.log(`tagged ${tagName}; nothing is pushed. Next: make publish`);
}

function currentTag() {
  const version = versionOf();
  const tagName = tagOf(version);
  if (!succeeds("git", ["rev-parse", "--verify", "--quiet", `refs/tags/${tagName}`])) {
    refuse(`no tag ${tagName}: run make release first`);
  }
  if (run("git", ["rev-list", "-n", "1", tagName]) !== run("git", ["rev-parse", "HEAD"])) {
    refuse(`${tagName} is not on HEAD`);
  }
  return { version, tagName };
}

function publish() {
  const { version, tagName } = currentTag();
  const files = releaseFiles(version).map((name) => join(RELEASE_DIR, name));
  const notes = releasedSection(version).body;
  if (succeeds("gh", ["release", "view", tagName])) {
    refuse(`the release ${tagName} exists on GitHub`);
  }
  run("git", ["push", "--atomic", "origin", "main", tagName], { stdio: "inherit" });
  // `--verify-tag`: the release is made on the tag just pushed, never on a tag gh would make.
  // The notes go in on the standard input, the changelog's section as it is.
  execFileSync(
    "gh",
    [
      "release",
      "create",
      tagName,
      "--verify-tag",
      "--title",
      tagName,
      "--notes-file",
      "-",
      ...files,
    ],
    { cwd: root, input: notes, stdio: ["pipe", "inherit", "inherit"] },
  );
  console.log(`published ${tagName}; check it with: gh release view ${tagName}`);
}

function notes(version) {
  console.log(releasedSection(version ?? versionOf()).body);
}

const COMMANDS = { prepare, check, tag, publish, notes };

const [command, ...args] = process.argv.slice(2);
const handler = COMMANDS[command];
if (handler === undefined) {
  console.error(`release: commands are ${Object.keys(COMMANDS).join(", ")}`);
  process.exit(2);
}
try {
  handler(...args);
} catch (error) {
  if (error instanceof Refusal) {
    console.error(`release: ${error.message}`);
    process.exit(1);
  }
  throw error;
}

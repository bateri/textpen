// Releases textpen: one script for every step a machine can check, so a release is
// the same sequence every time. RELEASING.md is the procedure, this file the checks.
//
//   prepare <version>   set the version, move Unreleased into a dated section of CHANGELOG.md
//   check               everything a release needs, without changing anything
//   tag                 check, then create the annotated tag v<version>
//   push                push main and the tag together (what starts the release workflow)
//   local               publish from this machine, then push (the first release)
//   approve             make public the version the workflow staged on npm
//   verify-tag <tag>    in the workflow: the tag names the manifest's version, on main
//   registry <version>  in the workflow: is the package, and this version, on npm
//   notes [<version>]   the CHANGELOG section of a version
//
// It uses nothing but Node, git and npm.

import { execFileSync } from "node:child_process";
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const NAME = "textpen";

/** The files the package must carry: what it runs, and what a person reads before running it. */
const REQUIRED = [
  "package.json",
  "LICENSE",
  "README.md",
  "CHANGELOG.md",
  "dist/server/main.js",
  "dist/view/index.html",
];

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
  if (unreleased().body !== "") {
    refuse("CHANGELOG.md: Unreleased is not empty; prepare moves it into the release");
  }
  const section = sections().find((s) => RELEASED.test(s.heading));
  if (section === undefined || RELEASED.exec(section.heading)[1] !== version) {
    refuse(`CHANGELOG.md: the newest section is not ${version}`);
  }
  releasedSection(version);

  // A dependency that is a path on this machine does not resolve for anyone who installs the package.
  const manifest = JSON.parse(readFileSync(join(root, "package.json"), "utf8"));
  for (const [name, range] of Object.entries(manifest.dependencies ?? {})) {
    if (/^(file|link|portal):/.test(range))
      refuse(`${name} is "${range}": a release needs a version from the registry`);
  }

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

  const files = contents();
  for (const file of REQUIRED) {
    if (!files.includes(file)) refuse(`the package lacks ${file}`);
  }
  const stray = files.filter(
    (file) => file.startsWith("test/") || file.startsWith("src/") || file.startsWith("harness/"),
  );
  if (stray.length > 0) refuse(`the package carries what it does not run: ${stray.join(", ")}`);
  console.log(`${NAME} ${version} is ready to tag as ${tag}`);
  return { version, tag, section };
}

/** The files npm would publish, as paths inside the package. */
function contents() {
  // The build `prepack` runs before a publish, run here on its own: run by `npm pack`, its output
  // would land in the middle of the report.
  run("npm", ["run", "build"]);
  const report = JSON.parse(run("npm", ["pack", "--dry-run", "--json", "--ignore-scripts"]));
  return report[0].files.map((file) => file.path);
}

function tag() {
  const { tag: tagName, section, version } = check();
  // Verbatim: the default cleanup would take the section's "### Added" lines for comments.
  const message = `${NAME} ${version}\n\n${section.body}\n`;
  run("git", ["tag", "--annotate", "--cleanup=verbatim", tagName, "--message", message]);
  // The workflow cannot stage a package the registry does not know yet.
  const next = succeeds("npm", ["view", NAME, "name"]) ? "publish" : "publish-local";
  console.log(`tagged ${tagName}; nothing is pushed. Next: make ${next}`);
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

function push() {
  const { tagName } = currentTag();
  run("git", ["push", "--atomic", "origin", "main", tagName], { stdio: "inherit" });
  console.log(`pushed main and ${tagName}; the release workflow takes it from here`);
}

/** How long `local` waits for npm to list what it published, and how often it asks. */
const LISTING = { minutes: 15, every: 15_000 };

function local() {
  const { version, tagName } = currentTag();
  run("npm", ["publish"], { stdio: "inherit" });
  // npm processes a new package for minutes before it lists it, and the workflow the push
  // starts asks npm whether the version is there: one that asks too early fails.
  console.log(`waiting for npm to list ${NAME} ${version} before pushing`);
  const deadline = Date.now() + LISTING.minutes * 60_000;
  while (!succeeds("npm", ["view", `${NAME}@${version}`, "version"])) {
    if (Date.now() > deadline) {
      refuse(
        `npm has not listed ${NAME} ${version} in ${LISTING.minutes} minutes; once it does: git push --atomic origin main ${tagName}`,
      );
    }
    Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, LISTING.every);
  }
  run("git", ["push", "--atomic", "origin", "main", tagName], { stdio: "inherit" });
  console.log(`published ${NAME} ${version} and pushed ${tagName}`);
}

/** `npm stage` needs npm 11.15 or later and Node 22 ships npm 10; the release workflow installs the same one. */
const STAGE_NPM = ["--yes", "npm@11.21.1", "stage"];

function approve() {
  const version = versionOf();
  const staged = JSON.parse(run("npx", [...STAGE_NPM, "list", NAME, "--json"])).filter(
    (item) => item.version === version,
  );
  if (staged.length === 0) {
    refuse(`npm holds no staged ${NAME} ${version}: has the release workflow staged it?`);
  }
  if (staged.length > 1) {
    const ids = staged.map((item) => `${item.id} (${item.status})`).join(", ");
    refuse(`npm holds more than one staged ${NAME} ${version}: ${ids}`);
  }
  // What is approved is shown first: who staged it and the shasum of what becomes public.
  run("npx", [...STAGE_NPM, "view", staged[0].id], { stdio: "inherit" });
  run("npx", [...STAGE_NPM, "approve", staged[0].id], { stdio: "inherit" });
}

function verifyTag(tagName) {
  const match = /^v(\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?)$/.exec(tagName ?? "");
  if (match === null) refuse(`"${tagName}" is not a release tag (v<version>)`);
  const version = match[1];
  if (versionOf() !== version)
    refuse(`${tagName} names ${version} but package.json is at ${versionOf()}`);
  releasedSection(version);
  run("git", ["fetch", "--quiet", "origin", "main"]);
  if (!succeeds("git", ["merge-base", "--is-ancestor", tagName, "origin/main"])) {
    refuse(`${tagName} is not on main`);
  }
  // Read by the workflow from its output file.
  console.log(`version=${version}`);
}

function registry(version) {
  const exists = succeeds("npm", ["view", NAME, "name"]);
  const published = exists && succeeds("npm", ["view", `${NAME}@${version}`, "version"]);
  // Read by the workflow from its output file.
  console.log(`exists=${exists}`);
  console.log(`published=${published}`);
}

function notes(version) {
  console.log(releasedSection(version ?? versionOf()).body);
}

const COMMANDS = {
  prepare,
  check,
  tag,
  push,
  local,
  approve,
  "verify-tag": verifyTag,
  registry,
  notes,
};

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

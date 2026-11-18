// The checks a release goes through (`tools/release-files.mjs`), run on a directory the test makes:
// the six executables (two for each target), SHA256SUMS, and an app.json signed with a key made for
// the test and thrown away. Nothing here reaches the network, and nothing writes outside a temporary directory.

import { createHash, generateKeyPairSync, sign } from "node:crypto";
import { chmodSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import assert from "node:assert/strict";
import { afterEach, describe, it } from "node:test";

import {
  BATERI_KEY,
  TARGETS,
  Refusal,
  fileOf,
  noTerminalFileOf,
  ownTarget,
  releaseFiles,
} from "../tools/release-files.mjs";

const VERSION = "0.6.0";

const dirs = [];
afterEach(() => {
  while (dirs.length > 0) rmSync(dirs.pop(), { recursive: true, force: true });
});

/** A key made for one test: the signer, and the text `app.json` names it by. */
function newKey() {
  const { privateKey, publicKey } = generateKeyPairSync("ed25519");
  const raw = publicKey.export({ format: "der", type: "spki" }).subarray(-32);
  return { privateKey, text: `ed25519:${raw.toString("base64")}` };
}

const sha256 = (bytes) => createHash("sha256").update(bytes).digest("hex");

/** The key each test release is signed with, by its directory: what `check` trusts for it. */
const trusted = new Map();

/** `releaseFiles`, trusting the key the test made for the directory (bateri's own if none). */
const check = (dir, version) => releaseFiles(dir, version, trusted.get(dir));

/**
 * A program of a release: a script that says its version, as the one this machine can run is
 * asked to. The one with the drawing in it is the longer, as the file is.
 */
const program = (says, target, drawing) =>
  `#!/bin/sh\necho ${says}\n# ${target}\n${drawing ? `# the drawing${".".repeat(300)}\n` : ""}`;

/**
 * The files of a complete release in a new directory, and the means to make them wrong. `bytes`
 * says what a program is, for a target and a kind (`drawing`: the one with the drawing in it); the
 * sums, SHA256SUMS and app.json are made from what it says.
 */
function release(version = VERSION, { bytes = (target, drawing) => program(version, target, drawing) } = {}) {
  const dir = mkdtempSync(join(tmpdir(), "textpen-release-test-"));
  dirs.push(dir);
  const out = join(dir, "release");
  mkdirSync(out);
  const key = newKey();
  trusted.set(out, key.text);
  const sums = new Map();
  for (const target of TARGETS) {
    for (const [name, drawing] of [
      [fileOf(version, target), true],
      [noTerminalFileOf(version, target), false],
    ]) {
      const made = bytes(target, drawing);
      writeFileSync(join(out, name), made);
      chmodSync(join(out, name), 0o755);
      sums.set(name, sha256(made));
    }
  }
  const manifest = {
    name: "textpen",
    version,
    protocol: [1],
    views: [{ kind: "lua", versions: ["0.3"] }],
    key: key.text,
    files: Object.fromEntries(
      TARGETS.map((target) => {
        // The file a terminal that draws the page itself takes: the one without the drawing.
        const name = noTerminalFileOf(version, target);
        return [
          target,
          { url: `https://github.com/bateri/textpen/releases/download/v${version}/${name}`, sha256: sums.get(name) },
        ];
      }),
    ),
    previous: [],
    opens: ["file", "folder"],
  };
  const write = {
    sums: (map = sums) =>
      writeFileSync(join(out, "SHA256SUMS"), [...map].map(([name, sum]) => `${sum}  ${name}\n`).join("")),
    manifest: (value = manifest, signer = key.privateKey) => {
      const bytes = Buffer.from(JSON.stringify(value, null, 2));
      writeFileSync(join(out, "app.json"), bytes);
      writeFileSync(join(out, "app.json.sig"), `ed25519:${sign(null, bytes, signer).toString("base64")}\n`);
    },
  };
  write.sums();
  write.manifest();
  return { out, manifest, sums, key, write };
}

const refusal = (what) => (error) => {
  assert.ok(error instanceof Refusal, `${error}`);
  assert.ok(error.message.includes(what), `${JSON.stringify(what)} is not in ${JSON.stringify(error.message)}`);
  return true;
};

describe("the files of a release", () => {
  it("are the six executables, SHA256SUMS, app.json and its signature", () => {
    const { out } = release();
    assert.deepEqual(check(out, VERSION), [
      ...TARGETS.flatMap((target) => [fileOf(VERSION, target), noTerminalFileOf(VERSION, target)]),
      "SHA256SUMS",
      "app.json",
      "app.json.sig",
    ]);
  });

  it("are refused when an executable of either kind is missing", () => {
    for (const name of [fileOf(VERSION, TARGETS[1]), noTerminalFileOf(VERSION, TARGETS[1])]) {
      const { out } = release();
      rmSync(join(out, name));
      assert.throws(() => check(out, VERSION), refusal(`dist/release holds`));
    }
  });

  it("are refused when SHA256SUMS names the executables of one kind only", () => {
    for (const drawing of [true, false]) {
      const { out, sums, write } = release();
      const kind = (name) => name.startsWith("textpen-no-terminal-") !== drawing;
      write.sums(new Map([...sums].filter(([name]) => kind(name))));
      assert.throws(() => check(out, VERSION), refusal("does not name exactly the six executables"));
    }
  });

  it("are refused when app.json names the executable with the drawing, not the one without", () => {
    const { out, manifest, sums, write } = release();
    const target = TARGETS[2];
    const name = fileOf(VERSION, target);
    write.manifest({
      ...manifest,
      files: {
        ...manifest.files,
        [target]: {
          url: `https://github.com/bateri/textpen/releases/download/v${VERSION}/${name}`,
          sha256: sums.get(name),
        },
      },
    });
    assert.throws(() => check(out, VERSION), refusal(`does not end in ${noTerminalFileOf(VERSION, target)}`));
  });

  it("are refused when the two executables of a target are one file, however they are listed and signed", () => {
    const target = TARGETS[1];
    const { out } = release(VERSION, {
      bytes: (of, drawing) => program(VERSION, of, of === target ? true : drawing),
    });
    assert.throws(
      () => check(out, VERSION),
      refusal(`${noTerminalFileOf(VERSION, target)} is the same file as ${fileOf(VERSION, target)}`),
    );
  });

  it("are refused when the executable without the drawing is not the smaller, however they are listed and signed", () => {
    const target = TARGETS[0];
    // The two builds the wrong way round: the copy of the first was taken after the second was made.
    const { out } = release(VERSION, {
      bytes: (of, drawing) => program(VERSION, of, of === target ? !drawing : drawing),
    });
    assert.throws(
      () => check(out, VERSION),
      refusal(`${noTerminalFileOf(VERSION, target)} is not smaller than ${fileOf(VERSION, target)}`),
    );
  });

  it("take executables that say the version, and refuse either kind that says another", () => {
    const own = ownTarget();
    if (own === undefined) return;
    for (const drawing of [true, false]) {
      const says = (text) =>
        release(VERSION, {
          bytes: (target, of) => program(target === own && of === drawing ? text : VERSION, target, of),
        });
      assert.equal(check(says(VERSION).out, VERSION).length, 9);
      const name = drawing ? fileOf(VERSION, own) : noTerminalFileOf(VERSION, own);
      assert.throws(() => check(says("0.5.1").out, VERSION), refusal(`${name} says it is 0.5.1`));
    }
  });

  it("are refused when app.json or its signature is missing", () => {
    for (const name of ["app.json", "app.json.sig"]) {
      const { out } = release();
      rmSync(join(out, name));
      assert.throws(() => check(out, VERSION), refusal(`dist/release holds`));
    }
  });

  it("are refused when something else is in the directory", () => {
    const { out } = release();
    writeFileSync(join(out, "notes.txt"), "not a file of the release");
    assert.throws(() => check(out, VERSION), refusal("notes.txt"));
  });

  it("are refused when app.json says another sum than SHA256SUMS, however well it is signed", () => {
    const { out, manifest, write } = release();
    const target = TARGETS[1];
    write.manifest({
      ...manifest,
      files: { ...manifest.files, [target]: { ...manifest.files[target], sha256: "0".repeat(64) } },
    });
    assert.throws(() => check(out, VERSION), refusal(`another sum for ${noTerminalFileOf(VERSION, target)}`));
  });

  it("are refused when SHA256SUMS lists a sum the file does not have", () => {
    const { out, sums, write } = release();
    const name = fileOf(VERSION, TARGETS[0]);
    write.sums(new Map([...sums, [name, "0".repeat(64)]]));
    assert.throws(() => check(out, VERSION), refusal(`${name} is not the file SHA256SUMS lists`));
  });

  it("are refused when app.json is not what was signed", () => {
    const { out } = release();
    const bytes = readFileSync(join(out, "app.json"), "utf8");
    // One character of the version changed: still JSON, still of the shape.
    writeFileSync(join(out, "app.json"), bytes.replace('"protocol": [\n    1', '"protocol": [\n    2'));
    assert.throws(() => check(out, VERSION), refusal("not the signature of app.json"));
  });

  it("are refused when app.json is signed by another key than the one it names", () => {
    const { out, manifest, write } = release();
    write.manifest(manifest, newKey().privateKey);
    assert.throws(() => check(out, VERSION), refusal("not the signature of app.json"));
  });

  it("are refused when the signature is not written as it should be", () => {
    for (const text of ["", "nonsense\n", "ed25519:AAAA\n", `rsa:${"A".repeat(88)}\n`]) {
      const { out } = release();
      writeFileSync(join(out, "app.json.sig"), text);
      assert.throws(() => check(out, VERSION), refusal("app.json.sig is not one line"));
    }
  });

  it("are refused when app.json is not JSON or has no key", () => {
    const first = release();
    writeFileSync(join(first.out, "app.json"), "not json");
    assert.throws(() => check(first.out, VERSION), refusal("is not JSON"));
    const second = release();
    const { key: _, ...withoutKey } = second.manifest;
    second.write.manifest(withoutKey);
    assert.throws(() => check(second.out, VERSION), refusal("does not name an ed25519 key"));
  });

  it("are refused when app.json is of another app, another version or other targets", () => {
    let made = release();
    made.write.manifest({ ...made.manifest, name: "another" });
    assert.throws(() => check(made.out, VERSION), refusal(`is of "another"`));

    made = release();
    made.write.manifest({ ...made.manifest, version: "0.5.1" });
    assert.throws(() => check(made.out, VERSION), refusal("is of version 0.5.1"));

    made = release();
    const { [TARGETS[2]]: _, ...two } = made.manifest.files;
    made.write.manifest({ ...made.manifest, files: two });
    assert.throws(() => check(made.out, VERSION), refusal("exactly the three targets"));

    made = release();
    made.write.manifest({
      ...made.manifest,
      files: {
        ...made.manifest.files,
        [TARGETS[0]]: { ...made.manifest.files[TARGETS[0]], url: "https://example.com/elsewhere" },
      },
    });
    assert.throws(() => check(made.out, VERSION), refusal(`does not end in ${noTerminalFileOf(VERSION, TARGETS[0])}`));
  });

  it("are refused when they are of another version than the one released", () => {
    const { out } = release("0.5.1");
    assert.throws(() => check(out, VERSION), refusal("dist/release holds"));
  });

  it("are refused when they are signed with another key than bateri's, however well", () => {
    const { out } = release();
    // Signed, and the signature holds for the key it names — a key made for a test, not bateri's.
    assert.throws(() => releaseFiles(out, VERSION), refusal(`not with the key bateri takes textpen by (${BATERI_KEY})`));
  });

  it("are refused unless app.json says that textpen opens a file and a folder", () => {
    const { out, manifest, write } = release();
    const { opens, ...without } = manifest;
    assert.deepEqual(opens, ["file", "folder"]);
    for (const bad of [undefined, [], ["file"], ["folder"], ["file", "file"], ["file", "folder", "link"], "file,folder"]) {
      write.manifest(bad === undefined ? without : { ...without, opens: bad });
      assert.throws(() => check(out, VERSION), refusal('does not say "opens"'), JSON.stringify(bad));
    }
    // Either order of the two is the same release.
    write.manifest({ ...without, opens: ["folder", "file"] });
    check(out, VERSION);
  });

  it("are refused when app.json names the key bateri takes but is signed by another", () => {
    const { out, manifest, write } = release();
    // The key's text alone is not enough: the signature has to be by that key.
    write.manifest({ ...manifest, key: BATERI_KEY });
    assert.throws(() => releaseFiles(out, VERSION), refusal("not the signature of app.json"));
  });

  it("are refused when there is no dist/release", () => {
    assert.throws(() => check(join(tmpdir(), "textpen-release-nothing-here"), VERSION), refusal("no dist/release"));
  });
});

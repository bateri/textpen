// The quick open's matching: what people type finds the file they mean, and not the
// files that merely contain the letters somewhere.

import assert from "node:assert/strict";
import { test } from "node:test";

import { fuzzyPath } from "../src/view/lib/fuzzy.ts";

const FILES = [
  "src/lib/orders.ts",
  "src/lib/format.ts",
  "src/routes/refunds.ts",
  "src/routes/products.ts",
  "src/server.ts",
  "README.md",
  "docs/CHANGELOG.md",
  "worker/mailer.py",
];

function best(query: string): string[] {
  return FILES.map((path) => ({ path, match: fuzzyPath(query, path) }))
    .filter((x) => x.match !== undefined)
    .sort((a, b) => b.match!.score - a.match!.score)
    .map((x) => x.path);
}

test("a word of the name finds the file, and only it", () => {
  assert.deepEqual(best("ord"), ["src/lib/orders.ts"]);
  assert.deepEqual(best("mail"), ["worker/mailer.py"]);
});

test("the start of each directory leads to the file", () => {
  assert.equal(best("s/l/fo")[0], "src/lib/format.ts");
  assert.equal(best("routes/pro")[0], "src/routes/products.ts");
});

test("a name comes before a path that only contains the letters", () => {
  assert.equal(best("server")[0], "src/server.ts");
  assert.equal(best("chlog")[0], "docs/CHANGELOG.md");
});

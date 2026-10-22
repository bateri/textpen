// The view's changes as the program applies them: whatever the edit, applying the calls
// in order to the old text gives the new one, and no call is near the terminal's limit.

import { ChangeSet, Text } from "@codemirror/state";
import assert from "node:assert/strict";
import { test } from "node:test";

import { batches } from "../src/view/editor/batches.ts";

function apply(text: string, groups: [number, number, string][][]): string {
  for (const group of groups) {
    for (const [from, to, insert] of group) {
      assert.ok(
        from >= 0 && from <= to && to <= text.length,
        `edit ${from}-${to} of ${text.length}`,
      );
      text = text.slice(0, from) + insert + text.slice(to);
    }
  }
  return text;
}

test("several changes, applied from the end, give the new text", () => {
  const before = "one two three four";
  const changes = ChangeSet.of(
    [
      { from: 0, to: 3, insert: "ONE" },
      { from: 8, to: 13, insert: "" },
      { from: 18, insert: " five" },
    ],
    before.length,
  );
  const after = changes.apply(Text.of([before])).toString();
  assert.equal(apply(before, batches(changes)), after);
});

test("composed changes over many lines", () => {
  const before = "a\nb\nc\nd";
  let changes = ChangeSet.of([{ from: 2, to: 3, insert: "B\nB2" }], before.length);
  changes = changes.compose(ChangeSet.of([{ from: 0, insert: "// top\n" }], changes.newLength));
  changes = changes.compose(
    ChangeSet.of(
      [{ from: changes.newLength - 1, to: changes.newLength, insert: "D" }],
      changes.newLength,
    ),
  );
  const after = changes.apply(Text.of(before.split("\n"))).toString();
  assert.equal(apply(before, batches(changes)), after);
});

test("a long insertion is cut, and every call stays under the limit", () => {
  const before = "start end";
  const big = "x".repeat(50_000) + "\n" + "y".repeat(30_000);
  const changes = ChangeSet.of(
    [
      { from: 6, to: 6, insert: big },
      { from: 0, to: 5, insert: "S" },
    ],
    before.length,
  );
  const groups = batches(changes, 10_000);
  for (const group of groups) {
    const size = group.reduce((sum, [, , insert]) => sum + insert.length, 0);
    assert.ok(size <= 10_000, `a call of ${size}`);
  }
  assert.ok(groups.length > 8);
  assert.equal(apply(before, groups), changes.apply(Text.of([before])).toString());
});

test("nothing changed is no call", () => {
  assert.deepEqual(batches(ChangeSet.empty(10)), []);
});

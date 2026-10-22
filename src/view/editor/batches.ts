/**
 * What changed in a file, as the calls that tell the program: plain replacements made
 * from the end of the text to its start (so that one does not move the next), a long
 * insertion cut in pieces, in groups that keep each call well under the 1 MiB a terminal
 * lets through to its program. Pure, so that it is tested without a browser.
 */

import type { ChangeSet } from "@codemirror/state";

export type Edit = [from: number, to: number, insert: string];

/** The most text one call carries, in UTF-16 units. */
export const CALL_TEXT = 192 * 1024;

export function batches(changes: ChangeSet, limit = CALL_TEXT): Edit[][] {
  const edits: Edit[] = [];
  changes.iterChanges((fromA, toA, _fromB, _toB, inserted) => {
    edits.push([fromA, toA, inserted.toString()]);
  });
  edits.reverse();
  const pieces: Edit[] = [];
  for (const [from, to, insert] of edits) {
    if (insert.length <= limit) {
      pieces.push([from, to, insert]);
      continue;
    }
    // The first piece replaces the range; the rest go in after it, one after another.
    let at = from;
    for (let i = 0; i < insert.length; i += limit) {
      const piece = insert.slice(i, i + limit);
      pieces.push(i === 0 ? [from, to, piece] : [at, at, piece]);
      at += piece.length;
    }
  }
  const groups: Edit[][] = [];
  let group: Edit[] = [];
  let size = 0;
  for (const edit of pieces) {
    if (group.length > 0 && size + edit[2].length > limit) {
      groups.push(group);
      group = [];
      size = 0;
    }
    group.push(edit);
    size += edit[2].length + 16;
  }
  if (group.length > 0) groups.push(group);
  return groups;
}

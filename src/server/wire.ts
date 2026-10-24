/**
 * What one message to the view may carry. The host passes a page's messages unread and
 * drops one over 1 MiB, in either direction, so nothing the program answers is allowed to
 * grow with the project: a long text, a directory of thousands of entries, the files of a
 * big repository and a long search all come in pieces that fit.
 */

/**
 * The room a payload has in a message of 1 MiB: what is around it (the envelope, the
 * other members of the result) takes a few hundred bytes, and this leaves them 124 KiB.
 */
export const WIRE_BUDGET = 900 * 1024;

/**
 * How many bytes a text takes in a message to the view: JSON's escapes, and pencere's
 * `\uXXXX` for every code unit outside ASCII, which a frame may not carry raw.
 */
export function wireSize(text: string): number {
  let size = 0;
  for (let i = 0; i < text.length; i += 1) size += unitSize(text.charCodeAt(i));
  return size;
}

/** The bytes one UTF-16 code unit takes in a message to the view. */
export function unitSize(c: number): number {
  if (c >= 0x7f) return 6;
  if (c === 0x22 || c === 0x5c || c === 0x0a || c === 0x09 || c === 0x0d) return 2;
  return c < 0x20 ? 6 : 1;
}

/** How many of the first `items` go in a message together, each taking `size(item)` bytes. At least one. */
export function fitting<T>(items: readonly T[], size: (item: T) => number): number {
  let used = 0;
  for (let i = 0; i < items.length; i += 1) {
    used += size(items[i]!);
    if (used > WIRE_BUDGET) return Math.max(i, 1);
  }
  return items.length;
}

/**
 * A file's text from the program, whole. A text too long for one message comes announced
 * (`partial`) and is read in pieces of the same version; a piece is at most a million
 * UTF-16 units, six megabytes on the wire when none of it is ASCII.
 */

import type { OpenedBuffer, Unopenable } from "../../server/types";
import { call } from "./bridge";

const SLICE = 1_000_000;

export async function whole<T extends OpenedBuffer | Unopenable>(opened: T): Promise<T> {
  if (!("text" in opened) || opened.partial === undefined) return opened;
  const { partial, ...rest } = opened;
  const parts: string[] = [];
  for (let from = 0; from < partial.length; from += SLICE) {
    const piece = await call<{ text: string }>("textpen_slice", {
      path: opened.path,
      version: opened.version,
      from,
      to: Math.min(from + SLICE, partial.length),
    });
    parts.push(piece.text);
  }
  return { ...rest, text: parts.join("") } as T;
}

/** Calls a tool that answers with a buffer, and reads its text whole. */
export async function callText<T extends OpenedBuffer | Unopenable>(
  name: string,
  args: Record<string, unknown>,
): Promise<T> {
  return whole(await call<T>(name, args));
}

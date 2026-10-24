/**
 * A file's text from the program, whole. A text too long for one message comes announced
 * (`partial`) and is read in pieces of the same version, each as much as one message holds
 * (the program cuts them: 1 MiB at most, and none of it is ASCII in the worst case).
 */

import type { OpenedBuffer, Unopenable } from "../../server/types";
import { call } from "./page";

export async function whole<T extends OpenedBuffer | Unopenable>(opened: T): Promise<T> {
  if (!("text" in opened) || opened.partial === undefined) return opened;
  const { partial, ...rest } = opened;
  const parts: string[] = [];
  for (let from = 0; from < partial.length;) {
    const piece = await call<{ text: string; end: number }>("slice", {
      path: opened.path,
      version: opened.version,
      from,
    });
    if (piece.end <= from) throw new Error(`${opened.path} could not be read to its end`);
    parts.push(piece.text);
    from = piece.end;
  }
  return { ...rest, text: parts.join("") } as T;
}

/** Calls a method that answers with a buffer, and reads its text whole. */
export async function callText<T extends OpenedBuffer | Unopenable>(
  method: string,
  params: Record<string, unknown>,
): Promise<T> {
  return whole(await call<T>(method, params));
}

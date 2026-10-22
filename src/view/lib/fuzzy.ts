/**
 * Fuzzy matching for the palette: every character of the query, in order, somewhere in
 * the text, scored so that matches at word starts and in a run come first — and, for a
 * path, in the file's own name before its directories.
 */

export interface Match {
  score: number;
  /** The indices of the matched characters, for highlighting. */
  positions: number[];
}

const BOUNDARY = /[\s\-_./:\\]/;

function boundaryAt(text: string, lower: string, at: number): boolean {
  if (at === 0) return true;
  if (BOUNDARY.test(lower[at - 1]!)) return true;
  // camelCase: a capital after a small letter starts a word.
  const here = text[at]!;
  const before = text[at - 1]!;
  return here !== here.toLowerCase() && before === before.toLowerCase();
}

/** Matches `query` in `text` from `start` on; undefined when a character is missing. */
function scan(query: string, text: string, lower: string, start: number): Match | undefined {
  const positions: number[] = [];
  let score = 0;
  let run = 0;
  let from = start;
  for (const char of query) {
    if (char === " ") continue;
    const first = lower.indexOf(char, from);
    if (first < 0) return undefined;
    let at = first;
    // A character that goes on a run stays there; one that does not is taken at a word
    // start a little further on, if there is one.
    if (positions.at(-1) !== first - 1) {
      for (
        let probe = first;
        probe >= 0 && probe < first + 24;
        probe = lower.indexOf(char, probe + 1)
      ) {
        if (boundaryAt(text, lower, probe)) {
          at = probe;
          break;
        }
      }
    }
    const isBoundary = boundaryAt(text, lower, at);
    run = positions.length > 0 && positions.at(-1) === at - 1 ? run + 1 : 0;
    score += 1 + run * 3 + (isBoundary ? 5 : 0) - Math.min(at - from, 20) * 0.15;
    positions.push(at);
    from = at + 1;
  }
  return { score, positions };
}

/** Characters spread over a whole name match it by chance. */
function scattered(positions: readonly number[]): boolean {
  if (positions.length < 2) return false;
  const span = positions.at(-1)! - positions[0]! + 1;
  return span > Math.max(positions.length * 3, positions.length + 4);
}

export function fuzzy(query: string, text: string): Match | undefined {
  const q = query.trim().toLowerCase();
  if (q === "") return { score: 0, positions: [] };
  const lower = text.toLowerCase();
  const direct = lower.indexOf(q);
  if (direct >= 0) {
    const positions = Array.from({ length: q.length }, (_, i) => direct + i);
    const start = boundaryAt(text, lower, direct) ? 30 : 0;
    return { score: 100 + start - direct * 0.5 - lower.length * 0.05, positions };
  }
  const match = scan(q, text, lower, 0);
  if (!match) return undefined;
  if (scattered(match.positions)) return undefined;
  return { score: match.score - lower.length * 0.05, positions: match.positions };
}

/**
 * A path: the query's characters may be spread over its directories, but a match in the
 * file's name scores far above one in the directories, and a shorter path above a longer.
 */
export function fuzzyPath(query: string, path: string): Match | undefined {
  const q = query.trim().toLowerCase().replace(/\\/g, "/");
  if (q === "") return { score: 0, positions: [] };
  const lower = path.toLowerCase();
  const nameStart = path.lastIndexOf("/") + 1;
  const penalty = path.length * 0.04 + (path.split("/").length - 1) * 0.6;

  // The whole query in the file's name.
  if (!q.includes("/")) {
    const name = lower.slice(nameStart);
    const direct = name.indexOf(q);
    if (direct >= 0) {
      const positions = Array.from({ length: q.length }, (_, i) => nameStart + direct + i);
      const exact = name === q || name.startsWith(`${q}.`) ? 40 : 0;
      return { score: 200 + exact + (direct === 0 ? 30 : 0) - direct - penalty, positions };
    }
    const inName = scan(q, path, lower, nameStart);
    if (inName && !scattered(inName.positions)) {
      return { score: 120 + inName.score - penalty, positions: inName.positions };
    }
  }
  const direct = lower.indexOf(q);
  if (direct >= 0) {
    const positions = Array.from({ length: q.length }, (_, i) => direct + i);
    return { score: 90 + (boundaryAt(path, lower, direct) ? 20 : 0) - penalty, positions };
  }
  const spread = scan(q, path, lower, 0);
  if (!spread) return undefined;
  // The last characters of the query belong in the file's name, as people type a path.
  const positions = spread.positions;
  if (positions.at(-1)! < nameStart) return undefined;
  // In the directories a character starts a word or goes on a run (`s/l/or` for
  // src/lib/orders.ts); one picked from the middle of a word is a coincidence.
  for (let i = 0; i < positions.length && positions[i]! < nameStart; i += 1) {
    const at = positions[i]!;
    if (BOUNDARY.test(lower[at]!)) continue; // a `/` typed is a `/` matched
    if (positions[i - 1] !== at - 1 && !boundaryAt(path, lower, at)) return undefined;
  }
  if (scattered(positions.filter((p) => p >= nameStart))) return undefined;
  return { score: spread.score - penalty, positions };
}

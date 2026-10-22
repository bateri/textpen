/**
 * A trace of textpen's turns with the terminal, for finding out why a view did not come:
 * `TEXTPEN_TRACE=/tmp/textpen.trace textpen`. Each line has the time, what happened, and,
 * before each request for a view, the tty's modes and who has its foreground, as the
 * terminal sees them when it decides whether to answer.
 */

import { execFileSync, spawnSync } from "node:child_process";
import { appendFileSync } from "node:fs";

const file = process.env.TEXTPEN_TRACE;
const started = Date.now();

export const tracing = file !== undefined && file !== "";

export function trace(what: string, detail: Record<string, unknown> = {}): void {
  if (!tracing) return;
  const line = `${((Date.now() - started) / 1000).toFixed(3)} ${what} ${JSON.stringify(detail)}\n`;
  try {
    appendFileSync(file!, line);
  } catch {
    // A trace that cannot be written is no reason to stop.
  }
}

/** The tty as the terminal sees it: its modes, and the group in its foreground against ours. */
export function traceTty(what: string): void {
  if (!tracing) return;
  const stty = spawnSync("stty", ["-a"], { stdio: ["inherit", "pipe", "pipe"], encoding: "utf8" });
  const modes = (stty.stdout || stty.stderr || "").match(/-?(icanon|echo|isig)\b/g) ?? [];
  let groups = "";
  try {
    groups = execFileSync("ps", ["-o", "pid=,pgid=,tpgid=,stat=", "-p", String(process.pid)], {
      encoding: "utf8",
    }).trim();
  } catch {
    groups = "unknown";
  }
  trace(what, { modes: modes.join(" "), "pid pgid tpgid stat": groups, stty: stty.status });
}

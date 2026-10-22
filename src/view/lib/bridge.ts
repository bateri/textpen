/**
 * The view's side of the conversation with its program, through whatever host
 * shows it: a terminal pane, a chat client's frame, the development harness.
 */

import type { Host, ToolResult } from "./host";

let host: Host | null = null;

export function attach(connected: Host | null): void {
  host = connected;
}

export class ToolError extends Error {
  override name = "ToolError";
}

export function textOf(result: ToolResult): string {
  const item = (result.content ?? []).find((c) => c.type === "text");
  return item?.text ?? "";
}

/** Calls a tool of the program and returns its structured result, or throws with what it said. */
export async function call<T>(name: string, args: Record<string, unknown> = {}): Promise<T> {
  if (host === null) throw new ToolError("Not connected to the program yet");
  let result: ToolResult;
  try {
    result = await host.callTool(name, args);
  } catch (error) {
    throw new ToolError(error instanceof Error ? error.message : String(error));
  }
  if (result.isError) throw new ToolError(textOf(result) || `${name} failed`);
  return result.structuredContent as T;
}

/** Whether the host opens links for the view. */
export function canOpenLinks(): boolean {
  return host?.capabilities.openLinks !== undefined;
}

export async function openLink(url: string): Promise<boolean> {
  if (host === null) return false;
  try {
    const result = (await host.request("ui/open-link", { url })) as
      { isError?: boolean } | undefined;
    return !result?.isError;
  } catch {
    return false;
  }
}

/** Copies text, with the old way for a frame where the clipboard API is not allowed. */
export async function copy(text: string): Promise<boolean> {
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    const area = document.createElement("textarea");
    area.value = text;
    area.setAttribute("readonly", "");
    area.style.position = "fixed";
    area.style.opacity = "0";
    document.body.append(area);
    area.select();
    let done = false;
    try {
      done = document.execCommand("copy");
    } catch {
      done = false;
    }
    area.remove();
    return done;
  }
}

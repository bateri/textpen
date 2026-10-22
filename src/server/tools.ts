/**
 * The MCP server: one tool with the view, two a model may call to read, and the tools
 * only the view calls (`visibility: ["app"]`). Whatever writes is the view's: a model
 * can read the project's files, and a person edits them.
 *
 * Nothing in this file knows about pencere. Started by an MCP Apps host over stdio, the
 * same server shows the same view there.
 */

import {
  RESOURCE_MIME_TYPE,
  registerAppResource,
  registerAppTool,
} from "@modelcontextprotocol/ext-apps/server";
import { McpServer } from "@modelcontextprotocol/server";
import { readFile } from "node:fs/promises";
import { z } from "zod";

import { VERSION } from "./app.js";
import type { Textpen } from "./app.js";
import { OutOfSync, sendable } from "./buffers.js";
import type { Edit } from "./buffers.js";
import { ICONS } from "./icon.js";
import type { Settings, UiState } from "./types.js";

export const VIEW_URI = "ui://textpen/view";

/** The view as the files `vite build` writes, for a terminal that keeps them by content. */
export const VIEW_DIRECTORY = new URL("../view/", import.meta.url);

/** The view as one document, for every other MCP Apps host. */
const VIEW_DOCUMENT = new URL("../single/index.html", import.meta.url);

const place = z.object({ anchor: z.number(), head: z.number(), top: z.number() });

const uiState = z.object({
  tabs: z.array(z.string()),
  active: z.string().nullable(),
  expanded: z.array(z.string()),
  places: z.record(z.string(), place),
  recent: z.array(z.string()),
  preview: z.array(z.string()),
});

const settings = z.object({
  vim: z.boolean(),
  wrap: z.boolean(),
  fontSize: z.number(),
  lineNumbers: z.enum(["on", "relative", "off"]),
  tabSize: z.number(),
  insertSpaces: z.boolean(),
  whitespace: z.boolean(),
  autoSave: z.boolean(),
  sidebar: z.boolean(),
  sidebarWidth: z.number(),
});

const app = { _meta: { ui: { visibility: ["app" as const] } } };

function text(value: string) {
  return { content: [{ type: "text" as const, text: value }] };
}

function data<T>(value: T, summary: string) {
  return { ...text(summary), structuredContent: value as Record<string, unknown> };
}

function failure(error: unknown) {
  // The view tells an edit that missed its version from any other failure by the name.
  const name = error instanceof OutOfSync ? "OutOfSync: " : "";
  return {
    isError: true,
    ...text(name + (error instanceof Error ? error.message : String(error))),
  };
}

/** Runs a handler and turns what it throws into a tool error the view can show. */
function guarded<A, R>(handler: (args: A) => Promise<R>) {
  return async (args: A) => {
    try {
      return await handler(args);
    } catch (error) {
      return failure(error);
    }
  };
}

export function createServer(textpen: Textpen): McpServer {
  // A terminal draws the icon beside the program's name above the view; other clients, beside the server.
  const server = new McpServer({ name: "textpen", version: VERSION, icons: ICONS });

  // ---- the view's tool -----------------------------------------------------------------

  const view = registerAppTool(
    server,
    "show_editor",
    {
      // What a terminal writes above the view, in place of `show_editor`.
      title: "Editor",
      description: "Open a code editor on the project's files, optionally on one file and line",
      inputSchema: z.object({
        path: z.string().optional().describe("A file to open, relative to the project"),
        line: z.number().int().min(1).optional().describe("The line to put the cursor on"),
      }),
      _meta: { ui: { resourceUri: VIEW_URI } },
    },
    guarded(async (args: { path?: string; line?: number }) => {
      if (args.path) {
        const path = textpen.project.name(textpen.project.inside(args.path));
        textpen.focus = { path, ...(args.line ? { line: args.line } : {}) };
        if (!textpen.ui.tabs.includes(path)) {
          textpen.ui = { ...textpen.ui, tabs: [...textpen.ui.tabs, path], active: path };
        }
      }
      const boot = await textpen.boot();
      const open = boot.ui.tabs.length;
      return data(
        { boot },
        `Editor on ${boot.root}${open ? `, ${open} file${open === 1 ? "" : "s"} open` : ""}`,
      );
    }),
  );
  // ext-apps' config has no `icons`, though the tool it registers carries them into tools/list.
  view.icons = ICONS;

  // ---- files ---------------------------------------------------------------------------

  registerAppTool(
    server,
    "textpen_list",
    {
      description: "The entries of a directory of the project",
      inputSchema: z.object({ dir: z.string() }),
      ...app,
    },
    guarded(async ({ dir }: { dir: string }) => {
      const listing = await textpen.project.list(dir);
      return data(listing, `${listing.entries.length} entries in ${listing.dir || "."}`);
    }),
  );

  registerAppTool(
    server,
    "textpen_files",
    { description: "Every file of the project, for the quick open", ...app },
    guarded(async () => {
      const index = await textpen.project.files();
      return data(index, `${index.files.length} files`);
    }),
  );

  registerAppTool(
    server,
    "textpen_search",
    {
      description: "Search the project's files",
      inputSchema: z.object({
        query: z.string().min(1),
        regex: z.boolean().optional(),
        caseSensitive: z.boolean().optional(),
        wholeWord: z.boolean().optional(),
      }),
      ...app,
    },
    guarded(
      async (args: {
        query: string;
        regex?: boolean;
        caseSensitive?: boolean;
        wholeWord?: boolean;
      }) => {
        const result = await textpen.project.search({
          query: args.query,
          regex: args.regex === true,
          caseSensitive: args.caseSensitive === true,
          wholeWord: args.wholeWord === true,
        });
        return data(result, `${result.matches} matches in ${result.files.length} files`);
      },
    ),
  );

  registerAppTool(
    server,
    "textpen_create",
    {
      description: "Create a file or a directory",
      inputSchema: z.object({ path: z.string().min(1), kind: z.enum(["file", "dir"]) }),
      ...app,
    },
    guarded(async ({ path, kind }: { path: string; kind: "file" | "dir" }) => {
      const created = await textpen.create(path, kind);
      return data({ path: created }, `Created ${created}`);
    }),
  );

  registerAppTool(
    server,
    "textpen_rename",
    {
      description: "Rename or move a file or a directory",
      inputSchema: z.object({ from: z.string().min(1), to: z.string().min(1) }),
      ...app,
    },
    guarded(async ({ from, to }: { from: string; to: string }) => {
      const path = await textpen.rename(from, to);
      return data({ path, buffers: textpen.buffers.all() }, `Renamed ${from} to ${path}`);
    }),
  );

  registerAppTool(
    server,
    "textpen_delete",
    {
      description: "Remove a file or a directory with what is in it",
      inputSchema: z.object({ path: z.string().min(1) }),
      ...app,
    },
    guarded(async ({ path }: { path: string }) => {
      await textpen.remove(path);
      return data({ path }, `Removed ${path}`);
    }),
  );

  // ---- buffers -------------------------------------------------------------------------

  registerAppTool(
    server,
    "textpen_open",
    {
      description: "Open a file, or the buffer of one already open, with its text",
      inputSchema: z.object({ path: z.string().min(1) }),
      ...app,
    },
    guarded(async ({ path }: { path: string }) => {
      const opened = sendable(await textpen.buffers.open(path));
      return data(
        opened,
        "text" in opened
          ? `${opened.path}, ${opened.partial?.length ?? opened.text.length} characters`
          : opened.message,
      );
    }),
  );

  registerAppTool(
    server,
    "textpen_edit",
    {
      description: "Apply the view's edits to a buffer",
      inputSchema: z.object({
        path: z.string(),
        base: z.number().int(),
        edits: z.array(z.tuple([z.number().int(), z.number().int(), z.string()])),
      }),
      ...app,
    },
    guarded(async ({ path, base, edits }: { path: string; base: number; edits: Edit[] }) => {
      const info = textpen.buffers.edit(path, base, edits);
      return data(info, `${path} at version ${info.version}`);
    }),
  );

  registerAppTool(
    server,
    "textpen_slice",
    {
      description: "A piece of a buffer's text too long for one message",
      inputSchema: z.object({
        path: z.string(),
        version: z.number().int(),
        from: z.number().int().min(0),
        to: z.number().int().min(0),
      }),
      ...app,
    },
    guarded(
      async ({
        path,
        version,
        from,
        to,
      }: {
        path: string;
        version: number;
        from: number;
        to: number;
      }) => {
        const text = textpen.buffers.slice(path, version, from, to);
        return data({ text }, `${text.length} characters`);
      },
    ),
  );

  registerAppTool(
    server,
    "textpen_save",
    {
      description: "Write a buffer to its file",
      inputSchema: z.object({
        path: z.string(),
        version: z.number().int(),
        force: z.boolean().optional(),
      }),
      ...app,
    },
    guarded(
      async ({ path, version, force }: { path: string; version: number; force?: boolean }) => {
        const result = await textpen.buffers.save(path, version, force === true);
        return data(
          result,
          result.saved ? `Saved ${path}` : `${path} changed on disk since it was opened`,
        );
      },
    ),
  );

  registerAppTool(
    server,
    "textpen_buffer",
    {
      description:
        "Act on a buffer: read it again from disk, keep it over a changed file, take or drop a draft, change its line ends, or close it",
      inputSchema: z.object({
        path: z.string(),
        action: z.enum(["text", "revert", "keep", "restore", "drop", "lf", "crlf", "close"]),
        discard: z.boolean().optional(),
      }),
      ...app,
    },
    guarded(
      async ({
        path,
        action,
        discard,
      }: {
        path: string;
        action: "text" | "revert" | "keep" | "restore" | "drop" | "lf" | "crlf" | "close";
        discard?: boolean;
      }) => {
        const buffers = textpen.buffers;
        switch (action) {
          case "text":
            return data(sendable(buffers.text(path)), path);
          case "revert": {
            const opened = sendable(await buffers.revert(path));
            return data(opened, "text" in opened ? `Read ${path} again` : opened.message);
          }
          case "keep":
            return data(await buffers.keep(path), `Kept the changes to ${path}`);
          case "restore":
          case "drop":
            return data(sendable(buffers.draft(path, action === "restore")), path);
          case "lf":
          case "crlf":
            return data(buffers.setEol(path, action), `${path} ends lines with ${action}`);
          case "close":
            buffers.close(path, discard === true);
            return data({ path }, `Closed ${path}`);
        }
      },
    ),
  );

  // ---- the view's state ------------------------------------------------------------------

  registerAppTool(
    server,
    "textpen_poll",
    {
      description:
        "The view's refresh: git, files changed on disk, directories that changed; keeps the view's state",
      inputSchema: z.object({ ui: uiState.optional() }),
      ...app,
    },
    guarded(async ({ ui }: { ui?: UiState }) => {
      const poll = await textpen.poll(ui);
      return data(poll, `${poll.buffers.length} files changed on disk`);
    }),
  );

  registerAppTool(
    server,
    "textpen_settings",
    {
      description: "Keep the editor's settings",
      inputSchema: z.object({ settings }),
      ...app,
    },
    guarded(async ({ settings: next }: { settings: Settings }) => {
      const saved = textpen.saveSettings(next);
      return data({ settings: saved }, "Settings kept");
    }),
  );

  registerAppTool(
    server,
    "textpen_handoff",
    {
      description:
        "Hand the pane to a shell in the project, which gives it back when it ends; or quit textpen",
      inputSchema: z.object({
        kind: z.enum(["shell", "quit"]),
        discard: z.boolean().optional().describe("Quit dropping what is not saved"),
      }),
      ...app,
    },
    guarded(async ({ kind, discard }: { kind: "shell" | "quit"; discard?: boolean }) => {
      const desk = textpen.desk;
      if (desk === undefined) {
        return data(
          { state: "unavailable" },
          "There is no terminal to hand over: this server was started by an MCP client.",
        );
      }
      const state = desk.request({ kind });
      if (state === "scheduled" && kind === "quit" && discard === true) {
        textpen.buffers.discardAll();
      }
      return data({ state }, state === "scheduled" ? "Handing the pane over" : "Busy");
    }),
  );

  // ---- for a model -----------------------------------------------------------------------

  server.registerTool(
    "list_files",
    {
      title: "Project files",
      description: "List the files of the project textpen was started in",
    },
    guarded(async () => {
      const index = await textpen.project.files();
      return text(index.files.join("\n") + (index.truncated ? "\n… (more files not listed)" : ""));
    }),
  );

  server.registerTool(
    "read_file",
    {
      title: "Read a file",
      description:
        "Read a text file of the project, as textpen has it: unsaved changes in the editor included",
      inputSchema: z.object({
        path: z.string().describe("The file, relative to the project"),
      }),
    },
    guarded(async ({ path }: { path: string }) => {
      const opened = await textpen.buffers.peek(textpen.project.inside(path));
      if (!("text" in opened)) return failure(opened.message);
      if (opened.missing) return failure(`There is no file ${path}`);
      return text(opened.text);
    }),
  );

  // ---- the view's document ----------------------------------------------------------------

  registerAppResource(server, "Editor", VIEW_URI, {}, async (uri) => ({
    contents: [
      { uri: uri.href, mimeType: RESOURCE_MIME_TYPE, text: await readFile(VIEW_DOCUMENT, "utf8") },
    ],
  }));

  return server;
}

/**
 * What the view may call and subscribe to: textpen's side of the six messages. Whatever
 * writes to the disk is the view's word, and the person's.
 *
 * Nothing in this file knows about a terminal or about pencere beyond the shapes of the
 * SDK's handlers. `main.ts` is the only place that asks a terminal to show the view, and
 * the development harness runs the same handlers in a browser's company.
 *
 * Calls: `boot`, `list`, `files`, `search`, `create`, `rename`, `delete`, `open`, `edit`,
 * `slice`, `save`, `buffer`, `ui`, `settings`, `discard` and `handoff`. Topic: `changes`.
 * A call that fails ends with a code the view can act on: `bad-params` for a call that is
 * not what the program asks, `out-of-sync` for an edit made on a version the program no
 * longer holds, `failed` for what the disk or the project would not do (the words are
 * for the person).
 */

import { ViewError } from "@pencere/sdk";
import type { CallContext, CallHandler, TopicHandler } from "@pencere/sdk";
import { z } from "zod";

import type { Textpen } from "./app.js";
import { OutOfSync, sendable } from "./buffers.js";
import type { Changes, FilesPage } from "./types.js";
import { fitting, wireSize } from "./wire.js";

/** The view as the files `vite build` writes, which the terminal keeps by content. */
export const VIEW_DIRECTORY = new URL("../view/", import.meta.url);

/** How often the `changes` topic looks at the disk and at git. */
export const CHANGES_EVERY = 2000;

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

/** What a failure of the disk or of the project tells the view: the words are the person's. */
function failure(error: unknown): ViewError {
  if (error instanceof ViewError) return error;
  // The view tells an edit that missed its version from any other failure by the code.
  const code = error instanceof OutOfSync ? "out-of-sync" : "failed";
  return new ViewError(code, error instanceof Error ? error.message : String(error));
}

function bad(error: z.ZodError): ViewError {
  const issue = error.issues[0];
  const where = issue && issue.path.length > 0 ? `${issue.path.join(".")}: ` : "";
  return new ViewError("bad-params", `${where}${issue?.message ?? "not what the call asks"}`);
}

/**
 * A call that takes the params `schema` describes: the view is not trusted to send them, so
 * what does not fit is answered `bad-params`, and what the work throws is `failure`.
 */
function call<S extends z.ZodType>(
  schema: S,
  work: (params: z.output<S>, context: CallContext) => unknown,
): CallHandler {
  return async (params, context) => {
    const parsed = schema.safeParse(params ?? {});
    if (!parsed.success) throw bad(parsed.error);
    try {
      return await work(parsed.data, context);
    } catch (error) {
      throw failure(error);
    }
  };
}

const nothing = z.object({});
const onePath = z.object({ path: z.string().min(1) });

export function createProgram(textpen: Textpen): {
  calls: Record<string, CallHandler>;
  topics: Record<string, TopicHandler>;
} {
  const calls: Record<string, CallHandler> = {
    /** What the view shows first: the project, its settings, and what is open. */
    boot: call(nothing, () => textpen.boot()),

    // ---- files -----------------------------------------------------------------------

    list: call(z.object({ dir: z.string() }), async ({ dir }) => {
      const listing = await textpen.project.list(dir);
      // A message to the view is at most 1 MiB: a directory too big for one is cut, and says so.
      const count = fitting(listing.entries, (e) => wireSize(e.name) + wireSize(e.path) + 80);
      if (count === listing.entries.length) return listing;
      return { ...listing, entries: listing.entries.slice(0, count), truncated: true };
    }),

    /** The files of the project for the quick open, a message's worth at a time from `from`. */
    files: call(z.object({ from: z.number().int().min(0).optional() }), async ({ from = 0 }) => {
      const index = await textpen.project.files();
      const rest = index.files.slice(from);
      const count = fitting(rest, (path) => wireSize(path) + 4);
      const end = from + count;
      const page: FilesPage = {
        files: rest.slice(0, count),
        next: end < index.files.length ? end : null,
        truncated: index.truncated,
      };
      return page;
    }),

    search: call(
      z.object({
        query: z.string().min(1),
        regex: z.boolean().optional(),
        caseSensitive: z.boolean().optional(),
        wholeWord: z.boolean().optional(),
      }),
      async (args) => {
        const result = await textpen.project.search({
          query: args.query,
          regex: args.regex === true,
          caseSensitive: args.caseSensitive === true,
          wholeWord: args.wholeWord === true,
        });
        // A message to the view is at most 1 MiB: a search too long for one is cut, and says so.
        const count = fitting(result.files, (file) => wireSize(JSON.stringify(file)));
        if (count === result.files.length) return result;
        return { ...result, files: result.files.slice(0, count), truncated: true };
      },
    ),

    create: call(
      z.object({ path: z.string().min(1), kind: z.enum(["file", "dir"]) }),
      async ({ path, kind }) => ({ path: await textpen.create(path, kind) }),
    ),

    rename: call(
      z.object({ from: z.string().min(1), to: z.string().min(1) }),
      async ({ from, to }) => ({
        path: await textpen.rename(from, to),
        buffers: textpen.buffers.all(),
      }),
    ),

    delete: call(onePath, async ({ path }) => {
      await textpen.remove(path);
      return { path };
    }),

    // ---- buffers ---------------------------------------------------------------------

    /** A file, or the buffer of one already open, with its text. */
    open: call(onePath, async ({ path }) => sendable(await textpen.buffers.open(path))),

    /** The view's edits to a buffer, made on a version the program checks. */
    edit: call(
      z.object({
        path: z.string(),
        base: z.number().int(),
        edits: z.array(z.tuple([z.number().int(), z.number().int(), z.string()])),
      }),
      ({ path, base, edits }) => textpen.buffers.edit(path, base, edits),
    ),

    /** The piece of a text too long for one message that begins at `from`, and where the next begins. */
    slice: call(
      z.object({ path: z.string(), version: z.number().int(), from: z.number().int().min(0) }),
      ({ path, version, from }) => textpen.buffers.slice(path, version, from),
    ),

    save: call(
      z.object({ path: z.string(), version: z.number().int(), force: z.boolean().optional() }),
      ({ path, version, force }) => textpen.buffers.save(path, version, force === true),
    ),

    /** Read a buffer again from disk, keep it over a changed file, take or drop a draft, change its line ends, close it. */
    buffer: call(
      z.object({
        path: z.string(),
        action: z.enum(["text", "revert", "keep", "restore", "drop", "lf", "crlf", "close"]),
        discard: z.boolean().optional(),
      }),
      async ({ path, action, discard }) => {
        const buffers = textpen.buffers;
        switch (action) {
          case "text":
            return sendable(buffers.text(path));
          case "revert":
            return sendable(await buffers.revert(path));
          case "keep":
            return await buffers.keep(path);
          case "restore":
          case "drop":
            return sendable(buffers.draft(path, action === "restore"));
          case "lf":
          case "crlf":
            return buffers.setEol(path, action);
          case "close":
            buffers.close(path, discard === true);
            return { path };
        }
      },
    ),

    /** Quitting without saving: what is not saved is not kept as a draft either. */
    discard: call(nothing, () => {
      textpen.buffers.discardAll();
      return {};
    }),

    // ---- the view's state ------------------------------------------------------------

    /** Where the view is (tabs, folders, cursors), kept for the next view and the next run. */
    ui: call(z.object({ ui: uiState }), ({ ui }) => {
      textpen.keep(ui);
      return {};
    }),

    settings: call(z.object({ settings }), ({ settings: next }) => ({
      settings: textpen.saveSettings(next),
    })),

    /** Hands the pane to a shell in the project, which gives it back when it ends. */
    handoff: call(z.object({ kind: z.literal("shell") }), () => {
      const desk = textpen.desk;
      return { state: desk === undefined ? "unavailable" : desk.shell() };
    }),
  };

  const topics: Record<string, TopicHandler> = {
    /**
     * The disk and git, as they change: one value at once and then one only when something
     * differs from what the view was told. Git's own state comes along when it changed.
     */
    changes: (_params, subscription) => {
      let timer: ReturnType<typeof setTimeout> | undefined;
      let told: string | undefined;
      const tick = async () => {
        try {
          const found = await textpen.changes();
          if (subscription.signal.aborted) return;
          const git = JSON.stringify(found.git);
          const value: Changes = {
            ...(git === told ? {} : { git: found.git }),
            buffers: found.buffers,
            dirs: found.dirs,
            ...(found.handoffFailed ? { handoffFailed: found.handoffFailed } : {}),
          };
          const quiet =
            git === told &&
            found.buffers.length === 0 &&
            found.dirs.length === 0 &&
            found.handoffFailed === undefined;
          if (!quiet && subscription.push(value)) told = git;
        } catch {
          // A look at the disk that failed is made again at the next turn.
        }
        if (!subscription.signal.aborted) timer = setTimeout(() => void tick(), CHANGES_EVERY);
      };
      void tick();
      return () => clearTimeout(timer);
    },
  };

  return { calls, topics };
}

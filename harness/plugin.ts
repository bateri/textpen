/**
 * The harness's server side: Vite serves the view with hot reload, and this plugin runs
 * the real textpen program beside it — the same calls and topics `main.ts` gives the SDK —
 * and plays the part of the terminal's tty: the messages a page sends travel here over
 * Vite's own socket, and the program's answers go back the same way. It edits `demo/`
 * unless TEXTPEN_ROOT names another project, and keeps its settings and drafts in
 * `.scratch/`, away from the person's own.
 *
 * The program is the built one (`npm run build:server`, which `npm run harness` runs).
 */

import type { IncomingMessage } from "node:http";
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";
import type { Plugin, WebSocketClient } from "vite";

interface CallContext {
  readonly method: string;
  readonly signal: AbortSignal;
  progress(done: number, total?: number, note?: string): void;
}

interface Subscription {
  readonly topic: string;
  readonly signal: AbortSignal;
  push(value: unknown): boolean;
}

interface Program {
  calls: Record<string, (params: unknown, call: CallContext) => unknown>;
  topics: Record<
    string,
    (
      params: unknown,
      subscription: Subscription,
    ) => void | (() => void) | Promise<void | (() => void)>
  >;
}

interface Session {
  calls: Map<number, AbortController>;
  subscriptions: Map<number, { controller: AbortController; cleanup: (() => void) | undefined }>;
}

type Json = Record<string, unknown>;

const isRecord = (value: unknown): value is Json =>
  typeof value === "object" && value !== null && !Array.isArray(value);

/** What an error that ended a call tells the page: a `ViewError` has its code and words. */
function failure(id: number, error: unknown): Json {
  const named = isRecord(error) || error instanceof Error ? (error as unknown as Json) : {};
  if (named.name === "ViewError" && typeof named.code === "string") {
    return {
      response: {
        id,
        error: {
          code: named.code,
          ...(typeof named.detail === "string" ? { message: named.detail } : {}),
        },
      },
    };
  }
  return {
    response: {
      id,
      error: { code: "internal", message: error instanceof Error ? error.message : String(error) },
    },
  };
}

export function harness(): Plugin {
  let program: Promise<{ program: Program; ending(): void }> | undefined;
  const load = () => {
    program ??= (async () => {
      // The program reads where to keep its settings and drafts when it is asked, so this is
      // set before it is made: a harness never touches the person's own.
      process.env.XDG_CONFIG_HOME = resolve(".scratch/config");
      process.env.XDG_STATE_HOME = resolve(".scratch/state");
      const built = (name: string) => import(pathToFileURL(resolve("dist/server", name)).href);
      const { Textpen } = await built("app.js");
      const { createProgram } = await built("program.js");
      const textpen = new Textpen(resolve(process.env.TEXTPEN_ROOT ?? "demo"));
      return { program: createProgram(textpen) as Program, ending: () => textpen.ending() };
    })();
    return program;
  };

  const sessions = new WeakMap<WebSocketClient, Session>();
  const sessionOf = (client: WebSocketClient) => {
    let session = sessions.get(client);
    if (session === undefined) {
      session = { calls: new Map(), subscriptions: new Map() };
      sessions.set(client, session);
    }
    return session;
  };

  /** What the view had asked is over with it: calls are cancelled, subscriptions stop. */
  const end = (session: Session) => {
    for (const controller of session.calls.values()) controller.abort();
    for (const { controller, cleanup } of session.subscriptions.values()) {
      controller.abort();
      cleanup?.();
    }
    session.calls.clear();
    session.subscriptions.clear();
  };

  /** The page's `call`, `subscribe` and `cancel`, as the SDK's `serve` answers them. */
  const hear = async (session: Session, message: unknown, tell: (message: Json) => void) => {
    if (!isRecord(message)) return;
    const members = Object.keys(message);
    const body = message[members[0] ?? ""];
    if (members.length !== 1 || !isRecord(body)) return;
    const id = body.id;
    if (typeof id !== "number" || !Number.isInteger(id) || id < 1) return;
    const { program: held } = await load();

    if (members[0] === "call" && typeof body.method === "string") {
      if (session.calls.has(id) || session.subscriptions.has(id)) return;
      const handler = Object.hasOwn(held.calls, body.method) ? held.calls[body.method] : undefined;
      if (handler === undefined) {
        tell({ response: { id, error: { code: "not-found" } } });
        return;
      }
      const controller = new AbortController();
      session.calls.set(id, controller);
      try {
        const result = await handler(body.params, {
          method: body.method,
          signal: controller.signal,
          progress: (done, total, note) => {
            if (!controller.signal.aborted) {
              tell({
                progress: {
                  id,
                  done,
                  ...(total === undefined ? {} : { total }),
                  ...(note === undefined ? {} : { note }),
                },
              });
            }
          },
        });
        if (controller.signal.aborted) return;
        session.calls.delete(id);
        tell({ response: { id, result: result ?? {} } });
      } catch (error) {
        if (controller.signal.aborted) return;
        session.calls.delete(id);
        tell(failure(id, error));
      }
    } else if (members[0] === "subscribe" && typeof body.topic === "string") {
      if (session.calls.has(id) || session.subscriptions.has(id)) return;
      const handler = Object.hasOwn(held.topics, body.topic) ? held.topics[body.topic] : undefined;
      if (handler === undefined) {
        tell({ response: { id, error: { code: "not-found" } } });
        return;
      }
      const controller = new AbortController();
      const subscribed = { controller, cleanup: undefined as (() => void) | undefined };
      session.subscriptions.set(id, subscribed);
      try {
        const cleanup = await handler(body.params, {
          topic: body.topic,
          signal: controller.signal,
          push: (value) => {
            if (controller.signal.aborted) return false;
            tell({ push: { id, value: value ?? null } });
            return true;
          },
        });
        if (typeof cleanup !== "function") return;
        if (controller.signal.aborted) cleanup();
        else subscribed.cleanup = cleanup;
      } catch (error) {
        session.subscriptions.delete(id);
        controller.abort();
        tell(failure(id, error));
      }
    } else if (members[0] === "cancel") {
      session.calls.get(id)?.abort();
      session.calls.delete(id);
      const subscribed = session.subscriptions.get(id);
      if (subscribed !== undefined) {
        session.subscriptions.delete(id);
        subscribed.controller.abort();
        subscribed.cleanup?.();
      }
    }
  };

  const body = (request: IncomingMessage) =>
    new Promise<string>((resolve, reject) => {
      let text = "";
      request.on("data", (chunk: Buffer) => (text += chunk.toString("utf8")));
      request.on("end", () => resolve(text));
      request.on("error", reject);
    });

  return {
    name: "textpen-harness",
    configureServer(server) {
      server.ws.on("textpen:message", (message: unknown, client) => {
        void hear(sessionOf(client), message, (reply) => client.send("textpen:message", reply));
      });
      // The stand-in terminal starts the view again: what the old one had asked is over.
      server.ws.on("textpen:reset", (_data: unknown, client) => end(sessionOf(client)));
      server.httpServer?.once("close", () => {
        void program?.then(({ ending }) => ending());
      });
      server.middlewares.use(async (request, response, next) => {
        if (request.url === "/" || request.url?.startsWith("/?")) {
          response.statusCode = 302;
          response.setHeader("Location", "/harness/index.html");
          response.end();
          return;
        }
        // A call of the harness page's own (the state a screenshot starts in), answered once.
        if (request.url !== "/__harness/call") return next();
        try {
          const { method, params } = JSON.parse(await body(request)) as {
            method: string;
            params?: unknown;
          };
          const { program: held } = await load();
          const handler = Object.hasOwn(held.calls, method) ? held.calls[method] : undefined;
          if (handler === undefined) throw new Error(`There is no call ${method}`);
          const result = await handler(params, {
            method,
            signal: new AbortController().signal,
            progress: () => {},
          });
          response.setHeader("Content-Type", "application/json");
          response.end(JSON.stringify(result ?? {}));
        } catch (error) {
          response.statusCode = 500;
          response.end(String(error));
        }
      });
    },
  };
}

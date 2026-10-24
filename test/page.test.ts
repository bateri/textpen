// The page's side of the protocol: what src/view/lib/page.ts says to its host and what it
// does with what the host says, and how the terminal's colours become the editor's. It runs
// the module itself against a stand-in window, with no browser.

import assert from "node:assert/strict";
import { test } from "node:test";

type Listener = (event: { source: unknown; data: unknown }) => void;

const sent: any[] = [];
const host = { postMessage: (message: unknown) => sent.push(message) };
const listeners: Listener[] = [];
Object.assign(globalThis, {
  window: {
    parent: host,
    addEventListener: (_type: string, listener: Listener) => listeners.push(listener),
  },
});

const page: typeof import("../src/view/lib/page.ts") = await import("../src/view/lib/page.ts");
const theme: typeof import("../src/view/lib/theme.ts") = await import("../src/view/lib/theme.ts");

/** The host says something to the page. */
function hear(data: unknown, source: unknown = host) {
  for (const listener of listeners) listener({ source, data });
}

const CONTEXT = {
  mode: "dark",
  colors: {
    background: "#000000",
    foreground: "#d8d9dd",
    dim: "#696b70",
    accent: "#7a9cc6",
    border: "#2b2b2c",
    success: "#8bb58b",
    error: "#d16d6a",
    warning: "#d6b16a",
    info: "#79b3b3",
  },
  font: "Menlo, monospace",
  size: { width: 800, height: 480 },
  locale: "en-US",
  timeZone: "Europe/Istanbul",
  reduceMotion: false,
};

test("asks the host for the context as soon as it is loaded", () => {
  assert.deepEqual(sent.shift(), { context: {} });
});

test("a call is numbered, sent, and answered by its response", async () => {
  const answer = page.call("list", { dir: "src" });
  assert.deepEqual(sent.shift(), { call: { id: 1, method: "list", params: { dir: "src" } } });
  hear({ response: { id: 1, result: { entries: [] } } });
  assert.deepEqual(await answer, { entries: [] });

  const plain = page.call("files");
  assert.deepEqual(sent.shift(), { call: { id: 2, method: "files" } });
  hear({ response: { id: 2, result: {} } });
  await plain;
});

test("a call the program answers with an error rejects with its code and words", async () => {
  const answer = page.call("save", {});
  sent.shift();
  hear({ response: { id: 3, error: { code: "bad-params", message: "version: missing" } } });
  await assert.rejects(answer, (error: any) => {
    return (
      error instanceof page.CallError &&
      error.code === "bad-params" &&
      /missing/.test(error.message)
    );
  });
});

test("what does not come from the host, or is not one message, is ignored", async () => {
  const answer = page.call("list");
  sent.shift();
  hear({ response: { id: 4, result: { from: "a stranger" } } }, { postMessage() {} });
  hear({ response: { id: 4, result: { two: "members" } }, push: { id: 4, value: 1 } });
  hear("a string");
  hear({ response: { id: 0, result: {} } });
  let settled = false;
  answer.then(() => (settled = true));
  await new Promise((resolve) => setTimeout(resolve, 20));
  assert.equal(settled, false);
  hear({ response: { id: 4, result: { from: "the host" } } });
  assert.deepEqual(await answer, { from: "the host" });
});

test("a subscription hears the program's pushes until it is cancelled", () => {
  const heard: unknown[] = [];
  const cancel = page.subscribe("changes", {}, (value) => heard.push(value));
  assert.deepEqual(sent.shift(), { subscribe: { id: 5, topic: "changes", params: {} } });
  hear({ push: { id: 5, value: { n: 1 } } });
  hear({ push: { id: 99, value: { n: "not ours" } } });
  hear({ push: { id: 5, value: { n: 2 } } });
  assert.deepEqual(heard, [{ n: 1 }, { n: 2 }]);
  cancel();
  assert.deepEqual(sent.shift(), { cancel: { id: 5 } });
  hear({ push: { id: 5, value: { n: 3 } } });
  cancel();
  assert.equal(sent.length, 0, "a subscription is cancelled once");
  assert.equal(heard.length, 2);
});

test("a subscription the program refuses is over, and says so", () => {
  const heard: unknown[] = [];
  const logged: unknown[] = [];
  const log = console.error;
  console.error = (...args: unknown[]) => logged.push(args);
  try {
    const cancel = page.subscribe("nothing", undefined, (value) => heard.push(value));
    assert.deepEqual(sent.shift(), { subscribe: { id: 6, topic: "nothing" } });
    hear({ response: { id: 6, error: { code: "not-found" } } });
    hear({ push: { id: 6, value: 1 } });
    cancel();
  } finally {
    console.error = log;
  }
  assert.equal(logged.length, 1);
  assert.deepEqual(heard, []);
  assert.equal(sent.length, 0, "there is nothing left to cancel");
});

test("the context reaches whoever listens, now and when it changes", () => {
  assert.equal(page.context(), undefined);
  const early: unknown[] = [];
  const stop = page.onContext((context) => early.push(context));
  hear({ context: { mode: "sepia" } });
  assert.equal(early.length, 0, "a context that is not one is not told");
  hear({ context: CONTEXT });
  assert.equal(early.length, 1);
  assert.equal(page.context()?.mode, "dark");
  assert.equal(page.context()?.font, "Menlo, monospace");

  const late: any[] = [];
  page.onContext((context) => late.push(context));
  assert.equal(late.length, 1, "a listener that comes late is told what the host last said");
  hear({ context: { ...CONTEXT, mode: "light", size: { width: 640, height: 400 } } });
  assert.deepEqual(
    late.map((c) => [c.mode, c.size.width]),
    [
      ["dark", 800],
      ["light", 640],
    ],
  );
  stop();
  hear({ context: CONTEXT });
  assert.equal(early.length, 2, "a listener that stopped is not told again");
});

test("closing the view and opening a link are the host's words", () => {
  page.close();
  assert.deepEqual(sent.shift(), { close: {} });
  page.openLink("https://example.com/docs");
  assert.deepEqual(sent.shift(), { link: { url: "https://example.com/docs" } });
});

test("the terminal's colours become the editor's, with the steps between them mixed", () => {
  const v = theme.variables(CONTEXT as never);
  assert.equal(v["--color-background-primary"], "#000000");
  assert.equal(v["--color-background-secondary"], "#0d0d0d", "6% of the way to the foreground");
  assert.equal(v["--color-background-tertiary"], "#1a1a1b", "12% of the way");
  assert.equal(v["--color-text-primary"], "#d8d9dd");
  assert.equal(v["--color-text-secondary"], "#696b70", "the dimmed foreground");
  assert.equal(v["--color-text-inverse"], "#000000");
  assert.equal(v["--color-border-primary"], "#2b2b2c");
  assert.equal(v["--color-ring-primary"], "#7a9cc6", "the accent");
  assert.equal(v["--color-text-success"], "#8bb58b");
  assert.equal(v["--color-text-danger"], "#d16d6a", "the error colour");
  assert.equal(v["--color-text-warning"], "#d6b16a");
  assert.equal(v["--color-text-info"], "#79b3b3");
  assert.equal(v["--font-mono"], "Menlo, monospace");
  assert.deepEqual(
    theme.variables({ ...CONTEXT, colors: { ...CONTEXT.colors, background: "red" } } as never),
    {},
  );
});

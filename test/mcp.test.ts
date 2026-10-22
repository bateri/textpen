// The built program as an MCP client starts it (stdin not a terminal): what it offers a model
// and the view, its icon, and the view it serves, on a project of its own.

import { Client } from "@modelcontextprotocol/client";
import { StdioClientTransport } from "@modelcontextprotocol/client/stdio";
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { after, before, test } from "node:test";

const main = new URL("../dist/server/main.js", import.meta.url).pathname;
const manifest = JSON.parse(readFileSync(new URL("../package.json", import.meta.url), "utf8"));

let dir: string;
let client: Client;

before(async () => {
  dir = mkdtempSync(join(tmpdir(), "textpen-mcp-"));
  writeFileSync(join(dir, "notes.md"), "# Notes\n");
  client = new Client({ name: "textpen-test", version: "0.0.0" });
  await client.connect(
    new StdioClientTransport({
      command: process.execPath,
      args: [main],
      cwd: dir,
      env: {
        PATH: process.env.PATH ?? "",
        XDG_CONFIG_HOME: join(dir, ".config"),
        XDG_STATE_HOME: join(dir, ".state"),
      },
    }),
  );
});

after(async () => {
  await client?.close();
  rmSync(dir, { recursive: true, force: true });
});

test("says the manifest's version", () => {
  assert.equal(
    execFileSync(process.execPath, [main, "--version"], { encoding: "utf8" }).trim(),
    manifest.version,
  );
  assert.equal(client.getServerVersion()?.version, manifest.version);
});

test("offers the view and reading to the model, and keeps what writes for the view", async () => {
  const { tools } = await client.listTools();
  const byName = new Map(tools.map((tool) => [tool.name, tool]));
  for (const name of ["show_editor", "list_files", "read_file"]) {
    assert.ok(byName.has(name), `${name} is offered`);
  }
  assert.equal(byName.get("show_editor")?._meta?.ui?.resourceUri, "ui://textpen/view");
  for (const name of ["textpen_edit", "textpen_save", "textpen_create", "textpen_delete"]) {
    assert.deepEqual(byName.get(name)?._meta?.ui?.visibility, ["app"], `${name} is the view's`);
  }
});

test("reads a file of the project for the model", async () => {
  const result = await client.callTool({ name: "read_file", arguments: { path: "notes.md" } });
  assert.match(JSON.stringify(result.content), /# Notes/);
});

test("serves the view as one document", async () => {
  const { contents } = await client.readResource({ uri: "ui://textpen/view" });
  assert.equal(contents.length, 1);
  const text = (contents[0] as { text?: string }).text ?? "";
  assert.match(text, /<div id="root">/);
  assert.ok(text.length > 100_000, "scripts and styles are inside the document");
});

interface Icon {
  src: string;
  mimeType?: string;
  sizes?: string[];
  theme?: string;
}

/**
 * What a pencere terminal takes of an icon list (specification §7.3): a base64 PNG in a data:
 * URI, at most 64 KiB and 512 pixels a side, four of them at most; a picture for each ground,
 * at the 32 pixels a strip asks for.
 */
function checkIcons(icons: Icon[] | undefined, owner: string) {
  assert.ok(Array.isArray(icons) && icons.length > 0 && icons.length <= 4, `${owner} has icons`);
  for (const icon of icons) {
    const match = /^data:image\/png;base64,([A-Za-z0-9+/=]+)$/.exec(icon.src);
    assert.ok(match, `${owner}: an icon is a base64 PNG in a data: URI`);
    const bytes = Buffer.from(match[1]!, "base64");
    assert.deepEqual([...bytes.subarray(0, 8)], [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);
    assert.ok(bytes.length <= 64 * 1024, `${owner}: an icon is at most 64 KiB`);
    const [width, height] = [bytes.readUInt32BE(16), bytes.readUInt32BE(20)];
    assert.ok(width <= 512 && height <= 512, `${owner}: an icon is at most 512 pixels a side`);
    assert.equal(width, height, `${owner}: an icon is square`);
    assert.deepEqual(icon.sizes, [`${width}x${height}`]);
    assert.equal(icon.mimeType, "image/png");
  }
  for (const theme of ["dark", "light"]) {
    assert.ok(
      icons.some((icon) => icon.theme === theme && icon.sizes?.[0] === "32x32"),
      `${owner} has a 32-pixel icon for a ${theme} ground`,
    );
  }
}

test("gives its icon as a terminal takes one, for itself and for the view's tool", async () => {
  checkIcons(client.getServerVersion()?.icons, "the server");
  const { tools } = await client.listTools();
  checkIcons(tools.find((tool) => tool.name === "show_editor")?.icons, "show_editor");
});

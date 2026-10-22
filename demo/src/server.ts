import { createServer } from "node:http";

import { formatPrice, slugify } from "./lib/format.ts";
import { OrderBook } from "./lib/orders.ts";
import { products } from "./routes/products.ts";

const PORT = Number(process.env.PORT ?? 8080);
const book = new OrderBook();

/** Answers with JSON, and the status a client should act on. */
function send(res: import("node:http").ServerResponse, status: number, body: unknown) {
  res.writeHead(status, { "content-type": "application/json" });
  res.end(JSON.stringify(body));
}

const server = createServer(async (req, res) => {
  const url = new URL(req.url ?? "/", `http://${req.headers.host}`);

  if (req.method === "GET" && url.pathname === "/products") {
    return send(res, 200, products.map((p) => ({ ...p, slug: slugify(p.name) })));
  }

  if (req.method === "POST" && url.pathname === "/orders") {
    const order = await book.place(await readJson(req));
    console.log(`order ${order.id}: ${formatPrice(order.total)}`);
    return send(res, 201, order);
  }

  send(res, 404, { error: `no route for ${req.method} ${url.pathname}` });
});

async function readJson(req: import("node:http").IncomingMessage): Promise<unknown> {
  const chunks: Buffer[] = [];
  for await (const chunk of req) chunks.push(chunk as Buffer);
  return JSON.parse(Buffer.concat(chunks).toString("utf8"));
}

server.listen(PORT, () => console.log(`lantern on :${PORT}`));

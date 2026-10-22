import { randomUUID } from "node:crypto";

import { TAX_RATE } from "./format.ts";

export interface Line {
  sku: string;
  quantity: number;
  price: number;
}

export interface Order {
  id: string;
  lines: Line[];
  total: number;
  placed: Date;
}

export class OrderBook {
  readonly #orders = new Map<string, Order>();

  async place(input: unknown): Promise<Order> {
    const lines = parseLines(input);
    const subtotal = lines.reduce((sum, line) => sum + line.price * line.quantity, 0);
    const order: Order = {
      id: randomUUID(),
      lines,
      total: Math.round(subtotal * (1 + TAX_RATE)),
      placed: new Date(),
    };
    this.#orders.set(order.id, order);
    return order;
  }

  get size(): number {
    return this.#orders.size;
  }
}

function parseLines(input: unknown): Line[] {
  if (!Array.isArray(input)) throw new TypeError("an order is a list of lines");
  return input.map((line, index) => {
    if (typeof line?.sku !== "string" || !Number.isInteger(line?.quantity)) {
      throw new TypeError(`line ${index + 1} needs a sku and a whole quantity`);
    }
    return { sku: line.sku, quantity: line.quantity, price: Number(line.price) || 0 };
  });
}

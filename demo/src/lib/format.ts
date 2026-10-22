const currency = new Intl.NumberFormat("en-US", { style: "currency", currency: "USD" });

/** A price in cents, as people read it: 1999 → "$19.99". */
export function formatPrice(cents: number): string {
  return currency.format(cents / 100);
}

/** A name as a path segment: "Brass Lantern, Large" → "brass-lantern-large". */
export function slugify(name: string): string {
  return name
    .normalize("NFKD")
    .replace(/[\u0300-\u036f]/g, "")
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-|-$/g, "");
}

export const TAX_RATE = 0.08;

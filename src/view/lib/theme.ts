/**
 * The terminal's colours and font on the document: the host's `context`, as the variables
 * the stylesheet reads. The nine roles are the terminal's own; the steps between them (a
 * surface a little lighter than the background, a rule softer than the border) are made
 * here, by mixing, so that every colour of the editor is one of the terminal's or a step
 * from two of them.
 */

import type { Context } from "./page";

type Rgb = [number, number, number];

function parse(hex: string): Rgb | undefined {
  const match = /^#([0-9a-f]{6})$/i.exec(hex);
  if (!match) return undefined;
  const digits = match[1]!;
  return [0, 2, 4].map((i) => parseInt(digits.slice(i, i + 2), 16)) as Rgb;
}

function css([r, g, b]: Rgb): string {
  return `#${[r, g, b].map((c) => c.toString(16).padStart(2, "0")).join("")}`;
}

/** A colour `amount` of the way from `a` to `b`, each channel rounded to the nearest value. */
function toward(a: Rgb, b: Rgb, amount: number): Rgb {
  return a.map((c, i) => Math.round(c + (b[i]! - c) * amount)) as Rgb;
}

/** The variables the stylesheet's `:root` reads, for a terminal's context. */
export function variables(context: Context): Record<string, string> {
  const { colors } = context;
  const bg = parse(colors.background);
  const fg = parse(colors.foreground);
  // A host sends all nine roles; one that is not a colour leaves the stylesheet's own.
  if (bg === undefined || fg === undefined) return {};
  const dim = parse(colors.dim) ?? toward(fg, bg, 1 / 3);
  const border = parse(colors.border) ?? toward(bg, fg, 0.2);
  const accent = parse(colors.accent);
  const v: Record<string, string> = {
    "--color-background-primary": css(bg),
    "--color-background-secondary": css(toward(bg, fg, 0.06)),
    "--color-background-tertiary": css(toward(bg, fg, 0.12)),
    "--color-text-primary": css(fg),
    "--color-text-secondary": css(dim),
    "--color-text-tertiary": css(toward(dim, bg, 0.35)),
    "--color-text-inverse": css(bg),
    "--color-border-primary": css(border),
    "--color-border-secondary": css(toward(border, bg, 0.4)),
  };
  if (accent) v["--color-ring-primary"] = css(accent);
  const roles: [string, string][] = [
    ["info", colors.info],
    ["danger", colors.error],
    ["success", colors.success],
    ["warning", colors.warning],
  ];
  for (const [name, hex] of roles) {
    const role = parse(hex);
    if (role) v[`--color-text-${name}`] = css(role);
  }
  if (context.font) v["--font-mono"] = context.font;
  return v;
}

/** Puts the terminal's theme on the document; called again each time the host says it changed. */
export function applyContext(context: Context): void {
  const root = document.documentElement;
  root.setAttribute("data-theme", context.mode);
  root.style.colorScheme = context.mode;
  for (const [name, value] of Object.entries(variables(context))) {
    root.style.setProperty(name, value);
  }
}

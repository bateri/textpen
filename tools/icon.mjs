// Draws textpen's icon, its mark (a monospace "t" with a terminal's block cursor after it: the name,
// being typed), as PNGs, and writes them to src/assets/icon-<side>-<theme>.png. A terminal draws a
// program's icon only from the bytes it is given, so the program carries them (src/program/main.rs
// names the files); run this again after changing the shapes here.
//
//   node tools/icon.mjs
//
// It uses nothing but Node: the shapes are rounded rectangles and strokes with round ends, filled
// with coverage taken from a grid of samples in each pixel, and the PNG is written by hand around
// zlib.

import { writeFileSync } from "node:fs";
import { deflateSync, crc32 } from "node:zlib";

/** A rounded rectangle: x, y, width, height, corner radius. */
const rect = (x, y, w, h, r) => ({ kind: "rect", x, y, w, h, r });
/** A straight stroke through points, with round ends and joins. */
const line = (points, width) => ({ kind: "line", points, width });
/** A stroke along a circle's arc, from one angle to another clockwise (degrees, y pointing down). */
const arc = (cx, cy, radius, from, to, width) => ({ kind: "arc", cx, cy, radius, from, to, width });

/**
 * The mark on a 16-unit grid, two pixels a unit at 32 and four at 64, so that its straight edges
 * fall on whole pixels. Each shape is the union of its parts, drawn with its opacity.
 */
const MARK = [
  {
    // The "t": its stem, curling to the right at the foot, and its bar.
    parts: [
      line([[5.5, 2.5], [5.5, 10]], 2),
      arc(8.5, 10, 3, 90, 180, 2),
      line([[8.5, 13], [9, 13]], 2),
      line([[3, 6], [9, 6]], 2),
    ],
    opacity: 1,
  },
  // The cursor, a cell wide, standing on the line the "t" stands on.
  { parts: [rect(11, 8, 3.5, 6, 1)], opacity: 1 },
];

/** The part of the grid an icon shows: all of it (the mark spans x 2–14.5, y 1.5–14). */
const FRAME = { x: 0, y: 0, side: 16 };

/** One colour for each ground, as an icon's `theme` says: the ground the icon is made to stand on. */
const COLOURS = {
  dark: [0xf0, 0xb2, 0x5a],
  light: [0xa8, 0x5c, 0x0a],
};

/** 32 pixels is what a terminal's strip asks for on a Retina screen; 64 for those that draw larger. */
const SIDES = [32, 64];

const SAMPLES = 8;

function distanceToSegment(px, py, [ax, ay], [bx, by]) {
  const [vx, vy] = [bx - ax, by - ay];
  const along = ((px - ax) * vx + (py - ay) * vy) / (vx * vx + vy * vy || 1);
  const t = Math.min(Math.max(along, 0), 1);
  return Math.hypot(px - (ax + t * vx), py - (ay + t * vy));
}

const INSIDE = {
  rect(px, py, { x, y, w, h, r }) {
    const dx = Math.max(Math.abs(px - (x + w / 2)) - (w / 2 - r), 0);
    const dy = Math.max(Math.abs(py - (y + h / 2)) - (h / 2 - r), 0);
    return dx * dx + dy * dy <= r * r;
  },
  line(px, py, { points, width }) {
    return points
      .slice(1)
      .some((to, i) => distanceToSegment(px, py, points[i], to) <= width / 2);
  },
  arc(px, py, { cx, cy, radius, from, to, width }) {
    const turn = (degrees) => ((degrees % 360) + 360) % 360;
    const angle = (Math.atan2(py - cy, px - cx) * 180) / Math.PI;
    if (turn(angle - from) <= turn(to - from)) {
      return Math.abs(Math.hypot(px - cx, py - cy) - radius) <= width / 2;
    }
    // Beyond its ends, the arc's round caps.
    const end = (degrees) => {
      const radians = (degrees * Math.PI) / 180;
      return Math.hypot(px - (cx + radius * Math.cos(radians)), py - (cy + radius * Math.sin(radians)));
    };
    return Math.min(end(from), end(to)) <= width / 2;
  },
};

const inside = (px, py, part) => INSIDE[part.kind](px, py, part);

function draw(side, [red, green, blue]) {
  const scale = FRAME.side / side;
  const rows = [];
  for (let row = 0; row < side; row++) {
    const line = Buffer.alloc(1 + side * 4); // a leading 0: no filter
    for (let column = 0; column < side; column++) {
      // The shapes are drawn one over another, each with its opacity.
      let alpha = 0;
      for (const shape of MARK) {
        let hits = 0;
        for (let sy = 0; sy < SAMPLES; sy++) {
          for (let sx = 0; sx < SAMPLES; sx++) {
            const px = FRAME.x + (column + (sx + 0.5) / SAMPLES) * scale;
            const py = FRAME.y + (row + (sy + 0.5) / SAMPLES) * scale;
            if (shape.parts.some((part) => inside(px, py, part))) hits++;
          }
        }
        const cover = (hits / (SAMPLES * SAMPLES)) * shape.opacity;
        alpha = cover + alpha * (1 - cover);
      }
      line.set([red, green, blue, Math.round(alpha * 255)], 1 + column * 4);
    }
    rows.push(line);
  }
  return png(side, Buffer.concat(rows));
}

function chunk(type, data) {
  const length = Buffer.alloc(4);
  length.writeUInt32BE(data.length);
  const body = Buffer.concat([Buffer.from(type, "ascii"), data]);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(body));
  return Buffer.concat([length, body, crc]);
}

function png(side, pixels) {
  const header = Buffer.alloc(13);
  header.writeUInt32BE(side, 0);
  header.writeUInt32BE(side, 4);
  header.set([8, 6, 0, 0, 0], 8); // 8 bits, RGBA, deflate, no filter method, no interlace
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk("IHDR", header),
    chunk("IDAT", deflateSync(pixels, { level: 9 })),
    chunk("IEND", Buffer.alloc(0)),
  ]);
}

let written = 0;
for (const [theme, colour] of Object.entries(COLOURS)) {
  for (const side of SIDES) {
    writeFileSync(
      new URL(`../src/assets/icon-${side}-${theme}.png`, import.meta.url),
      draw(side, colour),
    );
    written++;
  }
}
console.log(`wrote ${written} icons to src/assets`);

// Draws textpen's icon, its mark (the Mark in src/view/components/Welcome.tsx: lines of text,
// the last one being written, with the cursor at its end), as PNGs in data: URIs, and writes
// them to src/server/icon.ts. A terminal draws a program's icon only from the bytes it is given,
// so the server carries them; run this again after changing the shapes here and there.
//
//   node tools/icon.mjs
//
// It uses nothing but Node: the shapes are rounded rectangles, filled with coverage taken
// from a grid of samples in each pixel, and the PNG is written by hand around zlib.

import { writeFileSync } from "node:fs";
import { deflateSync, crc32 } from "node:zlib";

/** The mark in its 24-unit box: x, y, width, height, corner radius, opacity. */
const MARK = [
  [3, 3.8, 18, 3, 1.5, 0.4],
  [6.5, 9.8, 11.5, 3, 1.5, 0.7],
  [3, 15.8, 6, 3, 1.5, 1],
  [10.6, 14.3, 2.2, 6, 1.1, 1],
];

/** The part of that box an icon shows: the mark (x 3–21, y 3.8–20.3) with a unit around it. */
const FRAME = { x: 2, y: 2, side: 20 };

/** One colour for each ground, as an icon's `theme` says: the ground the icon is made to stand on. */
const COLOURS = {
  dark: [0xf0, 0xb2, 0x5a],
  light: [0xa8, 0x5c, 0x0a],
};

/** 32 pixels is what a terminal's strip asks for on a Retina screen; 64 for those that draw larger. */
const SIDES = [32, 64];

const SAMPLES = 8;

function inside(px, py, [x, y, w, h, r]) {
  const dx = Math.max(Math.abs(px - (x + w / 2)) - (w / 2 - r), 0);
  const dy = Math.max(Math.abs(py - (y + h / 2)) - (h / 2 - r), 0);
  return dx * dx + dy * dy <= r * r;
}

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
            if (inside(px, py, shape)) hits++;
          }
        }
        const cover = (hits / (SAMPLES * SAMPLES)) * shape[5];
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

const icons = [];
for (const [theme, colour] of Object.entries(COLOURS)) {
  for (const side of SIDES) {
    const src = `data:image/png;base64,${draw(side, colour).toString("base64")}`;
    icons.push(
      `  {\n    src: "${src}",\n    mimeType: "image/png",\n    sizes: ["${side}x${side}"],\n    theme: "${theme}",\n  },`,
    );
  }
}

const path = new URL("../src/server/icon.ts", import.meta.url);
writeFileSync(
  path,
  `// Written by tools/icon.mjs; change the shapes there and run it again.

import type { Icon } from "@pencere/sdk";

/**
 * textpen's icon, the mark its view shows, for a dark ground and a light one (an icon's \`theme\`),
 * 32 and 64 pixels a side, as PNGs in \`data:\` URIs: a terminal draws an icon only from the
 * bytes it is given, and never fetches one.
 */
export const ICONS: Icon[] = [
${icons.join("\n")}
];
`,
);
console.log(`wrote ${icons.length} icons to src/server/icon.ts`);

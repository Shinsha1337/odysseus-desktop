// Renders the official Odysseus boat logo (see app-icon.svg) to a 1024x1024
// RGBA PNG with zero dependencies (pure Node + zlib), then `npm run icons`
// feeds it to `tauri icon` to produce the full .ico / .icns / png set.
//
// The logo geometry is the favicon used across static/*.html:
//   main sail: triangle (16,4)-(16,22)-(6,22)
//   jib:       triangle (16,8)-(16,22)-(24,22)  @ 60% opacity
//   wave:      M4 24 Q10 20 16 24 Q22 28 28 24, stroke-width 2.5, round caps
// drawn in the brand red #e06c75 on a transparent background.
const fs = require("fs");
const zlib = require("zlib");

const SIZE = 1024;
const S = SIZE / 32; // viewBox (0..32) -> pixels
const W = SIZE,
  H = SIZE;
const RED = [224, 108, 117]; // #e06c75

// RGBA framebuffer with a leading filter byte per PNG scanline.
const ROW = 1 + W * 4;
const buf = Buffer.alloc(H * ROW); // zero-filled => transparent

function setPx(x, y, r, g, b, a) {
  if (x < 0 || y < 0 || x >= W || y >= H) return;
  const i = y * ROW + 1 + x * 4;
  buf[i] = r;
  buf[i + 1] = g;
  buf[i + 2] = b;
  buf[i + 3] = a;
}

// --- triangle fill (edge-sign test) ---
function edge(px, py, x0, y0, x1, y1) {
  return (px - x0) * (y1 - y0) - (py - y0) * (x1 - x0);
}
function fillTri(ax, ay, bx, by, cx, cy, alpha) {
  ax *= S; ay *= S; bx *= S; by *= S; cx *= S; cy *= S;
  const minX = Math.max(0, Math.floor(Math.min(ax, bx, cx)));
  const maxX = Math.min(W - 1, Math.ceil(Math.max(ax, bx, cx)));
  const minY = Math.max(0, Math.floor(Math.min(ay, by, cy)));
  const maxY = Math.min(H - 1, Math.ceil(Math.max(ay, by, cy)));
  for (let y = minY; y <= maxY; y++) {
    for (let x = minX; x <= maxX; x++) {
      const d1 = edge(x, y, ax, ay, bx, by);
      const d2 = edge(x, y, bx, by, cx, cy);
      const d3 = edge(x, y, cx, cy, ax, ay);
      const hasNeg = d1 < 0 || d2 < 0 || d3 < 0;
      const hasPos = d1 > 0 || d2 > 0 || d3 > 0;
      if (!(hasNeg && hasPos)) setPx(x, y, RED[0], RED[1], RED[2], alpha);
    }
  }
}

// --- thick polyline stroke via overlapping discs (gives round caps/joins) ---
function disc(cx, cy, r) {
  const minX = Math.max(0, Math.floor(cx - r));
  const maxX = Math.min(W - 1, Math.ceil(cx + r));
  const minY = Math.max(0, Math.floor(cy - r));
  const maxY = Math.min(H - 1, Math.ceil(cy + r));
  const r2 = r * r;
  for (let y = minY; y <= maxY; y++) {
    for (let x = minX; x <= maxX; x++) {
      const dx = x - cx,
        dy = y - cy;
      if (dx * dx + dy * dy <= r2) setPx(x, y, RED[0], RED[1], RED[2], 255);
    }
  }
}
function quad(p0, p1, p2, r) {
  for (let t = 0; t <= 1; t += 0.001) {
    const u = 1 - t;
    const x = (u * u * p0[0] + 2 * u * t * p1[0] + t * t * p2[0]) * S;
    const y = (u * u * p0[1] + 2 * u * t * p1[1] + t * t * p2[1]) * S;
    disc(x, y, r);
  }
}

// Paint order: jib (translucent) first, then main sail (opaque) overwrites the
// shared mast edge, then the wave on top.
fillTri(16, 8, 16, 22, 24, 22, Math.round(0.6 * 255)); // jib
fillTri(16, 4, 16, 22, 6, 22, 255); // main sail
quad([4, 24], [10, 20], [16, 24], 1.25 * S); // wave, half of stroke-width 2.5
quad([16, 24], [22, 28], [28, 24], 1.25 * S);

// --- PNG encode ---
const crcTable = (() => {
  const t = [];
  for (let n = 0; n < 256; n++) {
    let c = n;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    t[n] = c >>> 0;
  }
  return t;
})();
function crc32(b) {
  let c = 0xffffffff;
  for (let i = 0; i < b.length; i++) c = crcTable[(c ^ b[i]) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
}
function chunk(type, data) {
  const len = Buffer.alloc(4);
  len.writeUInt32BE(data.length, 0);
  const t = Buffer.from(type, "ascii");
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(Buffer.concat([t, data])), 0);
  return Buffer.concat([len, t, data, crc]);
}

const sig = Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);
const ihdr = Buffer.alloc(13);
ihdr.writeUInt32BE(W, 0);
ihdr.writeUInt32BE(H, 4);
ihdr[8] = 8; // bit depth
ihdr[9] = 6; // color type RGBA
const png = Buffer.concat([
  sig,
  chunk("IHDR", ihdr),
  chunk("IDAT", zlib.deflateSync(buf, { level: 9 })),
  chunk("IEND", Buffer.alloc(0)),
]);
fs.writeFileSync("app-icon.png", png);
console.log("wrote app-icon.png (" + png.length + " bytes) from the Odysseus boat logo");

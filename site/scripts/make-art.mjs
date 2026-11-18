// Generator for the site-only art in src/art/ (hero + FAQ). Run: node scripts/make-art.mjs
// Base: the resting look-left-right frame 1 from assets/illustrations/ocy-v1f (read only).
// Output per theme: 3 "line boil" variants (v1 = the original drawing, v2/v3 jitter every outline point by
// 0.3-0.7 units), each with an eye group (.ocy-eyes) and a closed-eye group (.ocy-blink, hidden by CSS).
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
const here = path.dirname(fileURLToPath(import.meta.url));
const src = path.resolve(here, '../../assets/illustrations/ocy-v1f/svg');
const out = path.resolve(here, '../src/art');
const INK = '#1B1E1F';
const ACCENT = { light: '#2F7D62', dark: '#5FBF97' };

function rng(seed) {
  let s = seed >>> 0;
  return () => ((s = (s * 1664525 + 1013904223) >>> 0) / 4294967296);
}
const eyeRe = /<path d="M(?:23\.64|38\.91)[^>]*fill="#1B1E1F"\/>/g;

function jitter(svg, seed) {
  if (!seed) return svg;
  const r = rng(seed);
  const j = (n) => {
    const v = parseFloat(n) + (r() < 0.5 ? -1 : 1) * (0.3 + r() * 0.4);
    return String(Math.round(v * 100) / 100);
  };
  return svg
    .replace(/ d="([^"]+)"/g, (_, d) => ` d="${d.replace(/-?\d+(?:\.\d+)?/g, j)}"`)
    .replace(/ c([xy])="([^"]+)"/g, (_, a, v) => ` c${a}="${j(v)}"`);
}

function parts(theme) {
  let b = fs.readFileSync(path.join(src, `look-left-right-${theme}-f1.svg`), 'utf8');
  b = b.replace(/<path d="M6\.17[^>]*\/>|<path d="M5\.83[^>]*\/>/g, ''); // side ticks of the look frame
  const eyes = b.match(eyeRe);
  if (!eyes || eyes.length !== 2) throw new Error('eye paths not found');
  const body = b.replace(eyeRe, '').replace(/^<svg[^>]*>/, '').replace('</svg>', '')
    // mark the head outline so the page can find the face centre
    .replace('<path d="M14.01', '<path class="ocy-head" d="M14.01');
  return { body, eyes: eyes.join('') };
}
// closed eyes, in the style of the blink exercise (soft arcs + a small lash)
const closed =
  `<path d="M19.2 37.6 Q21.5 39.9 23.8 37.4" fill="none" stroke="${INK}" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round"/>` +
  `<path d="M34.7 37.1 Q36.9 39.4 39.1 36.9" fill="none" stroke="${INK}" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round"/>`;

// four different hand-drawn question marks, each centred on 0,0 (curl path, dot, stroke width)
const QS = [
  ['M-2.6 -2.4 C-2.9 -5 -0.4 -6.2 1.3 -5.6 C3.2 -4.9 3.3 -2.6 2 -1.5 C0.9 -0.6 0.2 -0.1 0.3 1.6', [0.4, 4.2], 2.1],
  ['M-1.6 -3.4 C-1.4 -5.6 1.8 -5.9 2.1 -3.8 C2.4 -2.1 0.1 -1.6 -0.1 0.4 L-0.05 1.8', [0, 4.6], 1.7],
  ['M-3 -1.8 C-2.6 -4.6 2.6 -4.8 2.9 -2.3 C3.1 -0.6 0.4 -0.9 0.5 1.2', [0.6, 3.6], 2.3],
  ['M-2 -2.9 C-2.2 -4.6 -0.2 -5.3 1.2 -4.9 C2.4 -4.5 2.6 -3.3 1.7 -2.4 C1 -1.7 0.6 -1.6 0.4 -0.6 C0.3 0 0.5 0.8 0.3 1.4', [0.35, 3.9], 1.9],
];
// x, y, tilt, scale
const PLACES = [[55, 17, 12, 1.15], [8, 19, -14, 1], [51, 5, 6, 0.85], [14, 6, -8, 0.78]];
function marks(color, seed) {
  return PLACES.map(([x, y, rot, s], i) => {
    const [d, [dx, dy], w] = QS[i];
    return `<g transform="translate(${x} ${y}) rotate(${rot}) scale(${s})"><g class="ocy-q" data-q="${i}">` +
      `<path d="${seed ? jitter(` d="${d}"`, seed + i).slice(4, -1) : d}" fill="none" stroke="${color}" stroke-width="${w}" stroke-linecap="round" stroke-linejoin="round"/>` +
      `<path d="M${dx} ${dy} l0.12 0.1" fill="none" stroke="${color}" stroke-width="${w + 0.5}" stroke-linecap="round"/></g></g>`;
  }).join('');
}

for (const d of ['hero', 'faq']) {
  fs.mkdirSync(path.join(out, d), { recursive: true });
  for (const f of fs.readdirSync(path.join(out, d))) fs.unlinkSync(path.join(out, d, f));
}
for (const theme of ['light', 'dark']) {
  const { body, eyes } = parts(theme);
  [0, 7, 23].forEach((seed, i) => {
    const face = jitter(body, seed);
    const eyeG = `<g class="ocy-eyes">${jitter(eyes, seed && seed + 1)}</g><g class="ocy-blink">${jitter(closed, seed && seed + 2)}</g>`;
    fs.writeFileSync(path.join(out, 'hero', `ocy-hero-${theme}-v${i + 1}.svg`),
      `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64" width="64" height="64">${face}${eyeG}</svg>`);
    // FAQ: padded viewBox so the top marks are never clipped, eyes looking up
    const qs = marks(ACCENT[theme], seed && seed + 3);
    fs.writeFileSync(path.join(out, 'faq', `ocy-faq-${theme}-v${i + 1}.svg`),
      `<svg xmlns="http://www.w3.org/2000/svg" viewBox="-6 -8 76 76" width="76" height="76" overflow="visible">${face}` +
      `<g transform="translate(0.6 -1.6)">${eyeG}</g>${qs}</svg>`);
  });
}
console.log('art written to', out);

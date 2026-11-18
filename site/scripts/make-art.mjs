// One-off generator for the site-only art in src/art/ (hero eye-follow + FAQ question marks).
// Base: the resting look-left-right frame 1 from assets/illustrations/ocy-v1f. Run: node scripts/make-art.mjs
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
const here = path.dirname(fileURLToPath(import.meta.url));
const src = path.resolve(here, '../../assets/illustrations/ocy-v1f/svg');
const out = path.resolve(here, '../src/art');
const INK = '#1B1E1F';
const ACCENT = { light: '#2F7D62', dark: '#5FBF97' };

function base(theme) {
  return fs.readFileSync(path.join(src, `look-left-right-${theme}-f1.svg`), 'utf8');
}
// the two eye dots are the filled ink paths starting at M23.64 / M38.91
const eyeRe = /<path d="M(?:23\.64|38\.91)[^>]*fill="#1B1E1F"\/>/g;

function withEyes(svg, wrap) {
  const eyes = svg.match(eyeRe);
  if (!eyes || eyes.length !== 2) throw new Error('eye paths not found');
  let s = svg.replace(eyeRe, '');
  return s.replace('</svg>', wrap(eyes.join('')) + '</svg>');
}

// hand-drawn question mark, ~7 units tall, centred on 0,0
const q = (x, y, r, s, color) =>
  `<g transform="translate(${x} ${y}) rotate(${r}) scale(${s})">` +
  `<path d="M-2.3 -2.6 C-2.4 -4.7 -0.6 -5.6 0.9 -5.3 C2.6 -4.9 3.1 -3.2 2.3 -2 C1.6 -1 0.2 -0.6 0.1 0.9 L0.15 1.5" fill="none" stroke="${color}" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>` +
  `<path d="M0.1 4.1 l0.12 0.08" fill="none" stroke="${color}" stroke-width="2.4" stroke-linecap="round"/></g>`;

const marks = [
  [55, 16, 12, 1.15],
  [8, 18, -14, 1],
  [50, 5, 6, 0.85],
  [15, 6, -6, 0.8],
];

for (const theme of ['light', 'dark']) {
  const b = base(theme);
  fs.mkdirSync(path.join(out, 'hero'), { recursive: true });
  fs.mkdirSync(path.join(out, 'faq'), { recursive: true });
  // hero: eyes as their own group, the side ticks of the look frame removed (static face)
  const calm = b.replace(/<path d="M6\.17[^>]*\/>|<path d="M5\.83[^>]*\/>/g, '');
  fs.writeFileSync(path.join(out, 'hero', `ocy-hero-${theme}.svg`), withEyes(calm, (e) => `<g class="ocy-eyes">${e}</g>`));
  // faq: eyes looking up, question marks popping in one by one (frame 1 = none)
  for (let f = 1; f <= marks.length + 1; f++) {
    const qs = marks.slice(0, f - 1).map(([x, y, r, s]) => q(x, y, r, s, ACCENT[theme])).join('');
    const svg = withEyes(calm, (e) => `<g transform="translate(0.6 -1.6)">${e}</g>${qs}`);
    fs.writeFileSync(path.join(out, 'faq', `ocy-faq-${theme}-f${f}.svg`), svg);
  }
}
console.log('art written to', out);

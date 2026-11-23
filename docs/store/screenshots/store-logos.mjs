// Renders the Ocy Store logo art (Partner Center > Store logos) in the slide brand look:
// box art 1:1 (2160 + 1080), poster art 9:16 (1440x2160 + 720x1080) and the white-tile logo mark (1080, transparent).
//
//   NODE_PATH="<a node_modules folder that has playwright>" node docs/store/screenshots/store-logos.mjs --out <dir>
import { createRequire } from 'node:module';
import { readFileSync, mkdirSync, existsSync } from 'node:fs';
import { dirname, resolve, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const require = createRequire(join(process.env.NODE_PATH?.split(':')[0] || process.cwd(), 'x.js'));
const { chromium } = require('playwright');
const here = dirname(fileURLToPath(import.meta.url));
const arg = (n, d) => { const i = process.argv.indexOf(n); return i > 0 ? process.argv[i + 1] : d; };
const out = resolve(arg('--out', join(here, '../../../../../builds/store-0.2.2/store-logos')));
mkdirSync(out, { recursive: true });

const { heroChar } = JSON.parse(readFileSync(join(here, 'slides.json'), 'utf8'));
const G = heroChar.backgrounds.G;
// The Ocy logo = the sprout on the white rounded tile (as on the slides). Inline SVG (not <img>) so Chromium draws
// the 32-unit art as vector at full size, without faceted curves. Tile proportions: radius 27%, sprout 73%.
const sproutSvg = readFileSync(join(here, 'sprout-light-dark-stem.svg'), 'utf8');
const tile = (px, shadow = '') => `<div style="width:${px}px;height:${px}px;border-radius:${Math.round(px * 0.27)}px;background:#F2F7F4;display:grid;place-items:center;${shadow}">${sproutSvg.replace(/width="32" height="32"/, `width="${Math.round(px * 0.73)}" height="${Math.round(px * 0.73)}"`)}</div>`;
const lift = 'box-shadow:0 24px 60px rgba(8,36,26,.28)';
const W = '/mnt/c/Windows/Fonts';
const font = existsSync(join(W, 'seguisb.ttf')) ? `@font-face{font-family:"Ocy Segoe UI";font-weight:600;src:url(${pathToFileURL(join(W, 'seguisb.ttf')).href})}
  @font-face{font-family:"Ocy Segoe UI";font-weight:400;src:url(${pathToFileURL(join(W, 'segoeui.ttf')).href})}` : '';
const base = `${font} html,body{margin:0} body{font-family:"Ocy Segoe UI","Segoe UI","Ubuntu Sans",sans-serif;color:#F2F5F3}
  .c{position:absolute;inset:0;display:flex;flex-direction:column;align-items:center;justify-content:center}
  .c img{display:block;filter:drop-shadow(0 24px 40px rgba(10,40,30,.30))}
  .name{font-weight:600;letter-spacing:-0.01em;line-height:1}
  .line{font-weight:400;color:#F2F7F4;line-height:1.25;text-align:center}`;

// [file stem, css width, css height, html]
const jobs = [
  ['box-art', 1080, 1080, `<div class="c" style="background:${G}">${tile(440, lift)}
     <div class="name" style="font-size:150px;margin-top:64px">Ocy</div></div>`],
  ['poster-art', 720, 1080, `<div class="c" style="background:${G}">${tile(340, lift)}
     <div class="name" style="font-size:132px;margin-top:56px">Ocy</div>
     <div class="line" style="font-size:40px;margin-top:22px;max-width:520px">Gentle breaks for your eyes</div></div>`],
];
const browser = await chromium.launch({ args: ['--allow-file-access-from-files'] });
const ready = p => p.evaluate(() => Promise.all([...document.images].map(i => i.decode().catch(() => {}))).then(() => document.fonts.ready));
for (const [stem, w, h, html] of jobs) for (const s of [2, 1]) {
  const page = await browser.newPage({ viewport: { width: w, height: h }, deviceScaleFactor: s });
  await page.goto(pathToFileURL(join(here, 'template.html')).href);
  await page.evaluate(([c, b]) => { document.head.innerHTML = `<style>${c}</style>`; document.body.innerHTML = b; }, [base, html]);
  await ready(page);
  const file = join(out, `${stem}-${w * s}x${h * s}.png`);
  await page.screenshot({ path: file }); console.log(file); await page.close();
}
// Logo mark and app tiles: the tile fills the square, transparent outside its rounded corners.
for (const [name, px] of [['logo-mark-1080', 1080], ['app-tile-300x300', 300], ['app-tile-150x150', 150], ['app-tile-71x71', 71]]) {
  const page = await browser.newPage({ viewport: { width: px, height: px } });
  await page.goto(pathToFileURL(join(here, 'template.html')).href);
  await page.evaluate(([h]) => { document.head.innerHTML = '<style>html,body{margin:0;background:transparent;overflow:hidden}</style>'; document.body.innerHTML = h; }, [tile(px)]);
  await ready(page);
  const file = join(out, `${name}.png`);
  await page.screenshot({ path: file, omitBackground: true }); console.log(file); await page.close();
}
await browser.close();

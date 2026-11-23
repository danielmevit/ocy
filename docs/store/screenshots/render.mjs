// Renders the Ocy Store slides: template.html + slides.json -> 3840x2160 and 1920x1080 PNGs + contact-sheet.png.
//
//   node docs/store/screenshots/render.mjs --caps <captures dir> --out <output dir> [--only 03-long-break]
//
// Captures come from `cargo run -p ocy-ui --example readme_screens -- <caps>` plus `... -- <caps>/games games`.
// Playwright is not a repo dependency: point NODE_PATH at any node_modules that has it (>= 1.40), e.g.
//   NODE_PATH="<a node_modules folder that has playwright>" node docs/store/screenshots/render.mjs ...
import { createRequire } from 'node:module';
import { readFileSync, mkdirSync, existsSync } from 'node:fs';
import { dirname, resolve, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const require = createRequire(join(process.env.NODE_PATH?.split(':')[0] || process.cwd(), 'x.js'));
const { chromium } = require('playwright');

const here = dirname(fileURLToPath(import.meta.url));
const repo = resolve(here, '../../..');
const arg = (name, def) => { const i = process.argv.indexOf(name); return i > 0 ? process.argv[i + 1] : def; };
const caps = resolve(arg('--caps', join(repo, '../../builds/store-0.2.2/captures')));
const out = resolve(arg('--out', join(repo, '../../builds/store-0.2.2/screenshots')));
const only = arg('--only');
mkdirSync(out, { recursive: true });

const url = p => pathToFileURL(p.replace(/^caps\//, caps + '/').replace(/^repo\//, repo + '/')).href;
const { slides, heroVariants = [], heroVariants2 = [], heroChar = null, heroRight = null } = JSON.parse(readFileSync(join(here, 'slides.json'), 'utf8'));
for (const s of slides) {
  if (s.brand) s.brand = url(s.brand);
  for (const sh of s.shots || []) sh.src = url(sh.src);
  for (const a of s.art || []) a.src = url(a.src);
  if (s.strip) s.strip.files = s.strip.files.map(url);
}
for (const set of [heroChar, heroRight]) if (set) for (const c of Object.values(set.compositions)) {
  for (const a of c.art || []) a.src = url(a.src);
  for (const sh of c.shots || []) sh.src = url(sh.src);
}

// The app's font on Windows; used when the Windows fonts are reachable (WSL), else the CSS fallbacks.
const winFonts = '/mnt/c/Windows/Fonts';
const fontCss = existsSync(join(winFonts, 'segoeui.ttf')) ? `
  @font-face { font-family: "Ocy Segoe UI"; font-weight: 400; src: url(${pathToFileURL(join(winFonts, 'segoeui.ttf')).href}); }
  @font-face { font-family: "Ocy Segoe UI"; font-weight: 600; src: url(${pathToFileURL(join(winFonts, 'seguisb.ttf')).href}); }
  @font-face { font-family: "Ocy Segoe UI"; font-weight: 700; src: url(${pathToFileURL(join(winFonts, 'segoeuib.ttf')).href}); }` : '';

const browser = await chromium.launch({ args: ['--allow-file-access-from-files'] });

// --hero-variants: render the hero with each heroVariants[].bg into <out>/hero-variants/, plus a labelled sheet.
// Apply a chosen variant to every slide by giving each slide that "bg".
// --hero-char: the character hero, every composition x background, plus a labelled grid sheet.
const rightMode = process.argv.includes('--hero-right');
if (rightMode || process.argv.includes('--hero-char')) {
  const set = rightMode ? heroRight : heroChar;
  const dir = join(out, 'hero-variants'); mkdirSync(dir, { recursive: true });
  const page = await browser.newPage({ viewport: { width: 1920, height: 1080 } });
  await page.goto(pathToFileURL(join(here, 'template.html')).href);
  if (fontCss) await page.addStyleTag({ content: fontCss });
  const { compositions, backgrounds } = set; const copy = heroChar.copy;
  const rows = [];
  for (const [bk, bg] of Object.entries(backgrounds)) {
    const row = [];
    for (const [ck, c] of Object.entries(compositions)) {
      if (only && ck !== only) continue;
      const slide = { ...slides[0], ...copy, ...c, bg, center: true };
      slide.brand = url(copy.brand);
      await page.evaluate(x => window.renderSlide(x), slide);
      await page.evaluate(() => Promise.all([...document.images].map(i => i.decode().catch(() => {}))).then(() => document.fonts.ready).then(() => window.centerSlide()));
      const file = join(dir, rightMode ? `hero-right-${ck}.png` : `hero-char-${ck}-${bk}.png`); await page.screenshot({ path: file });
      if (c.hires) { // also a 3840x2160 copy
        const p2 = await browser.newPage({ viewport: { width: 1920, height: 1080 }, deviceScaleFactor: 2 });
        await p2.goto(pathToFileURL(join(here, 'template.html')).href);
        if (fontCss) await p2.addStyleTag({ content: fontCss });
        await p2.evaluate(x => window.renderSlide(x), slide);
        await p2.evaluate(() => Promise.all([...document.images].map(i => i.decode().catch(() => {}))).then(() => document.fonts.ready).then(() => window.centerSlide()));
        await p2.screenshot({ path: file.replace('.png', '-3840x2160.png') }); await p2.close();
      } row.push([rightMode ? `${ck}  ${c.label || ''}` : `${ck} / ${bk}`, file]); console.log(file);
    }
    rows.push(row);
  }
  const cells = rows.flat().map(([t, f]) => `<figure><img src="${pathToFileURL(f).href}"><figcaption>${t}</figcaption></figure>`).join('');
  const sheet = await browser.newPage({ viewport: { width: 1700, height: 100 } });
  await sheet.goto(pathToFileURL(join(here, 'template.html')).href);
  await sheet.evaluate(h => { document.documentElement.innerHTML = h; }, `<style>body{margin:0;padding:20px;background:#1d2120;font:20px sans-serif;color:#dfe7e3;display:grid;grid-template-columns:repeat(3,540px);gap:20px}
    figure{margin:0}img{width:540px;display:block}figcaption{padding-top:6px}</style>${cells}`);
  await sheet.evaluate(() => Promise.all([...document.images].map(i => i.decode())));
  await sheet.screenshot({ path: join(dir, rightMode ? 'hero-variants-4.png' : 'hero-variants-3.png'), fullPage: true });
  console.log(join(dir, rightMode ? 'hero-variants-4.png' : 'hero-variants-3.png'));
  await browser.close(); process.exit(0);
}
const v2 = process.argv.includes('--hero-variants-2');
if (v2 || process.argv.includes('--hero-variants')) {
  const dir = join(out, 'hero-variants'); mkdirSync(dir, { recursive: true });
  const page = await browser.newPage({ viewport: { width: 1920, height: 1080 } });
  await page.goto(pathToFileURL(join(here, 'template.html')).href);
  if (fontCss) await page.addStyleTag({ content: fontCss });
  const files = [];
  for (const v of (v2 ? heroVariants2 : heroVariants)) {
    await page.evaluate(x => window.renderSlide(x), { ...slides[0], bg: v.bg });
    await page.evaluate(() => Promise.all([...document.images].map(i => i.decode().catch(() => {}))).then(() => document.fonts.ready).then(() => window.centerSlide && window.centerSlide()));
    const file = join(dir, `hero-${v.id}.png`); await page.screenshot({ path: file }); files.push([v, file]); console.log(file);
  }
  const esc = t => t.replace(/&/g, '&amp;').replace(/</g, '&lt;');
  const cells = files.map(([v, f]) => `<figure><img src="${pathToFileURL(f).href}"><figcaption><b>${v.id}</b> ${esc(v.desc)}</figcaption></figure>`).join('');
  const sheet = await browser.newPage({ viewport: { width: 1660, height: 100 } });
  await sheet.goto(pathToFileURL(join(here, 'template.html')).href);
  await sheet.evaluate(h => { document.documentElement.innerHTML = h; }, `<style>body{margin:0;padding:24px;background:#1d2120;font:17px sans-serif;color:#dfe7e3;display:grid;grid-template-columns:repeat(2,794px);gap:24px}
    figure{margin:0}img{width:794px;display:block}figcaption{padding-top:8px}b{font-size:22px;margin-right:8px}</style>${cells}`);
  await sheet.evaluate(() => Promise.all([...document.images].map(i => i.decode())));
  await sheet.screenshot({ path: join(dir, v2 ? 'hero-variants-2.png' : 'hero-variants.png'), fullPage: true });
  console.log(join(dir, v2 ? 'hero-variants-2.png' : 'hero-variants.png'));
  await browser.close(); process.exit(0);
}
const done = [];
for (const scale of [2, 1]) {
  const page = await browser.newPage({ viewport: { width: 1920, height: 1080 }, deviceScaleFactor: scale });
  await page.goto(pathToFileURL(join(here, 'template.html')).href);
  if (fontCss) await page.addStyleTag({ content: fontCss });
  for (const s of slides) {
    if (only && s.id !== only) continue;
    await page.evaluate(x => window.renderSlide(x), s);
    await page.evaluate(() => Promise.all([...document.images].map(i => i.decode().catch(() => {}))).then(() => document.fonts.ready).then(() => window.centerSlide && window.centerSlide()));
    const file = join(out, `ocy-${s.id}-${scale === 2 ? '3840x2160' : '1920x1080'}.png`);
    await page.screenshot({ path: file });
    if (scale === 1) done.push(file);
    console.log(file);
  }
  await page.close();
}

// Contact sheet: all slides small, for review.
if (!only) {
  const page = await browser.newPage({ viewport: { width: 1640, height: 100 } });
  await page.goto(pathToFileURL(join(here, 'template.html')).href);
  const cells = done.map(f => `<figure><img src="${pathToFileURL(f).href}"><figcaption>${f.split('/').pop()}</figcaption></figure>`).join('');
  await page.evaluate(h => { document.documentElement.innerHTML = h; }, `<style>body{margin:0;padding:20px;background:#202322;font:14px sans-serif;color:#ccc;display:grid;grid-template-columns:repeat(2,790px);gap:20px}
    figure{margin:0}img{width:790px;display:block}figcaption{padding-top:6px}</style>${cells}`);
  await page.evaluate(() => Promise.all([...document.images].map(i => i.decode())));
  const file = join(out, 'contact-sheet.png');
  await page.screenshot({ path: file, fullPage: true });
  console.log(file);
}
await browser.close();

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const siteDir = path.resolve(__dirname, '..');
const repoRoot = path.resolve(siteDir, '..');
const i18nDir = path.join(siteDir, 'src', 'i18n');
const distDir = path.join(siteDir, 'dist');
const origin = 'https://danielmevit.github.io';
const base = '/ocy';

const errors = [];
const fail = (msg) => errors.push(msg);

if (!fs.existsSync(distDir)) {
  console.error(`Missing build output directory: ${distDir}`);
  process.exit(1);
}

const langs = fs
  .readdirSync(i18nDir)
  .filter((f) => f.endsWith('.json'))
  .map((f) => f.slice(0, -'.json'.length))
  .sort((a, b) => (a === 'en' ? -1 : b === 'en' ? 1 : a.localeCompare(b, 'en')));

if (!langs.includes('en')) {
  fail('Missing site/src/i18n/en.json');
}

const enTable = JSON.parse(fs.readFileSync(path.join(i18nDir, 'en.json'), 'utf8'));
const enKeys = Object.keys(enTable).filter((k) => !k.startsWith('_meta.'));
const appEnTable = JSON.parse(
  fs.readFileSync(path.join(repoRoot, 'content/strings/en.json'), 'utf8')
);
const allKnownKeys = Array.from(
  new Set([
    ...enKeys,
    ...Object.keys(appEnTable).filter((k) => k.startsWith('exercise.')),
  ])
);

for (const lang of langs) {
  if (lang === 'en') continue;
  const table = JSON.parse(fs.readFileSync(path.join(i18nDir, `${lang}.json`), 'utf8'));
  for (const key of enKeys) {
    if (typeof table[key] !== 'string' || table[key].trim() === '') {
      fail(`${lang}.json is missing or empty for key "${key}"`);
    }
  }
}

function distFileFor(lang, pageType) {
  const slug = lang === 'en' ? '' : lang.toLowerCase();
  if (pageType === 'home') {
    return slug ? path.join(distDir, slug, 'index.html') : path.join(distDir, 'index.html');
  }
  if (pageType === 'privacy') {
    return slug
      ? path.join(distDir, slug, 'privacy', 'index.html')
      : path.join(distDir, 'privacy', 'index.html');
  }
  return slug ? path.join(distDir, slug, '404', 'index.html') : path.join(distDir, '404.html');
}

function expectedUrlFor(lang, pageType) {
  const prefix = lang === 'en' ? '' : `/${lang.toLowerCase()}`;
  if (pageType === 'home') return `${origin}${base}${prefix}/`;
  if (pageType === 'privacy') return `${origin}${base}${prefix}/privacy/`;
  return lang === 'en' ? `${origin}${base}/404.html` : `${origin}${base}${prefix}/404/`;
}

function parseAttributes(tag) {
  const attrs = {};
  const re = /([a-zA-Z_:.-]+)\s*=\s*"([^"]*)"/g;
  let m;
  while ((m = re.exec(tag)) !== null) {
    attrs[m[1].toLowerCase()] = m[2];
  }
  return attrs;
}

function decodeBasicEntities(s) {
  return s
    .replaceAll('&amp;', '&')
    .replaceAll('&lt;', '<')
    .replaceAll('&gt;', '>')
    .replaceAll('&quot;', '"')
    .replaceAll('&#39;', "'")
    .replaceAll('&#x27;', "'");
}

function extractTagText(html, tagName) {
  const m = html.match(new RegExp(`<${tagName}[^>]*>([\\s\\S]*?)</${tagName}>`, 'i'));
  return m ? decodeBasicEntities(m[1].trim()) : null;
}

function stripNonVisibleBlocks(html) {
  return html
    .replace(/<script\b[^>]*>[\s\S]*?<\/script>/gi, ' ')
    .replace(/<style\b[^>]*>[\s\S]*?<\/style>/gi, ' ')
    .replace(/<svg\b[^>]*>[\s\S]*?<\/svg>/gi, ' ');
}

const enTitles = {};
const enDescriptions = {};
const hreflangMaps = { home: new Map(), privacy: new Map() };

// First pass: English baseline and all-page checks
for (const pageType of ['home', 'privacy', '404']) {
  for (const lang of langs) {
    const filePath = distFileFor(lang, pageType);
    const relPath = path.relative(siteDir, filePath);
    if (!fs.existsSync(filePath)) {
      fail(`Missing built HTML file: ${relPath}`);
      continue;
    }

    const html = fs.readFileSync(filePath, 'utf8');

    // 1. <html lang="...">
    const htmlTagMatch = html.match(/<html\b[^>]*>/i);
    if (!htmlTagMatch) {
      fail(`${relPath}: missing <html> tag`);
    } else {
      const htmlAttrs = parseAttributes(htmlTagMatch[0]);
      if (htmlAttrs.lang !== lang) {
        fail(`${relPath}: expected <html lang="${lang}">, got "${htmlAttrs.lang}"`);
      }
    }

    // 2. <title> and <meta name="description">
    const title = extractTagText(html, 'title');
    if (!title) {
      fail(`${relPath}: missing <title>`);
    }
    const metaDescMatch = html.match(/<meta\b[^>]*\bname="description"[^>]*>/i);
    const metaDesc = metaDescMatch ? decodeBasicEntities(parseAttributes(metaDescMatch[0]).content || '') : null;
    if (!metaDesc) {
      fail(`${relPath}: missing <meta name="description">`);
    }

    if (lang === 'en') {
      enTitles[pageType] = title;
      enDescriptions[pageType] = metaDesc;
    } else {
      // Short page titles can legitimately equal English ("Privacy" in Dutch and Italian), so only the home
      // page title, which carries the search phrases, must differ. The meta description must always differ.
      if (pageType === 'home' && title && title === enTitles[pageType]) {
        fail(`${relPath}: <title> matches English ("${title}")`);
      }
      if (metaDesc && metaDesc === enDescriptions[pageType]) {
        fail(`${relPath}: meta description matches English`);
      }
    }

    // 3. No {placeholder} or raw i18n key in rendered HTML
    const visibleWithoutScripts = stripNonVisibleBlocks(html);
    const placeholderMatches = visibleWithoutScripts.match(/\{[a-zA-Z_][a-zA-Z0-9_]*\}/g);
    if (placeholderMatches) {
      fail(`${relPath}: found unresolved placeholder(s) in HTML: ${Array.from(new Set(placeholderMatches)).join(', ')}`);
    }

    // Check JSON-LD scripts separately for unresolved placeholders & inLanguage
    const ldScripts = Array.from(html.matchAll(/<script\b[^>]*type="application\/ld\+json"[^>]*>([\s\S]*?)<\/script>/gi));
    for (const ldMatch of ldScripts) {
      const ldRaw = ldMatch[1];
      const ldPlaceholders = ldRaw.match(/\{[a-zA-Z_][a-zA-Z0-9_]*\}/g);
      if (ldPlaceholders) {
        fail(`${relPath}: found unresolved placeholder(s) in JSON-LD: ${ldPlaceholders.join(', ')}`);
      }
      try {
        const ldObj = JSON.parse(ldRaw);
        if (ldObj.inLanguage !== lang) {
          fail(`${relPath}: JSON-LD @type=${ldObj['@type']} expected inLanguage="${lang}", got "${ldObj.inLanguage}"`);
        }
      } catch (e) {
        fail(`${relPath}: invalid JSON-LD (${e.message})`);
      }
    }

    // Check that no raw i18n key appears in text content, titles, metas, or JSON-LD
    const textContent = visibleWithoutScripts.replace(/<[^>]+>/g, ' ') + ' ' + ldScripts.map((m) => m[1]).join(' ');
    for (const key of allKnownKeys) {
      // Match key as an isolated token (not part of a CSS class/id/URL)
      const escaped = key.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
      const keyRe = new RegExp(`(^|[\\s>"'])${escaped}([\\s<"']|$)`);
      if (keyRe.test(textContent)) {
        fail(`${relPath}: raw i18n key "${key}" leaked into rendered output`);
      }
    }

    // 4. Indexable pages: canonical + hreflang + og:locale
    if (pageType !== '404') {
      const expectedUrl = expectedUrlFor(lang, pageType);
      const canonicalTag = html.match(/<link\b[^>]*\brel="canonical"[^>]*>/i);
      if (!canonicalTag) {
        fail(`${relPath}: missing <link rel="canonical">`);
      } else {
        const canonicalHref = parseAttributes(canonicalTag[0]).href;
        if (canonicalHref !== expectedUrl) {
          fail(`${relPath}: canonical "${canonicalHref}" !== expected "${expectedUrl}"`);
        }
      }

      const ogLocaleTag = html.match(/<meta\b[^>]*\bproperty="og:locale"[^>]*>/i);
      if (!ogLocaleTag || !parseAttributes(ogLocaleTag[0]).content) {
        fail(`${relPath}: missing <meta property="og:locale">`);
      }

      const altTags = Array.from(html.matchAll(/<link\b[^>]*\brel="alternate"[^>]*\bhreflang="[^"]*"[^>]*>/gi));
      const map = new Map();
      for (const m of altTags) {
        const attrs = parseAttributes(m[0]);
        if (map.has(attrs.hreflang)) {
          fail(`${relPath}: duplicate hreflang="${attrs.hreflang}"`);
        }
        map.set(attrs.hreflang, attrs.href);
      }
      for (const expectedLang of langs) {
        const expectedAltUrl = expectedUrlFor(expectedLang, pageType);
        if (map.get(expectedLang) !== expectedAltUrl) {
          fail(`${relPath}: hreflang="${expectedLang}" is "${map.get(expectedLang)}", expected "${expectedAltUrl}"`);
        }
      }
      const xDefaultExpected = expectedUrlFor('en', pageType);
      if (map.get('x-default') !== xDefaultExpected) {
        fail(`${relPath}: hreflang="x-default" is "${map.get('x-default')}", expected "${xDefaultExpected}"`);
      }
      if (map.size !== langs.length + 1) {
        fail(`${relPath}: expected ${langs.length + 1} hreflang links, found ${map.size}`);
      }
      hreflangMaps[pageType].set(lang, map);
    }
  }
}

// Reciprocity check
for (const pageType of ['home', 'privacy']) {
  for (const a of langs) {
    const mapA = hreflangMaps[pageType].get(a);
    if (!mapA) continue;
    for (const b of langs) {
      const mapB = hreflangMaps[pageType].get(b);
      if (!mapB) continue;
      if (mapA.get(b) !== expectedUrlFor(b, pageType) || mapB.get(a) !== expectedUrlFor(a, pageType)) {
        fail(`Non-reciprocal hreflang between ${a} and ${b} on ${pageType}`);
      }
    }
  }
}

// 5. Sitemap check
const sitemapPath = path.join(distDir, 'sitemap.xml');
if (!fs.existsSync(sitemapPath)) {
  fail('Missing dist/sitemap.xml');
} else {
  const xml = fs.readFileSync(sitemapPath, 'utf8');
  if (!xml.includes('xmlns:xhtml="http://www.w3.org/1999/xhtml"')) {
    fail('dist/sitemap.xml missing xmlns:xhtml namespace');
  }
  const urlBlocks = Array.from(xml.matchAll(/<url>([\s\S]*?)<\/url>/g));
  const seenLocs = new Set();
  for (const blockMatch of urlBlocks) {
    const block = blockMatch[1];
    const locMatch = block.match(/<loc>([^<]+)<\/loc>/);
    if (!locMatch) {
      fail('dist/sitemap.xml <url> entry missing <loc>');
      continue;
    }
    const loc = locMatch[1];
    seenLocs.add(loc);
    const pageType = loc.endsWith('/privacy/') ? 'privacy' : 'home';
    const altLinks = Array.from(block.matchAll(/<xhtml:link\b[^>]*>/g)).map((m) => parseAttributes(m[0]));
    const altMap = new Map(altLinks.map((a) => [a.hreflang, a.href]));
    for (const l of langs) {
      if (altMap.get(l) !== expectedUrlFor(l, pageType)) {
        fail(`dist/sitemap.xml <loc>${loc}</loc> missing/wrong xhtml:link for ${l}`);
      }
    }
    if (altMap.get('x-default') !== expectedUrlFor('en', pageType)) {
      fail(`dist/sitemap.xml <loc>${loc}</loc> missing/wrong xhtml:link for x-default`);
    }
  }
  for (const pageType of ['home', 'privacy']) {
    for (const lang of langs) {
      const u = expectedUrlFor(lang, pageType);
      if (!seenLocs.has(u)) {
        fail(`dist/sitemap.xml missing <loc>${u}</loc>`);
      }
    }
  }
}

if (errors.length > 0) {
  console.error(`check-i18n failed with ${errors.length} error(s):`);
  for (const e of errors) console.error(`  - ${e}`);
  process.exit(1);
}

console.log(
  `check-i18n passed: ${langs.length} language(s) (${langs.join(', ')}), ${langs.length * 3} HTML pages, ${langs.length * 2} sitemap URLs, ${enKeys.length} keys in en.json.`
);

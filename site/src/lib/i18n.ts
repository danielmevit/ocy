import fs from 'node:fs';
import path from 'node:path';

export type PageType = 'home' | 'privacy' | '404';

export const DEFAULT_LANG = 'en';

/**
 * Endonyms copied from `rust/apps/ocy/ocy-app/src/strings.rs` (`language_display_name`).
 * Lowercase lookup is supported via `languageEndonym()`.
 */
export const LANGUAGE_ENDONYMS: Record<string, string> = {
  en: 'English',
  de: 'Deutsch',
  mk: 'Македонски',
  ja: '日本語',
  es: 'Español',
  fr: 'Français',
  it: 'Italiano',
  'pt-BR': 'Português (Brasil)',
  'pt-PT': 'Português (Portugal)',
  nl: 'Nederlands',
  pl: 'Polski',
  cs: 'Čeština',
  sk: 'Slovenčina',
  sl: 'Slovenščina',
  hr: 'Hrvatski',
  bs: 'Bosanski',
  'sr-Latn': 'Srpski (latinica)',
  hu: 'Magyar',
  ro: 'Română',
  sv: 'Svenska',
  da: 'Dansk',
  nb: 'Norsk bokmål',
  fi: 'Suomi',
  et: 'Eesti',
  lv: 'Latviešu',
  lt: 'Lietuvių',
  sq: 'Shqip',
  tr: 'Türkçe',
  sr: 'Српски',
  bg: 'Български',
  ru: 'Русский',
  uk: 'Українська',
  el: 'Ελληνικά',
  'zh-Hans': '简体中文',
  'zh-Hant': '繁體中文',
  ko: '한국어',
  hi: 'हिन्दी',
  th: 'ไทย',
  vi: 'Tiếng Việt',
  id: 'Bahasa Indonesia',
};

const OG_LOCALES: Record<string, string> = {
  en: 'en_US',
  bg: 'bg_BG',
  bs: 'bs_BA',
  cs: 'cs_CZ',
  da: 'da_DK',
  de: 'de_DE',
  el: 'el_GR',
  es: 'es_ES',
  et: 'et_EE',
  fi: 'fi_FI',
  fr: 'fr_FR',
  hi: 'hi_IN',
  hr: 'hr_HR',
  hu: 'hu_HU',
  id: 'id_ID',
  it: 'it_IT',
  ja: 'ja_JP',
  ko: 'ko_KR',
  lt: 'lt_LT',
  lv: 'lv_LV',
  mk: 'mk_MK',
  nb: 'nb_NO',
  nl: 'nl_NL',
  pl: 'pl_PL',
  'pt-BR': 'pt_BR',
  'pt-PT': 'pt_PT',
  ro: 'ro_RO',
  ru: 'ru_RU',
  sk: 'sk_SK',
  sl: 'sl_SI',
  sq: 'sq_AL',
  sr: 'sr_RS',
  'sr-Latn': 'sr_RS',
  sv: 'sv_SE',
  th: 'th_TH',
  tr: 'tr_TR',
  uk: 'uk_UA',
  vi: 'vi_VN',
  'zh-Hans': 'zh_CN',
  'zh-Hant': 'zh_TW',
};

const ENDONYMS_LOWER = new Map<string, string>(
  Object.entries(LANGUAGE_ENDONYMS).map(([k, v]) => [k.toLowerCase(), v])
);
const OG_LOCALES_LOWER = new Map<string, string>(
  Object.entries(OG_LOCALES).map(([k, v]) => [k.toLowerCase(), v])
);

function i18nDir(): string {
  const cwd = process.cwd();
  const direct = path.join(cwd, 'src', 'i18n');
  if (fs.existsSync(direct)) return direct;
  const nested = path.join(cwd, 'site', 'src', 'i18n');
  if (fs.existsSync(nested)) return nested;
  throw new Error(`Could not locate site/src/i18n directory from ${cwd}`);
}

const tableCache = new Map<string, Record<string, string>>();
let cachedLanguages: string[] | null = null;

function readJsonTable(filePath: string): Record<string, string> {
  const parsed = JSON.parse(fs.readFileSync(filePath, 'utf8')) as Record<string, unknown>;
  const table: Record<string, string> = {};
  for (const [key, val] of Object.entries(parsed)) {
    if (key.startsWith('_meta.')) continue;
    if (typeof val !== 'string') {
      throw new Error(`Expected string for i18n key "${key}" in ${filePath}`);
    }
    table[key] = val;
  }
  return table;
}

/**
 * Lists all available languages that have a `site/src/i18n/<lang>.json` file,
 * with `en` first and remaining tags sorted alphabetically.
 */
export function getAvailableLanguages(): string[] {
  if (cachedLanguages) return cachedLanguages;
  const dir = i18nDir();
  const langs = fs
    .readdirSync(dir)
    .filter((name) => name.endsWith('.json'))
    .map((name) => name.slice(0, -'.json'.length))
    .sort((a, b) => {
      if (a === DEFAULT_LANG) return -1;
      if (b === DEFAULT_LANG) return 1;
      return a.localeCompare(b, 'en');
    });
  if (!langs.includes(DEFAULT_LANG)) {
    throw new Error(`Missing required default translation file ${DEFAULT_LANG}.json in ${dir}`);
  }
  cachedLanguages = langs;
  return langs;
}

/**
 * Returns the non-English languages for `[lang]/...` dynamic routes.
 */
export function getNonDefaultLanguages(): string[] {
  return getAvailableLanguages().filter((l) => l !== DEFAULT_LANG);
}

/**
 * Loads and validates the translation table for `lang` against `en.json`.
 * Fails the build if any key in `en.json` is missing from `<lang>.json`.
 */
export function loadStrings(lang: string): Record<string, string> {
  const hit = tableCache.get(lang);
  if (hit) return hit;

  const dir = i18nDir();
  const filePath = path.join(dir, `${lang}.json`);
  if (!fs.existsSync(filePath)) {
    throw new Error(`Missing site i18n file for language "${lang}": ${filePath}`);
  }
  const table = readJsonTable(filePath);

  if (lang !== DEFAULT_LANG) {
    const enTable = loadStrings(DEFAULT_LANG);
    const missing = Object.keys(enTable).filter((k) => !(k in table) || table[k].trim() === '');
    if (missing.length > 0) {
      throw new Error(
        `Translation table "${lang}.json" is missing ${missing.length} key(s): ${missing.join(', ')}`
      );
    }
  }

  tableCache.set(lang, table);
  return table;
}

/**
 * Looks up `key` in `lang` and interpolates `{placeholder}` tokens.
 * Throws during build if the key is missing or any `{placeholder}` remains unresolved.
 */
export function t(lang: string, key: string, vars?: Record<string, string | number>): string {
  const table = loadStrings(lang);
  if (!(key in table)) {
    throw new Error(`Missing i18n key "${key}" for language "${lang}"`);
  }
  let value = table[key];
  if (vars) {
    for (const [k, v] of Object.entries(vars)) {
      value = value.replaceAll(`{${k}}`, String(v));
    }
  }
  const unresolved = value.match(/\{[a-zA-Z0-9_]+\}/);
  if (unresolved) {
    throw new Error(`Unresolved placeholder ${unresolved[0]} in key "${key}" (${lang}): "${value}"`);
  }
  return value;
}

/**
 * Splits a localized template string on `{token}` placeholders for safe Astro/HTML rendering
 * of inline links, `<code>`, or `<strong>` elements without hardcoding word order.
 */
export function splitTemplateParts(
  template: string
): Array<{ type: 'text'; value: string } | { type: 'slot'; name: string }> {
  const parts: Array<{ type: 'text'; value: string } | { type: 'slot'; name: string }> = [];
  const re = /\{([a-zA-Z0-9_]+)\}/g;
  let lastIndex = 0;
  let match: RegExpExecArray | null;
  while ((match = re.exec(template)) !== null) {
    if (match.index > lastIndex) {
      parts.push({ type: 'text', value: template.slice(lastIndex, match.index) });
    }
    parts.push({ type: 'slot', name: match[1] });
    lastIndex = re.lastIndex;
  }
  if (lastIndex < template.length) {
    parts.push({ type: 'text', value: template.slice(lastIndex) });
  }
  return parts;
}

/**
 * Raw template lookup (for templates containing structural slot tokens like `{zip}` or `{github}`).
 */
export function rawTemplate(lang: string, key: string): string {
  const table = loadStrings(lang);
  if (!(key in table)) {
    throw new Error(`Missing i18n key "${key}" for language "${lang}"`);
  }
  return table[key];
}

export function languageEndonym(lang: string): string {
  return ENDONYMS_LOWER.get(lang.toLowerCase()) ?? lang;
}

export function ogLocaleFor(lang: string): string {
  return OG_LOCALES_LOWER.get(lang.toLowerCase()) ?? lang.replace('-', '_');
}

/**
 * Returns the lowercase URL slug for non-default languages, or `''` for English.
 */
export function langSlug(lang: string): string {
  return lang === DEFAULT_LANG ? '' : lang.toLowerCase();
}

/**
 * Returns the relative path inside `base` (starting with `/`) for a given `(lang, pageType)`.
 */
export function pageSubPath(lang: string, pageType: PageType): string {
  const prefix = lang === DEFAULT_LANG ? '' : `/${lang.toLowerCase()}`;
  if (pageType === 'home') return `${prefix}/`;
  if (pageType === 'privacy') return `${prefix}/privacy/`;
  return lang === DEFAULT_LANG ? '/404.html' : `${prefix}/404/`;
}

/**
 * Returns the full `href` path including `base` (e.g. `/ocy/de/privacy/`).
 */
export function pageHref(base: string, lang: string, pageType: PageType): string {
  const cleanBase = base.replace(/\/$/, '');
  return `${cleanBase}${pageSubPath(lang, pageType)}`;
}

/**
 * Returns the absolute canonical URL for `(origin, base, lang, pageType)`.
 */
export function pageAbsoluteUrl(
  origin: string,
  base: string,
  lang: string,
  pageType: PageType
): string {
  const cleanOrigin = origin.replace(/\/$/, '');
  return `${cleanOrigin}${pageHref(base, lang, pageType)}`;
}

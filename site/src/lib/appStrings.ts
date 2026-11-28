import fs from 'node:fs';
import path from 'node:path';

/**
 * Reads the app's own UI labels from the repo's string tables (`content/strings/<lang>.json`),
 * so the site names menus and switches exactly as the app shows them. Build-time only.
 */

/** Site language tag → app string table name, where they differ. */
const APP_LANG_MAP: Record<string, string> = {};

function stringsDir(): string {
  const cwd = process.cwd();
  for (const dir of [path.join(cwd, '..', 'content', 'strings'), path.join(cwd, 'content', 'strings')]) {
    if (fs.existsSync(path.join(dir, 'en.json'))) return dir;
  }
  throw new Error(`Could not locate the app string tables (content/strings) from ${cwd}`);
}

const cache = new Map<string, Record<string, unknown>>();
function table(name: string): Record<string, unknown> {
  const hit = cache.get(name);
  if (hit) return hit;
  const file = path.join(stringsDir(), `${name}.json`);
  if (!fs.existsSync(file)) {
    throw new Error(`No app string table for site language "${name}" (${file}); add it to APP_LANG_MAP`);
  }
  const parsed = JSON.parse(fs.readFileSync(file, 'utf8')) as Record<string, unknown>;
  cache.set(name, parsed);
  return parsed;
}

/** The app's label for `key` in `lang`, falling back to the app's English table. */
export function appLabel(lang: string, key: string): string {
  const own = table(APP_LANG_MAP[lang] ?? lang)[key];
  if (typeof own === 'string' && own.trim() !== '') return own;
  const en = table('en')[key];
  if (typeof en === 'string' && en.trim() !== '') return en;
  throw new Error(`App string "${key}" is missing from content/strings/en.json`);
}

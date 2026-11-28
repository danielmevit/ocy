"""Extracts Ocy's app languages (tag + endonym) from the app source into languages.json, in the dropdown's order.
Source of truth: rust/apps/ocy/ocy-app/src/strings.rs (language_display_name) + content/strings/*.json (shipped tags).
Run: python3 docs/store/screenshots/languages.py"""
import json, os, re
here = os.path.dirname(os.path.abspath(__file__)); repo = os.path.normpath(os.path.join(here, '../../..'))
src = open(os.path.join(repo, 'rust/apps/ocy/ocy-app/src/strings.rs'), encoding='utf-8').read()
body = src[src.index('pub fn language_display_name'):]
body = body[:body.index('\n}\n')]
names = dict(re.findall(r'"([a-z-]+)" => "([^"]+)"', body))
tags = sorted(f[:-5] for f in os.listdir(os.path.join(repo, 'content/strings')) if f.endswith('.json'))
langs = []
for t in tags:
    n = names.get(t.lower())
    if n is None: raise SystemExit(f'no endonym for {t}')
    langs.append({'tag': t, 'name': n})
# Dropdown order (language_choice.rs): Latin scripts, then Cyrillic, then others; folded alphabetical inside.
import unicodedata
def rank(n):
    for ch in n:
        if ch.isalpha():
            o = ord(ch)
            if o < 0x250 or 0x1E00 <= o <= 0x1EFF: return 0
            if 0x400 <= o <= 0x52F: return 1
            return 2
    return 2
fold = lambda n: ''.join(c for c in unicodedata.normalize('NFKD', n) if not unicodedata.combining(c)).lower()
langs.sort(key=lambda l: (rank(l['name']), fold(l['name'])))
# Keep the flag mapping (variant B) if languages.json already has one.
try:
    old = {l['tag']: l.get('flag') for l in json.load(open(os.path.join(here, 'languages.json'), encoding='utf-8'))}
    for l in langs:
        if old.get(l['tag']): l['flag'] = old[l['tag']]
except FileNotFoundError:
    pass
json.dump(langs, open(os.path.join(here, 'languages.json'), 'w', encoding='utf-8'), ensure_ascii=False, indent=1)
print(len(langs), 'languages')

#!/usr/bin/env python3
"""WCAG 2.x contrast for Ocy tokens. Usage: python3 contrast.py [tokens.toml]
Reads tokens.toml, composites alpha tokens over their backdrop, prints ratios."""
import sys, tomllib
P = sys.argv[1] if len(sys.argv) > 1 else "rust/apps/ocy/theme/tokens.toml"
T = tomllib.load(open(P, "rb"))["color"]
def hx(s):
    s = s.lstrip("#"); a = int(s[6:8], 16)/255 if len(s) == 8 else 1.0
    return (int(s[0:2],16), int(s[2:4],16), int(s[4:6],16), a)
def tok(name, th):
    if name not in T and name == "input-border": name = "border"  # pre-V1 token set
    t = T[name]
    if "ref" in t:
        r = tok(t["ref"], th); al = t["alpha"]; al = al[th] if isinstance(al, dict) else al
        return (*r[:3], al)
    v = t[th]
    return tok(v["ref"], th) if isinstance(v, dict) else hx(v)
def over(fg, bg):
    a = fg[3]; return tuple(round(fg[i]*a + bg[i]*(1-a)) for i in range(3)) + (1.0,)
def lum(c):
    f = lambda v: (v/255)/12.92 if v/255 <= 0.04045 else ((v/255+0.055)/1.055)**2.4
    return 0.2126*f(c[0]) + 0.7152*f(c[1]) + 0.0722*f(c[2])
def ratio(a, b):
    la, lb = sorted((lum(a), lum(b)), reverse=True); return (la+0.05)/(lb+0.05)
def h(c): return "#%02X%02X%02X" % c[:3]
def dim(c, op, bg):  # element drawn at opacity `op` (disabled = 0.4) over bg
    return over((*c[:3], op), bg)
# (fg, bg, target, where, fg_opacity)  bg may be "name@over" meaning name composited over `over`
PAIRS = [
 ("text","surface",4.5,"card titles, labels, outline button text",1),
 ("text","bg",4.5,"Settings window text outside cards",1),
 ("text-muted","surface",4.5,"help/unit text, ghost button, muted body",1),
 ("text-muted","bg",4.5,"muted text directly on window bg",1),
 ("accent-text","surface",4.5,"LinkButton text (normal size), Today count",1),
 ("accent-text","bg",4.5,"LinkButton text on bg",1),
 ("accent-text-pressed","surface",4.5,"LinkButton pressed",1),
 ("on-accent","accent",4.5,"accent button label (rest)",1),
 ("on-accent","button-accent-hover",4.5,"accent button label (hover)",1),
 ("on-accent","button-accent-pressed",4.5,"accent button label (pressed)",1),
 ("error","bg",4.5,"save error text (Settings)",1),
 ("error","surface",4.5,"field error text in cards",1),
 ("attention","surface",4.5,"WarningCard title + help text",1),
 ("text","button-outline-hover@surface",4.5,"outline button text on hover",1),
 ("text","button-outline-pressed@surface",4.5,"outline button text pressed",1),
 ("accent","surface",3.0,"focus ring, toggle track/knob, progress fill, accent button shape",1),
 ("accent","bg",3.0,"focus ring on bg",1),
 ("system-accent-light-1","surface",3.0,"toggle hover",1),
 ("system-accent-dark-1","surface",3.0,"toggle pressed",1),
 ("surface","accent",3.0,"toggle knob (checked) on track",1),
 ("text-muted","surface",3.0,"outline button border, CloseButton x, stepper arrows",1),
 ("input-border@surface","surface",3.0,"NumberField border at rest",1),
 ("text-muted","surface",3.0,"NumberField border on hover",1),
 ("border@surface","bg",3.0,"Card border vs window bg (decorative, exempt: card is not a control)",1),
 ("accent","progress-track@surface",3.0,"progress fill vs track",1),
 ("progress-track@surface","surface",3.0,"progress track vs card (exempt: the fill carries the info)",1),
 ("divider@surface","surface",1.0,"SettingRow divider (decorative, exempt)",1),
 ("on-accent","accent",4.5,"disabled accent button (opacity .4, exempt)",0.4),
 ("text","surface",4.5,"disabled outline button (opacity .4, exempt)",0.4),
]
def res(n, th, base=None):
    if "@" in n:
        a, b = n.split("@"); return over(tok(a, th), res(b, th))
    c = tok(n, th); return over(c, (255,255,255,1)) if c[3] < 1 and base is None else c
def table(th):
    out = [f"\n### {th}\n", "| fg | bg | fg hex | bg hex | ratio | target | result | where |", "|---|---|---|---|---|---|---|---|"]
    fails = 0
    for fg, bg, tgt, where, op in PAIRS:
        b = res(bg, th); f = res(fg, th)
        if op < 1:
            # whole element at opacity over surface: composite both
            f = dim(f, op, res("surface", th)); b = dim(b, op, res("surface", th))
        r = ratio(f, b); ok = r >= tgt
        exempt = "exempt" in where
        if not ok and not exempt: fails += 1
        out.append(f"| {fg} | {bg} | {h(f)} | {h(b)} | {r:.2f} | {tgt if tgt>1 else '-'} | {'pass' if ok else ('exempt' if exempt else 'FAIL')} | {where} |")
    for desk in ("#F3F3F3", "#202020"):
        d = hx(desk)
        for n in ("edge-frame", "attention-frame", "overlay"):
            c = over(tok(n, th), d)
            st = "-" if n == "overlay" else ("pass" if ratio(c, d) >= 3 else "advisory")
            out.append(f"| {n} over desktop | {desk} | {h(c)} | {desk} | {ratio(c,d):.2f} | {'3.0 (advisory)' if n!='overlay' else '-'} | {st} | overlay effective colour |")
    out.append(f"\nFailing (non-exempt): {fails}")
    return "\n".join(out)
for th in ("light", "dark"): print(table(th))

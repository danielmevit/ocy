#!/usr/bin/env python3
"""V10 playful mode art: writes SVG sources (text-free, resvg-friendly).

PLACEHOLDER art (V10 by Claude); the final art comes from the owner.
Usage: python3 -I gen.py svg

Per mode (gentle, balanced, focused, custom) and theme (light, dark):
  scene-<mode>-<theme>-f<1..4>.svg     character scene, 160 x 110 header, 4-frame idle loop (f1 = still)
  glyph-<mode>-<theme>-f1.svg          40 x 40 card glyph
(The V10 abstract and person variants are still defined below but no longer written.)

The hand: ink outline 2.4 per 64 units (scaled), a soft fill printed slightly off-register
(down-right) under a wobbly outline, dot eyes, green cheeks, the two-leaf sprout. Frames re-wobble
the outline a little (the flip-book boil) on top of the motion. Every colour is a named token.
"""
import math
import os
import random
import sys

TOKENS = {
    "light": dict(bg="#F6F7F5", card="#FFFFFF", ink="#1B1E1F", line="#1B1E1F", body="#DCE9E4",
                  leaf="#2F7D62", paper="#FFFFFF", mute="#959DA1", soft="#CDD3D5",
                  warm="#E8955A", glow="#F4C76B", bubble="#EAF3EF"),
    # Dark (V11, One-green-era tokens): V10's dark ink vanished on the dark card, so dark uses a
    # light ink on a deep jade body; free-standing lines use `line`. `leaf` stays the dark-mode green.
    "dark": dict(bg="#17191A", card="#212426", ink="#E3E7E4", line="#C9CFCC", body="#2E4A40",
                 leaf="#5FBF97", paper="#33393A", mute="#A4ABAF", soft="#8E989C",
                 warm="#E8955A", glow="#F4C76B", bubble="#2C3A36"),
}
MODES = ["gentle", "balanced", "focused", "custom"]
FRAMES = 4
W, H = 160, 110
SW = 2.6          # ink weight in the 160x110 header (~= 2.4 @ 64 in ocy-v1f at display size)
OFF = (1.3, 1.1)  # fill misregistration


# ---- the hand -----------------------------------------------------------------------------
def spline(pts, closed=True):
    n = len(pts)
    seg = n if closed else n - 1
    d = f"M{pts[0][0]:.2f},{pts[0][1]:.2f}"
    for i in range(seg):
        p0 = pts[(i - 1) % n] if closed or i > 0 else pts[0]
        p1, p2 = pts[i], pts[(i + 1) % n]
        p3 = pts[(i + 2) % n] if closed or i + 2 < n else p2
        c1 = (p1[0] + (p2[0] - p0[0]) / 6, p1[1] + (p2[1] - p0[1]) / 6)
        c2 = (p2[0] - (p3[0] - p1[0]) / 6, p2[1] - (p3[1] - p1[1]) / 6)
        d += f" C{c1[0]:.2f},{c1[1]:.2f} {c2[0]:.2f},{c2[1]:.2f} {p2[0]:.2f},{p2[1]:.2f}"
    return d + (" Z" if closed else "")


class Hand:
    def __init__(self, T, seed, frame):
        self.T, self.r, self.frame = T, random.Random(seed * 31 + frame), frame

    def j(self, a):
        return self.r.uniform(-a, a)

    def blob_pts(self, cx, cy, rx, ry, n=10, jit=0.05, rot=0.0):
        out = []
        for i in range(n):
            a = 2 * math.pi * i / n + rot
            k = 1 + self.j(jit)
            out.append((cx + rx * k * math.cos(a), cy + ry * k * math.sin(a)))
        return out

    def poly_pts(self, pts, jit=0.5, sub=3):
        """Rounded-ish polygon: subdivide edges, jitter, spline through."""
        out = []
        n = len(pts)
        for i in range(n):
            (x0, y0), (x1, y1) = pts[i], pts[(i + 1) % n]
            for k in range(sub):
                t = k / sub
                out.append((x0 + (x1 - x0) * t + self.j(jit), y0 + (y1 - y0) * t + self.j(jit)))
        return out

    def shape(self, pts, fill, sw=SW, off=OFF, stroke=None, fill_op=1.0):
        d = spline(pts)
        s = stroke or self.T["ink"]
        o = ""
        if fill:
            o += (f'<path d="{d}" transform="translate({off[0]},{off[1]})" fill="{fill}" '
                  f'fill-opacity="{fill_op}"/>')
        if sw:
            o += (f'<path d="{d}" fill="none" stroke="{s}" stroke-width="{sw}" '
                  f'stroke-linecap="round" stroke-linejoin="round"/>')
        return o

    def stroke(self, pts, sw=SW, color=None, jit=0.0, op=1.0):
        pts = [(x + self.j(jit), y + self.j(jit)) for x, y in pts]
        return (f'<path d="{spline(pts, closed=False)}" fill="none" stroke="{color or self.T["ink"]}" '
                f'stroke-width="{sw}" stroke-opacity="{op}" stroke-linecap="round" stroke-linejoin="round"/>')

    def dot(self, x, y, r, color=None, op=1.0):
        return f'<ellipse cx="{x:.2f}" cy="{y:.2f}" rx="{r*1.05:.2f}" ry="{r*0.95:.2f}" fill="{color or self.T["ink"]}" fill-opacity="{op}"/>'


def rot(pts, cx, cy, deg):
    a = math.radians(deg)
    c, s = math.cos(a), math.sin(a)
    return [(cx + (x - cx) * c - (y - cy) * s, cy + (x - cx) * s + (y - cy) * c) for x, y in pts]


def rect(x, y, w, h):
    return [(x, y), (x + w, y), (x + w, y + h), (x, y + h)]


def g(body, tf=""):
    return f'<g transform="{tf}">{body}</g>' if tf else body


def svg(body, w=W, h=H):
    return (f'<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" '
            f'viewBox="0 0 {w} {h}">{body}</svg>')


# ---- characters ---------------------------------------------------------------------------
def sprout_leaves(h, x, y, s, droop=0.0, color=None):
    """Stem from (x,y) up, two leaves; droop tilts the leaves down (0..1)."""
    T = h.T
    top = (x + h.j(0.3), y - 7 * s)
    o = h.stroke([(x, y), (x + 0.4 * s, y - 3.5 * s), top], sw=SW * 0.8 * min(1, s + 0.2))
    for side in (-1, 1):
        a0 = math.radians(28 - 55 * droop)
        L = 8.5 * s
        tip = (top[0] + side * L * math.cos(a0), top[1] - L * math.sin(a0))
        mid = ((top[0] + tip[0]) / 2, (top[1] + tip[1]) / 2)
        nx, ny = -(tip[1] - top[1]), tip[0] - top[0]
        nl = math.hypot(nx, ny) or 1
        w = 2.6 * s
        p1 = (mid[0] + nx / nl * w, mid[1] + ny / nl * w)
        p2 = (mid[0] - nx / nl * w, mid[1] - ny / nl * w)
        pts = [top, p1, tip, p2]
        d = f"M{top[0]:.2f},{top[1]:.2f} Q{p1[0]:.2f},{p1[1]:.2f} {tip[0]:.2f},{tip[1]:.2f} Q{p2[0]:.2f},{p2[1]:.2f} {top[0]:.2f},{top[1]:.2f}Z"
        o += (f'<path d="{d}" transform="translate(0.8,0.7)" fill="{color or T["leaf"]}"/>'
              f'<path d="{d}" fill="none" stroke="{T["ink"]}" stroke-width="{SW*0.7*min(1,s+0.2):.2f}" stroke-linejoin="round"/>')
    return o


def face(h, cx, cy, s, eyes="dot", mouth="smile", look=(0, 0)):
    T = h.T
    o = ""
    ex = 6.2 * s
    for side in (-1, 1):
        x, y = cx + side * ex, cy - 1 * s
        if eyes == "dot":
            o += h.dot(x + look[0] * s, y + look[1] * s, 1.9 * s)
        elif eyes == "closed":   # content ^ ^ arcs
            o += h.stroke([(x - 2.2 * s, y + 0.6 * s), (x, y - 1.4 * s), (x + 2.2 * s, y + 0.6 * s)], sw=SW * 0.75 * s)
        elif eyes == "wide":
            o += (f'<ellipse cx="{x:.2f}" cy="{y:.2f}" rx="{3.4*s:.2f}" ry="{3.7*s:.2f}" fill="{T["paper"]}" '
                  f'stroke="{T["ink"]}" stroke-width="{SW*0.7*s:.2f}"/>')
            o += h.dot(x + look[0] * s, y + look[1] * s, 1.6 * s)
        elif eyes == "squint":   # concentrating > <
            o += h.stroke([(x - 2 * s * side, y - 1.6 * s), (x + 1.6 * s * side, y), (x - 2 * s * side, y + 1.6 * s)], sw=SW * 0.75 * s)
        # cheeks
        o += (f'<ellipse cx="{cx + side * 9.6 * s:.2f}" cy="{cy + 4.6 * s:.2f}" rx="{2.6*s:.2f}" ry="{1.7*s:.2f}" '
              f'fill="{T["leaf"]}" fill-opacity="0.8"/>')
    my = cy + 5.6 * s
    if mouth == "smile":
        o += h.stroke([(cx - 2.4 * s, my - 0.6 * s), (cx, my + 1.3 * s), (cx + 2.4 * s, my - 0.8 * s)], sw=SW * 0.8 * s)
    elif mouth == "o":
        o += f'<ellipse cx="{cx:.2f}" cy="{my+0.5*s:.2f}" rx="{1.5*s:.2f}" ry="{1.9*s:.2f}" fill="{T["ink"]}"/>'
    elif mouth == "tongue":
        o += f'<ellipse cx="{cx+1.6*s:.2f}" cy="{my+1.6*s:.2f}" rx="{1.6*s:.2f}" ry="{1.9*s:.2f}" fill="{T["warm"]}" stroke="{T["ink"]}" stroke-width="{SW*0.5*s:.2f}"/>'
        o += h.stroke([(cx - 2.6 * s, my), (cx + 3.0 * s, my - 0.2 * s)], sw=SW * 0.8 * s)
    elif mouth == "wobble":
        o += h.stroke([(cx - 3 * s, my), (cx - 1.5 * s, my - 1 * s), (cx, my), (cx + 1.5 * s, my - 1 * s), (cx + 3 * s, my)], sw=SW * 0.7 * s)
    return o


def sprout(h, cx, cy, s, eyes="dot", mouth="smile", look=(0, 0), droop=0.0, squash=1.0):
    """The Ocy sprout: a wobbly round body with a two-leaf sprout. (cx,cy) = body centre."""
    T = h.T
    rx, ry = 15 * s / squash ** 0.5, 14 * s * squash
    body = h.blob_pts(cx, cy, rx, ry, n=10, jit=0.045, rot=0.3)
    o = sprout_leaves(h, cx + 0.5 * s, cy - ry + 0.8 * s, s, droop)
    o += h.shape(body, T["body"], sw=SW * min(1, 0.55 + 0.5 * s))
    o += face(h, cx, cy + 1 * s, s, eyes, mouth, look)
    return o


def person(h, cx, cy, s, eyes="dot", mouth="smile", look=(0, 0)):
    """A simple person, same hand: round head, hair tuft, leaf-green jumper. (cx,cy)=head centre."""
    T = h.T
    torso = h.poly_pts([(cx - 13 * s, cy + 30 * s), (cx - 10 * s, cy + 13 * s), (cx + 10 * s, cy + 13 * s), (cx + 13 * s, cy + 30 * s)], jit=0.4)
    o = h.shape(torso, T["leaf"])
    head = h.blob_pts(cx, cy, 11 * s, 11.5 * s, n=9, jit=0.04)
    o += h.shape(head, T["body"])
    o += h.stroke([(cx - 7 * s, cy - 8 * s), (cx - 2 * s, cy - 12.5 * s), (cx + 4 * s, cy - 11 * s), (cx + 8 * s, cy - 7 * s)], sw=SW * 1.1)
    o += face(h, cx, cy + 0.5 * s, s * 0.72, eyes, mouth, look)
    return o


# ---- props --------------------------------------------------------------------------------
def flame(h, x, y, s, flick=0.0):
    T = h.T
    hgt = (9 + 2.5 * flick) * s
    outer = [(x, y - hgt), (x + 3.6 * s, y - 3.5 * s), (x + 2.8 * s, y + 0.8 * s), (x, y + 1.6 * s),
             (x - 2.8 * s, y + 0.8 * s), (x - 3.6 * s, y - 3.5 * s)]
    outer = [(px + h.j(0.25 * s), py + h.j(0.25 * s)) for px, py in outer]
    inner = [(x + 0.3 * s, y - hgt * 0.55), (x + 1.7 * s, y - 1.6 * s), (x, y + 0.4 * s), (x - 1.7 * s, y - 1.6 * s)]
    return h.shape(outer, T["warm"], sw=SW * 0.6 * s, off=(0.5, 0.5)) + h.shape(inner, T["glow"], sw=0)


def swirl(h, cx, cy, r, a0, sweep, sw=SW * 0.8, color=None, op=1.0):
    pts = []
    n = 9
    for i in range(n):
        t = i / (n - 1)
        a = a0 + sweep * t
        rr = r * (1 - 0.45 * t)
        pts.append((cx + rr * math.cos(a), cy + rr * math.sin(a)))
    return h.stroke(pts, sw=sw, color=color or h.T["mute"], op=op, jit=0.3)


def raincloud(h, x, y, s, frame):
    T = h.T
    o = ""
    for i in range(3):
        dx = x - 6 * s + i * 6 * s
        dy = y + 7 * s + ((frame + i) % 2) * 2 * s
        o += h.stroke([(dx, dy), (dx - 1.5 * s, dy + 4 * s)], sw=SW * 0.7, color=T["mute"])
    pts = []
    for i in range(16):
        a = math.pi + math.pi * i / 15          # upper half, left to right
        bump = 1 + 0.32 * abs(math.sin(2.5 * (a - math.pi)))
        pts.append((x + 13 * s * math.cos(a) * (1 + 0.05 * bump), y + 8 * s * math.sin(a) * bump + h.j(0.2)))
    pts += [(x + 12 * s, y + 2.5 * s), (x, y + 3 * s), (x - 12 * s, y + 2.5 * s)]
    return o + h.shape(pts, T["soft"], sw=SW * 0.75)


def bolt(h, x, y, s):
    T = h.T
    pts = [(x, y), (x + 5 * s, y), (x + 2 * s, y + 5 * s), (x + 5.5 * s, y + 5 * s), (x - 1 * s, y + 13 * s), (x + 1 * s, y + 7 * s), (x - 2.5 * s, y + 7 * s)]
    return h.shape(h.poly_pts(pts, jit=0.15, sub=1), T["glow"], sw=SW * 0.6, off=(0.6, 0.6))


def knob(h, x, y, r, ang, fill=None):
    T = h.T
    o = h.shape(h.blob_pts(x, y, r, r, n=7, jit=0.04), fill or T["paper"], sw=SW * 0.65, off=(0.6, 0.6))
    a = math.radians(ang)
    o += h.stroke([(x, y), (x + r * 0.8 * math.cos(a), y + r * 0.8 * math.sin(a))], sw=SW * 0.6)
    return o


def slider(h, x, y0, y1, t, w=1.0):
    T = h.T
    o = h.stroke([(x, y0), (x, y1)], sw=SW * 0.6, color=T["ink"])
    ky = y0 + (y1 - y0) * t
    o += h.shape(h.poly_pts(rect(x - 3.2 * w, ky - 1.8, 6.4 * w, 3.6), jit=0.12, sub=1), T["leaf"], sw=SW * 0.55, off=(0.5, 0.5))
    return o


# ---- scenes (character track) -------------------------------------------------------------
def scene_gentle(T, f, who="sprout"):
    h = Hand(T, 11, f)
    ph = f * math.pi / 2
    o = ""
    # storm around: swirls rotate a quarter step each frame
    for i, (r, cx, cy) in enumerate([(14, 26, 34), (12, 136, 30), (10, 140, 80), (11, 20, 82)]):
        o += swirl(h, cx, cy, r, ph * (1 if i % 2 else -1) + i, 4.2, color=T["mute"])
    o += raincloud(h, 34, 16 + (f % 2), 0.9, f)
    o += bolt(h, 122, 6 + (f % 2) * 0.8, 0.85)
    # flying stuff: a leaf, an umbrella-less hat... a sock
    lx, ly = 18 + 6 * math.cos(ph), 56 + 4 * math.sin(ph)
    o += h.shape(rot(h.blob_pts(lx, ly, 5, 2.2, n=6, jit=0.02), lx, ly, 40 + 90 * f), T["leaf"], sw=SW * 0.6, off=(0.5, 0.5))
    sx, sy = 145 + 4 * math.sin(ph), 54 + 5 * math.cos(ph)
    sock = rot(h.poly_pts([(sx - 3, sy - 7), (sx + 2, sy - 7), (sx + 2, sy + 1), (sx + 6, sy + 3), (sx + 5, sy + 7), (sx - 3, sy + 6)], jit=0.2, sub=1), sx, sy, -25 + 30 * f)
    o += h.shape(sock, T["warm"], sw=SW * 0.6, off=(0.5, 0.5))
    # wind dashes
    for k in range(3):
        y = 96 + k * 4 - (k == 1) * 2
        x = 10 + ((f * 9 + k * 23) % 30)
        o += h.stroke([(x, y), (x + 14, y - 1), (x + 20, y + 1)], sw=SW * 0.6, color=T["mute"])
        x2 = 112 + ((f * 7 + k * 17) % 24)
        o += h.stroke([(x2, y - 4), (x2 + 12, y - 5)], sw=SW * 0.6, color=T["mute"])
    # the calm bubble
    bob = [0, -1.2, 0, 1.2][f]
    b = h.blob_pts(80, 58 + bob, 33, 32, n=14, jit=0.015)
    o += h.shape(b, T["bubble"], sw=SW * 0.8, off=(0, 0), fill_op=0.95)
    o += h.stroke([(60, 42 + bob), (64, 36 + bob), (70, 32 + bob)], sw=SW * 0.8, color=T["paper"])
    if who == "sprout":
        o += sprout(h, 80, 66 + bob, 1.05, eyes="closed", mouth="smile")
        # a tiny cup of tea in hand-reach
        cx, cy = 98, 80 + bob
        o += h.shape(h.poly_pts([(cx - 4, cy - 4), (cx + 4, cy - 4), (cx + 3, cy + 3), (cx - 3, cy + 3)], jit=0.15, sub=1), T["paper"], sw=SW * 0.6, off=(0.5, 0.5))
        o += h.stroke([(cx, cy - 6), (cx + 1, cy - 9), (cx, cy - 12)], sw=SW * 0.5, color=T["mute"])
    return svg(o)


def stack_items(h, base_x, base_y, tilt, f):
    """Book, mug, apple, cactus, star: stacked upward from (base_x, base_y)."""
    T = h.T
    o = ""
    y = base_y
    items = []
    # book
    items.append((h.poly_pts(rect(base_x - 12, y - 6, 24, 6), jit=0.25, sub=2), T["leaf"]))
    y -= 6
    # second book, offset
    items.append((h.poly_pts(rect(base_x - 9, y - 5, 20, 5), jit=0.25, sub=2), T["warm"]))
    y -= 5
    # mug
    items.append((h.poly_pts([(base_x - 5, y - 10), (base_x + 5, y - 10), (base_x + 4.5, y), (base_x - 4.5, y)], jit=0.2, sub=1), T["paper"]))
    mug_top = y - 10
    y -= 10
    # plant pot on top of mug
    items.append((h.poly_pts([(base_x - 4, y - 6), (base_x + 4, y - 6), (base_x + 3, y), (base_x - 3, y)], jit=0.2, sub=1), T["warm"]))
    y -= 6
    for pts, fill in items:
        o += h.shape(rot(pts, base_x, base_y, tilt), fill, sw=SW * 0.7, off=(0.7, 0.6))
    # mug handle
    hp = rot([(base_x + 5, mug_top + 3), (base_x + 8.5, mug_top + 4.5), (base_x + 5, mug_top + 7.5)], base_x, base_y, tilt)
    o += h.stroke(hp, sw=SW * 0.7)
    # tiny leaf on the pot
    tp = rot([(base_x, y)], base_x, base_y, tilt)[0]
    o += sprout_leaves(h, tp[0], tp[1], 0.55)
    # the star on top, wobbling most
    st = rot([(base_x + 1, y - 9)], base_x, base_y, tilt * 1.7)[0]
    spts = []
    for i in range(10):
        a = -math.pi / 2 + i * math.pi / 5
        r = 4.2 if i % 2 == 0 else 1.9
        spts.append((st[0] + r * math.cos(a + math.radians(tilt * 2)), st[1] + r * math.sin(a + math.radians(tilt * 2))))
    o += h.shape(spts, T["glow"], sw=SW * 0.55, off=(0.5, 0.5))
    return o


def scene_balanced(T, f, who="sprout"):
    h = Hand(T, 22, f)
    tilt = [-5, 0, 5, 0][f]
    o = ""
    # the tightrope with posts
    o += h.stroke([(4, 86), (40, 89), (80, 92), (120, 89), (156, 86)], sw=SW * 0.7, color=T["line"])
    for x in (8, 152):
        o += h.stroke([(x, 86), (x, 108)], sw=SW, color=T["line"])
    lean = tilt * 0.4
    if who == "sprout":
        cx, cy = 80, 76
        # balance pole, tilting opposite
        pole = rot([(44, cy + 3), (80, cy + 4), (116, cy + 3)], cx, cy, -tilt * 0.6)
        o += h.stroke(pole, sw=SW * 0.8)
        for px, py in (pole[0], pole[-1]):
            o += h.shape(h.blob_pts(px, py, 2.6, 2.6, n=6, jit=0.05), T["leaf"], sw=SW * 0.55, off=(0.4, 0.4))
        o += g(sprout(h, cx, cy, 1.05, eyes="dot", mouth="wobble", look=(0, -1.2)), f"rotate({lean} {cx} {cy+10})")
        # stack balanced on head (behind the sprout leaves is fine: draw on top)
        o += g(stack_items(h, cx + lean * 0.3, cy - 15, tilt, f), f"translate({cx} {cy-15}) scale(1.25) translate({-cx} {-(cy-15)})")
        # sweat drop
        o += h.shape(h.poly_pts([(cx + 19, cy - 12), (cx + 21.5, cy - 7), (cx + 19, cy - 5), (cx + 16.5, cy - 7)], jit=0.05, sub=1), T["paper"], sw=SW * 0.45, off=(0, 0))
    else:
        cx, cy = 80, 50
        pole = rot([(42, cy + 22), (80, cy + 22), (118, cy + 22)], cx, cy + 22, -tilt * 0.6)
        o += person(h, cx, cy, 0.95, eyes="dot", mouth="wobble", look=(0, -1.5))
        # legs to the rope
        o += h.stroke([(cx - 6, cy + 28), (cx - 7, cy + 41)], sw=SW)
        o += h.stroke([(cx + 6, cy + 28), (cx + 8, cy + 41)], sw=SW)
        # arms to the pole
        o += h.stroke([(cx - 10, cy + 16), (cx - 16, cy + 21), pole[0][:1] and (cx - 20, pole[1][1] + (pole[0][1] - pole[1][1]) * 0.3)], sw=SW * 0.9)
        o += h.stroke([(cx + 10, cy + 16), (cx + 16, cy + 21), (cx + 20, pole[1][1] + (pole[2][1] - pole[1][1]) * 0.3)], sw=SW * 0.9)
        o += h.stroke(pole, sw=SW * 0.8)
        for px, py in (pole[0], pole[-1]):
            o += h.shape(h.blob_pts(px, py, 2.6, 2.6, n=6, jit=0.05), T["leaf"], sw=SW * 0.55, off=(0.4, 0.4))
        o += g(stack_items(h, cx, cy - 11, tilt, f), f"translate({cx} {cy-11}) scale(1.25) translate({-cx} {-(cy-11)})")
    return svg(o)


def laptop(h, cx, by, w=46, hgt=24):
    T = h.T
    # lid seen from behind, plus base wedge
    lid = h.poly_pts([(cx - w / 2, by - hgt), (cx + w / 2, by - hgt), (cx + w / 2 - 1, by), (cx - w / 2 + 1, by)], jit=0.3, sub=2)
    o = h.shape(lid, T["soft"], sw=SW * 0.8)
    base = h.poly_pts([(cx - w / 2 - 4, by), (cx + w / 2 + 4, by), (cx + w / 2 + 6, by + 4), (cx - w / 2 - 6, by + 4)], jit=0.2, sub=2)
    o += h.shape(base, T["mute"], sw=SW * 0.8)
    o += sprout_leaves(h, cx, by - hgt / 2 + 3, 0.45, color=T["paper"])
    return o


def scene_focused(T, f, who="sprout"):
    h = Hand(T, 33, f)
    flick = [0, 1, 0.3, 0.8][f]
    o = ""
    # glow of the screen on the face: soft beams
    cx, cy = 80, 50
    o += h.stroke([(20, 100), (140, 100)], sw=SW * 0.7, color=T["line"])
    o += sprout(h, cx, cy, 1.35, eyes="wide", mouth="o", droop=0.55 + 0.15 * (f % 2))
    # flames on the eyes (each eye has its own flicker)
    for side, fl in ((-1, flick), (1, 1 - flick)):
        o += flame(h, cx + side * 6.2 * 1.35, cy - 4.6, 0.95, fl)
    # little smoke puffs from the leaves
    for k in range(2):
        px, py = cx + 14 + k * 6, cy - 32 - k * 6 - (f % 2) * 2
        o += h.shape(h.blob_pts(px, py, 3 + k, 2.6 + k, n=7, jit=0.08), T["soft"], sw=SW * 0.5, off=(0.4, 0.4))
    # laptop in front (we see its back)
    o += laptop(h, cx, 96, w=56, hgt=28)
    # a tiny clock on the side with the hand far round
    kx, ky = 132, 34
    o += h.shape(h.blob_pts(kx, ky, 9, 9, n=9, jit=0.03), T["paper"], sw=SW * 0.75)
    a = math.radians(-90 + 300 + 20 * f)
    o += h.stroke([(kx, ky), (kx + 6 * math.cos(a), ky + 6 * math.sin(a))], sw=SW * 0.6)
    o += h.stroke([(kx, ky), (kx + 3.5, ky - 1)], sw=SW * 0.6)
    # coffee cups piling up on the left
    for i, (x, y) in enumerate([(22, 90), (34, 90), (28, 79)]):
        o += h.stroke([(x + 4.6, y - 7), (x + 7.5, y - 5), (x + 4.3, y - 2.5)], sw=SW * 0.6)
        o += h.shape(h.poly_pts([(x - 5, y - 9), (x + 5, y - 9), (x + 4, y), (x - 4, y)], jit=0.15, sub=1), T["paper"], sw=SW * 0.6, off=(0.5, 0.5))
    return svg(o)


def scene_custom(T, f, who="sprout"):
    h = Hand(T, 44, f)
    o = ""
    # the sprout behind the desk, tongue out, concentrating
    cx, cy = 80, 40
    o += sprout(h, cx, cy, 1.15, eyes="squint" if f in (1, 2) else "dot", mouth="tongue", look=(0, 1.5))
    # clipboard of rules pinned up right
    bx, by = 134, 20
    o += h.shape(h.poly_pts(rect(bx - 10, by - 2, 20, 26), jit=0.25, sub=2), T["paper"], sw=SW * 0.75)
    o += h.shape(h.poly_pts(rect(bx - 4, by - 5, 8, 4), jit=0.1, sub=1), T["mute"], sw=SW * 0.55, off=(0.4, 0.4))
    for k in range(4):
        y = by + 4 + k * 5
        o += h.stroke([(bx - 6, y), (bx - 4.5, y + 1.3), (bx - 2.5, y - 1.6)], sw=SW * 0.5, color=T["leaf"])
        o += h.stroke([(bx, y), (bx + 6 - (k % 2) * 2, y)], sw=SW * 0.5, color=T["mute"])
    # wrench on the left
    wx, wy = 22, 30
    wr = rot([(wx - 2, wy), (wx + 2, wy), (wx + 2, wy + 20), (wx - 2, wy + 20)], wx, wy + 10, 25)
    o += h.shape(h.poly_pts(wr, jit=0.12, sub=1), T["mute"], sw=SW * 0.6, off=(0.5, 0.5))
    head = h.blob_pts(wx - 4.2, wy - 1, 5, 5, n=8, jit=0.03)
    o += h.shape(head, T["mute"], sw=SW * 0.6, off=(0.5, 0.5))
    o += h.dot(wx - 4.4, wy - 2.2, 2.0, T["card"])
    # the mixing desk with far too many knobs
    desk = h.poly_pts([(18, 62), (142, 62), (156, 104), (4, 104)], jit=0.35, sub=4)
    o += h.shape(desk, T["paper"], sw=SW)
    o += h.stroke([(10, 96), (150, 96)], sw=SW * 0.5, color=T["mute"])
    rr = random.Random(7)
    angs = [rr.uniform(0, 360) for _ in range(40)]
    k = 0
    for row, (y, x0, x1, r) in enumerate([(69, 26, 134, 3.0), (78, 20, 140, 3.3)]):
        n = 11 if row == 0 else 10
        for i in range(n):
            x = x0 + (x1 - x0) * i / (n - 1)
            spin = 40 * f * (1 if (i + row) % 3 == 0 else 0)
            o += knob(h, x, y, r, angs[k] + spin, fill=T["leaf"] if (i * 7 + row) % 5 == 0 else None)
            k += 1
    for i in range(9):
        x = 26 + i * 13.5
        t = 0.5 + 0.4 * math.sin(i * 1.7 + (f * math.pi / 2 if i % 2 else 0))
        o += slider(h, x, 85, 94, t)
    # one tiny warning-free light blinking
    o += h.dot(146, 70, 1.6, T["glow"] if f % 2 == 0 else T["soft"])
    return svg(o)


# ---- abstract track -----------------------------------------------------------------------
def abs_gentle(T, f):
    h = Hand(T, 55, f)
    ph = f * math.pi / 2
    o = ""
    for i in range(7):
        a = ph * 0.5 + i * 2 * math.pi / 7
        r = 40 + (i % 3) * 6
        x, y = 80 + r * 1.35 * math.cos(a), 56 + r * 0.95 * math.sin(a)
        o += swirl(h, x, y, 7 + (i % 3) * 2, a + 1.5, 3.6, color=T["mute"])
    # flying shards: triangles and squares tumbling
    for i, (x, y, kind) in enumerate([(16, 18, 0), (142, 22, 1), (150, 92, 0), (14, 94, 1), (120, 102, 0), (42, 8, 1)]):
        x += 3 * math.cos(ph + i)
        y += 3 * math.sin(ph + i)
        pts = [(x, y - 4), (x + 4, y + 3), (x - 4, y + 3)] if kind == 0 else rect(x - 3, y - 3, 6, 6)
        o += h.shape(rot(h.poly_pts(pts, jit=0.1, sub=1), x, y, 30 * f + i * 40), T["soft"] if i % 2 else T["warm"], sw=SW * 0.55, off=(0.4, 0.4))
    # the calm: a still circle with a still ring
    o += h.shape(h.blob_pts(80, 56, 26, 26, n=12, jit=0.012), T["bubble"], sw=SW * 0.6, off=(0, 0))
    o += h.shape(h.blob_pts(80, 57 + [0, -0.8, 0, 0.8][f], 13, 12.5, n=9, jit=0.03), T["leaf"])
    return svg(o)


def abs_balanced(T, f):
    h = Hand(T, 66, f)
    tilt = [-4, 0, 4, 0][f]
    o = h.stroke([(4, 96), (80, 98), (156, 96)], sw=SW * 0.7, color=T["line"])
    # base: a half dome standing on the line
    base = [(66, 97), (66.5, 88), (73, 81), (80, 79), (87, 81), (93.5, 88), (94, 97)]
    o += h.shape(h.poly_pts(base, jit=0.2, sub=1), T["leaf"])
    px, py = 80, 79
    parts = [
        (rect(66, 66, 28, 13), T["paper"]),
        ([(70, 66), (94, 66), (82, 50)], T["warm"]),
        (rect(77, 41, 12, 9), T["soft"]),
        ("c", (83, 35, 6), T["body"]),
        ("c", (86, 25.5, 3.6), T["glow"]),
    ]
    for i, p in enumerate(parts):
        t = tilt * (0.5 + 0.35 * i)
        if p[0] == "c":
            x, y, r = p[1]
            pts = h.blob_pts(x, y, r, r, n=8, jit=0.04)
            fill = p[2]
        else:
            pts, fill = h.poly_pts(p[0], jit=0.25, sub=2), p[1]
        o += h.shape(rot(pts, px, py, t), fill)
    return svg(o)


def abs_focused(T, f):
    h = Hand(T, 77, f)
    flick = [0, 1, 0.3, 0.8][f]
    o = ""
    # the screen: a big rounded square, glowing lines on it
    o += h.shape(h.poly_pts(rect(46, 12, 68, 50), jit=0.3, sub=3), T["soft"])
    o += h.stroke([(80, 62), (80, 72)], sw=SW)
    o += h.shape(h.poly_pts(rect(64, 72, 32, 5), jit=0.2, sub=2), T["mute"], sw=SW * 0.8)
    for k in range(3):
        o += h.stroke([(54, 22 + k * 8), (54 + 40 - k * 10, 22 + k * 8)], sw=SW * 0.6, color=T["paper"])
    # two eye discs in front, staring, flames
    for side, fl in ((-1, flick), (1, 1 - flick)):
        ex, ey = 80 + side * 14, 90
        o += h.shape(h.blob_pts(ex, ey, 9, 9, n=9, jit=0.03), T["paper"])
        o += h.dot(ex, ey - 2, 3.2)
        o += flame(h, ex, ey - 11, 1.0, fl)
    return svg(o)


def abs_custom(T, f):
    h = Hand(T, 88, f)
    o = h.shape(h.poly_pts(rect(10, 14, 140, 84), jit=0.35, sub=4), T["paper"])
    rr = random.Random(3)
    for row in range(3):
        for i in range(7):
            x, y = 24 + i * 12, 28 + row * 14
            ang = rr.uniform(0, 360) + (45 * f if (row + i) % 4 == 0 else 0)
            o += knob(h, x, y, 4.2, ang, fill=T["leaf"] if (row * 7 + i) % 6 == 0 else None)
    for i in range(4):
        x = 112 + i * 9
        t = 0.5 + 0.42 * math.sin(i * 2.1 + f * math.pi / 2 * (1 if i % 2 else -1))
        o += slider(h, x, 24, 84, t, w=1.1)
    for i in range(6):
        o += h.dot(24 + i * 15, 86, 2, T["glow"] if (i + f) % 3 == 0 else T["soft"])
    return svg(o)


# ---- glyphs (40 x 40) ---------------------------------------------------------------------
GW = 2.3


def glyph(T, mode):
    h = Hand(T, 99 + MODES.index(mode), 0)
    o = ""
    if mode == "gentle":
        o += h.stroke([(5, 13), (9, 6), (17, 4)], sw=GW, color=T["mute"])
        o += h.stroke([(35, 27), (31, 34), (23, 36)], sw=GW, color=T["mute"])
        o += h.stroke([(3, 26), (8, 25)], sw=GW * 0.8, color=T["mute"])
        o += h.stroke([(32, 9), (37, 8)], sw=GW * 0.8, color=T["mute"])
        o += h.shape(h.blob_pts(20, 20, 12, 12, n=10, jit=0.015), T["bubble"], sw=GW * 0.8, off=(0, 0))
        o += h.shape(h.blob_pts(20, 20.5, 6.2, 6, n=8, jit=0.03), T["leaf"], sw=GW, off=(0.7, 0.6))
    elif mode == "balanced":
        o += h.stroke([(2, 36), (20, 37), (38, 36)], sw=GW * 0.8, color=T["line"])
        o += h.shape(h.poly_pts([(13, 36.5), (14, 31), (20, 28), (26, 31), (27, 36.5)], jit=0.1, sub=1), T["leaf"], sw=GW, off=(0.7, 0.6))
        o += h.shape(rot(h.poly_pts(rect(12.5, 20, 15, 8), jit=0.12, sub=1), 20, 28, -4), T["paper"], sw=GW, off=(0.7, 0.6))
        o += h.shape(rot(h.poly_pts([(14, 20), (26, 20), (20.5, 11)], jit=0.1, sub=1), 20, 28, -7), T["warm"], sw=GW, off=(0.7, 0.6))
        o += h.shape(h.blob_pts(22.2, 7, 3.6, 3.6, n=7, jit=0.03), T["glow"], sw=GW, off=(0.6, 0.5))
    elif mode == "focused":
        o += h.shape(h.blob_pts(20, 25, 12, 10.5, n=10, jit=0.02), T["paper"], sw=GW, off=(0.7, 0.6))
        o += h.dot(20, 25.5, 4.4)
        o += h.dot(18.4, 23.8, 1.3, T["paper"])
        # flame on top
        x, y = 20, 14.5
        outer = [(x + 0.5, y - 12), (x + 5.2, y - 4.5), (x + 4, y + 0.5), (x, y + 1.6), (x - 4, y + 0.5), (x - 5.2, y - 4.5), (x - 2.5, y - 7), (x - 1.5, y - 9.5)]
        o += h.shape(outer, T["warm"], sw=GW * 0.8, off=(0.5, 0.5))
        o += h.shape([(x + 0.3, y - 6.5), (x + 2.2, y - 2.2), (x, y + 0.2), (x - 2.2, y - 2.2)], T["glow"], sw=0)
    elif mode == "custom":
        for i, (x, t) in enumerate([(9, 0.25), (20, 0.7), (31, 0.45)]):
            o += h.stroke([(x, 5), (x, 35)], sw=GW * 0.8, color=T["line"])
            ky = 7 + 26 * t
            o += h.shape(h.poly_pts(rect(x - 4.6, ky - 2.6, 9.2, 5.2), jit=0.1, sub=1),
                         T["leaf"] if i != 1 else T["warm"], sw=GW, off=(0.6, 0.5))
    return svg(o, 40, 40)


SCENES = {"gentle": scene_gentle, "balanced": scene_balanced, "focused": scene_focused, "custom": scene_custom}
ABSTRACT = {"gentle": abs_gentle, "balanced": abs_balanced, "focused": abs_focused, "custom": abs_custom}


def main(out):
    """V11 app set: svg/scene-<mode>-<theme>-f1..f4.svg and svg/glyph-<mode>-<theme>-f1.svg."""
    os.makedirs(out, exist_ok=True)
    n = 0
    for theme, T in TOKENS.items():
        for m in MODES:
            for f in range(FRAMES):
                with open(os.path.join(out, f"scene-{m}-{theme}-f{f + 1}.svg"), "w") as fh:
                    fh.write(SCENES[m](T, f))
                n += 1
            with open(os.path.join(out, f"glyph-{m}-{theme}-f1.svg"), "w") as fh:
                fh.write(glyph(T, m))
            n += 1
    print(f"wrote {n} svg")


if __name__ == "__main__":
    main(sys.argv[1])

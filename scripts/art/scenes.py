#!/usr/bin/env python3
"""Original illustrations for the launcher's hub: one scene per game.

Dev-time asset authoring only (the outputs, assets/art/*.jpg, are committed;
nothing in the build or the package runs this). Every scene is drawn here
from shapes and a fixed seed: no photos, no game files, no official art, no
logos and no characters. Sega's arcade racers of the 2000s live under big
skies, so each scene is a road receding to the horizon under its own sky: a
sunset coast with palms (OutRun 2 SP SDX), a midday coast (OutRun 2 SP), a
city at golden hour (Crazy Taxi), a jungle dirt stage (SEGA Rally 3), a
speedway under a blue sky with clouds (SEGA Racing Classic), the same
speedway at night under floodlights (Daytona Championship USA) and a street
circuit by the sea (F355 Challenge 2).

usage: python3 -I scripts/art/scenes.py assets/art   (needs rsvg-convert, Pillow)
"""
import math
import os
import random
import subprocess
import sys

W, H = 2560, 1440
HORIZON = 760
VX = W // 2 - 80


def hexrgb(h):
    h = h.lstrip('#')
    return tuple(int(h[i:i + 2], 16) for i in (0, 2, 4))


def mix(a, b, t):
    a, b = hexrgb(a), hexrgb(b)
    return '#%02x%02x%02x' % tuple(round(x + (y - x) * t) for x, y in zip(a, b))


class Scene:
    def __init__(self, slug, seed, sky_top, sky_low, sun, ground, bend, features, road='asphalt',
                 sea='#1b6fa8', night=False):
        self.slug, self.seed = slug, seed
        self.sky_top, self.sky_low, self.sun, self.ground = sky_top, sky_low, sun, ground
        self.bend, self.features, self.road, self.sea, self.night = bend, set(features), road, sea, night


SCENES = [
    # OutRun 2 SP SDX: the coast road at sunset, palms, the sea on the right
    Scene('or2sdx', 3, '#2b1650', '#ff8a3d', '#ffd66b', '#3a2a2e', -0.55,
          ['sun', 'sea_right', 'palms', 'clouds_low'], sea='#3a4f9a'),
    # OutRun 2 SP Special Tours: the same coast at midday, turquoise water, cliffs
    Scene('or2sp', 5, '#1f74d6', '#bfe7ff', '#fffbe0', '#c9b58a', 0.45,
          ['sea_left', 'palms', 'clouds', 'cliffs'], sea='#19b6c9'),
    # Crazy Taxi High Roller: a city avenue at golden hour
    Scene('cthr', 8, '#3366b8', '#ffc77a', '#fff0b0', '#2c2f38', 0.2,
          ['city', 'clouds_low', 'sun']),
    # SEGA Rally 3: a jungle dirt stage, dust in the air
    Scene('sr3', 13, '#3d8fd8', '#e7f2d8', '#fffbe8', '#2f4a1c', -0.8,
          ['jungle', 'clouds', 'dust'], road='dirt'),
    # SEGA Racing Classic: the speedway under a blue sky and white clouds
    Scene('src', 17, '#0f4fd0', '#9fd2ff', '#ffffff', '#2f8a3a', 0.65,
          ['speedway', 'clouds_big']),
    # F355 Challenge 2: a street circuit by the sea, late afternoon
    Scene('f355', 23, '#3a62b8', '#ffd9a0', '#fff1c9', '#3b3f48', 0.75,
          ['sun', 'sea_left', 'city', 'clouds_low'], sea='#2a7fb0'),
    # Daytona Championship USA: the speedway at night, floodlights
    Scene('dcusa', 19, '#050a1c', '#1e2f6a', '#e8f0ff', '#14361a', 0.65,
          ['speedway', 'floodlights'], night=True),
]


# ----------------------------------------------------------------- the road

def centre(s, u):
    """The road's centre line at depth u (0 at the horizon, 1 at the bottom edge)."""
    far = (1 - u) ** 2
    return VX + s.bend * 760 * far


def half_width(u):
    return 30 + 980 * u


def ypos(u):
    return HORIZON + (H - HORIZON) * u


def pt(s, u, lateral, height=0.0):
    return centre(s, u) + lateral * half_width(u), ypos(u) - height * 520 * u


def depths(n, near=1.0, far=0.04):
    z0, z1 = 1 / near, 1 / far
    return [1 / (z0 + (z1 - z0) * (i / (n - 1))) for i in range(n)]


def poly(points, fill, extra=''):
    return f'<polygon points="{" ".join(f"{x:.0f},{y:.0f}" for x, y in points)}" fill="{fill}" {extra}/>'


def defs(s):
    return f'''<defs>
  <linearGradient id="sky" x1="0" y1="0" x2="0" y2="1">
    <stop offset="0" stop-color="{s.sky_top}"/>
    <stop offset="0.7" stop-color="{mix(s.sky_top, s.sky_low, 0.75)}"/>
    <stop offset="1" stop-color="{s.sky_low}"/>
  </linearGradient>
  <linearGradient id="sea" x1="0" y1="0" x2="0" y2="1">
    <stop offset="0" stop-color="{mix(s.sea, s.sky_low, 0.35)}"/>
    <stop offset="1" stop-color="{mix(s.sea, '#000000', 0.35)}"/>
  </linearGradient>
  <radialGradient id="vig" cx="0.5" cy="0.5" r="0.78">
    <stop offset="0.6" stop-color="#000" stop-opacity="0"/>
    <stop offset="1" stop-color="#000" stop-opacity="{0.75 if s.night else 0.45}"/>
  </radialGradient>
  <filter id="blur6" x="-50%" y="-50%" width="200%" height="200%"><feGaussianBlur stdDeviation="6"/></filter>
  <filter id="blur24" x="-50%" y="-50%" width="200%" height="200%"><feGaussianBlur stdDeviation="24"/></filter>
  <filter id="blur60" x="-50%" y="-50%" width="200%" height="200%"><feGaussianBlur stdDeviation="60"/></filter>
</defs>'''


def sky(r, s):
    out = [f'<rect width="{W}" height="{H}" fill="url(#sky)"/>']
    if s.night:
        for _ in range(260):
            out.append(f'<circle cx="{r.randint(0, W)}" cy="{r.randint(10, HORIZON - 200)}" '
                       f'r="{r.uniform(0.7, 2.0):.1f}" fill="#fff" opacity="{r.uniform(0.2, 0.8):.2f}"/>')
    if 'sun' in s.features:
        sx, sy = VX + 520, HORIZON - 120
        out.append(f'<circle cx="{sx}" cy="{sy}" r="420" fill="{s.sun}" opacity="0.35" filter="url(#blur60)"/>')
        out.append(f'<circle cx="{sx}" cy="{sy}" r="150" fill="{s.sun}"/>')
    else:
        out.append(f'<circle cx="{W - 420}" cy="180" r="260" fill="{s.sun}" opacity="0.5" filter="url(#blur60)"/>')
    return '\n'.join(out)


def clouds(r, s, n, ymin, ymax, big=False):
    out = []
    for _ in range(n):
        cx, cy = r.randint(-100, W + 100), r.randint(ymin, ymax)
        size = r.uniform(1.4, 2.6) if big else r.uniform(0.6, 1.3)
        col = mix('#ffffff', s.sky_low, 0.15)
        for _ in range(7):
            out.append(f'<ellipse cx="{cx + r.uniform(-200, 200) * size:.0f}" cy="{cy + r.uniform(-30, 30) * size:.0f}" '
                       f'rx="{r.uniform(90, 170) * size:.0f}" ry="{r.uniform(40, 80) * size:.0f}" fill="{col}" '
                       f'opacity="{r.uniform(0.75, 0.95):.2f}"/>')
        out.append(f'<ellipse cx="{cx}" cy="{cy + 50 * size:.0f}" rx="{260 * size:.0f}" ry="{30 * size:.0f}" '
                   f'fill="{mix(s.sky_top, "#ffffff", 0.55)}" opacity="0.5" filter="url(#blur24)"/>')
    return '\n'.join(out)


def sea(s, side):
    x0 = 0 if side < 0 else VX
    x1 = VX if side < 0 else W
    out = [f'<rect x="{x0}" y="{HORIZON - 4}" width="{x1 - x0}" height="{H - HORIZON + 4}" fill="url(#sea)"/>']
    for k in range(18):
        y = HORIZON + 6 + k * k * 2.2
        out.append(f'<line x1="{x0}" y1="{y:.0f}" x2="{x1}" y2="{y:.0f}" stroke="{mix(s.sun, "#ffffff", 0.4)}" '
                   f'stroke-width="{1 + k * 0.25:.1f}" opacity="{0.12 + 0.01 * k:.2f}"/>')
    if 'sun' in s.features:
        out.append(f'<ellipse cx="{VX + 520}" cy="{HORIZON + 60}" rx="140" ry="220" fill="{s.sun}" opacity="0.35" filter="url(#blur24)"/>')
    return '\n'.join(out)


def hills(r, s, base, amp, shade):
    pts = [(-10, HORIZON + 40)]
    x = -10
    while x <= W + 80:
        pts.append((x, base - amp * (0.5 + 0.5 * math.sin(x / 380 + s.seed)) - r.uniform(0, amp * 0.3)))
        x += 60
    pts.append((W + 10, HORIZON + 40))
    return poly(pts, shade)


def cliffs(r, s):
    out = []
    pts = [(VX + 200, HORIZON + 4)]
    x = VX + 200
    while x < W + 60:
        pts.append((x, HORIZON - 40 - (x - VX) * 0.32 - r.uniform(0, 50)))
        x += 70
    pts += [(W + 10, H), (VX + 200, H)]
    out.append(poly(pts, mix(s.ground, '#7a5a3a', 0.4)))
    return '\n'.join(out)


def city(r, s):
    out = []
    for layer, (count, hmin, hmax, shade) in enumerate([(40, 120, 380, 0.55), (26, 220, 620, 0.35)]):
        for _ in range(count):
            side = r.choice((-1, 1))
            x = VX + side * r.uniform(160, 1300) * (1 + layer * 0.6)
            w = r.uniform(60, 160) * (1 + layer)
            h = r.uniform(hmin, hmax)
            col = mix('#20283a', s.sky_low, shade)
            out.append(f'<rect x="{x - w / 2:.0f}" y="{HORIZON - h:.0f}" width="{w:.0f}" height="{h + 10:.0f}" fill="{col}"/>')
            for wy in range(int(HORIZON - h + 20), HORIZON - 10, 34):
                for wx in range(int(x - w / 2 + 10), int(x + w / 2 - 10), 26):
                    if r.random() < 0.35:
                        out.append(f'<rect x="{wx}" y="{wy}" width="12" height="16" fill="{s.sun}" opacity="0.55"/>')
    return '\n'.join(out)


def palms(r, s):
    out = []
    for side in (-1, 1):
        if ('sea_right' in s.features and side > 0) or ('sea_left' in s.features and side < 0):
            lat = 1.6
        else:
            lat = 1.9
        for u in depths(9, 0.95, 0.12):
            x, y = pt(s, u, side * lat)
            h = 900 * u
            lean = side * 0.18 * h
            trunk = f'M{x:.0f},{y:.0f} Q{x + lean * 0.3:.0f},{y - h * 0.5:.0f} {x + lean:.0f},{y - h:.0f}'
            col = '#1a1410' if 'sun' in s.features else '#3b2a1c'
            out.append(f'<path d="{trunk}" stroke="{col}" stroke-width="{4 + 26 * u:.1f}" fill="none" stroke-linecap="round"/>')
            tx, ty = x + lean, y - h
            leaf = '#14200f' if 'sun' in s.features else '#2f7a2e'
            for a in range(9):
                ang = a / 9 * math.pi * 2 + r.uniform(-0.2, 0.2)
                ln = h * r.uniform(0.32, 0.45)
                ex, ey = tx + math.cos(ang) * ln, ty + math.sin(ang) * ln * 0.5 + ln * 0.35
                cx, cy = tx + math.cos(ang) * ln * 0.5, ty - ln * 0.15
                out.append(f'<path d="M{tx:.0f},{ty:.0f} Q{cx:.0f},{cy:.0f} {ex:.0f},{ey:.0f}" stroke="{leaf}" '
                           f'stroke-width="{3 + 16 * u:.1f}" fill="none" stroke-linecap="round"/>')
    return '\n'.join(out)


def jungle(r, s):
    out = [hills(r, s, HORIZON - 40, 160, mix(s.ground, s.sky_low, 0.45)),
           hills(r, s, HORIZON + 10, 120, mix(s.ground, s.sky_low, 0.2))]
    for side in (-1, 1):
        for u in depths(26, 1.0, 0.06):
            x, y = pt(s, u, side * r.uniform(1.4, 2.6))
            rad = 260 * u * r.uniform(0.7, 1.3)
            for _ in range(4):
                out.append(f'<circle cx="{x + r.uniform(-1, 1) * rad:.0f}" cy="{y - rad * r.uniform(0.6, 1.8):.0f}" '
                           f'r="{rad * r.uniform(0.5, 0.9):.0f}" fill="{mix(s.ground, "#6fb03a", r.uniform(0.1, 0.5))}"/>')
    return '\n'.join(out)


def speedway(r, s):
    """The outside wall, a grandstand along it, flags and the infield."""
    out = []
    side = -1 if s.bend > 0 else 1
    # infield grass on the inside, the grandstand behind the outside wall
    stand = [(0 if side < 0 else W, HORIZON - 260)]
    for u in depths(30, 1.0, 0.04):
        stand.append(pt(s, u, side * 1.55, 0.7))
    stand.append((0 if side < 0 else W, H))
    stand_col = mix('#3a4256', s.sky_low, 0.2) if not s.night else '#141a2c'
    out.append(poly(stand, stand_col))
    # the crowd: dots of colour on the stand
    for _ in range(1800):
        u = r.uniform(0.06, 0.95)
        x, y = pt(s, u, side * r.uniform(1.6, 3.4), r.uniform(0.05, 0.6))
        col = r.choice(['#ffffff', '#ff4040', '#ffd23f', '#3fa0ff', '#20232c'])
        out.append(f'<rect x="{x:.0f}" y="{y:.0f}" width="{2 + 6 * u:.0f}" height="{2 + 6 * u:.0f}" fill="{col}" '
                   f'opacity="{0.5 if s.night else 0.8}"/>')
    # the wall: white concrete, a coloured band
    us = depths(50, 1.0, 0.04)
    for u0, u1 in zip(us, us[1:]):
        a0, b0 = pt(s, u0, side * 1.12), pt(s, u0, side * 1.12, 0.16)
        a1, b1 = pt(s, u1, side * 1.12), pt(s, u1, side * 1.12, 0.16)
        out.append(poly([a0, a1, b1, b0], '#e9edf2' if not s.night else '#9aa6c0'))
        m0, m1 = pt(s, u0, side * 1.12, 0.08), pt(s, u1, side * 1.12, 0.08)
        out.append(poly([m0, m1, b1, b0], '#d0342c'))
    # banners along the top of the wall
    for i, u in enumerate(depths(14, 0.9, 0.08)):
        x, y = pt(s, u, side * 1.12, 0.3)
        w, h = 160 * u, 60 * u
        col = ['#ffd23f', '#1f5fd6', '#d0342c', '#ffffff'][i % 4]
        out.append(f'<rect x="{x - w / 2:.0f}" y="{y:.0f}" width="{w:.0f}" height="{h:.0f}" fill="{col}"/>')
    return '\n'.join(out)


def floodlights(r, s):
    out = []
    for k in range(6):
        x = 260 + k * 420 + r.randint(-60, 60)
        top = HORIZON - 420 - r.randint(0, 120)
        out.append(f'<line x1="{x}" y1="{HORIZON - 40}" x2="{x}" y2="{top}" stroke="#2a3248" stroke-width="10"/>')
        out.append(f'<rect x="{x - 60}" y="{top - 40}" width="120" height="40" fill="#f4f7ff"/>')
        out.append(f'<ellipse cx="{x}" cy="{top - 20}" rx="320" ry="180" fill="#dfe8ff" opacity="0.28" filter="url(#blur60)"/>')
    return '\n'.join(out)


def road(r, s):
    ground = mix(s.ground, s.sky_low, 0.1)
    out = [f'<rect x="0" y="{HORIZON}" width="{W}" height="{H - HORIZON}" fill="{ground}"/>']
    if 'sea_right' in s.features:
        out.append(sea(s, 1))
    if 'sea_left' in s.features:
        out.append(sea(s, -1))
    us = depths(60)
    if s.road == 'dirt':
        base, edge = '#a8794a', '#7a5532'
    elif s.night:
        base, edge = '#2a2f3a', '#1c2029'
    else:
        base, edge = '#5b5f68', '#43474f'
    for u0, u1 in zip(us, us[1:]):
        a, b = pt(s, u0, -1), pt(s, u0, 1)
        c, d = pt(s, u1, 1), pt(s, u1, -1)
        out.append(poly([a, b, c, d], mix(edge, base, u0)))
    if s.road == 'dirt':
        for lat in (-0.45, 0.45):
            pts = [pt(s, u, lat) for u in us]
            out.append(f'<polyline points="{" ".join(f"{x:.0f},{y:.0f}" for x, y in pts)}" stroke="#6a4a2a" '
                       f'stroke-width="40" fill="none" opacity="0.35"/>')
    else:
        for lat in (-0.94, 0.94):
            pts = [pt(s, u, lat) for u in us]
            out.append(f'<polyline points="{" ".join(f"{x:.0f},{y:.0f}" for x, y in pts)}" stroke="#f4f4f0" '
                       f'stroke-width="8" fill="none" opacity="0.85"/>')
        dash = depths(36, 1.0, 0.05)
        for i in range(0, len(dash) - 1, 2):
            u0, u1 = dash[i], dash[i + 1]
            (xa, ya), (xb, yb) = pt(s, u0, 0), pt(s, u1, 0)
            w0, w1 = 2 + 14 * u0, 2 + 14 * u1
            col = '#f6d046' if 'speedway' not in s.features else '#ffffff'
            out.append(poly([(xa - w0, ya), (xa + w0, ya), (xb + w1, yb), (xb - w1, yb)], col, 'opacity="0.85"'))
    return '\n'.join(out)


def tail_lights(r, s):
    out = []
    for u, lat in ((0.24, 0.15), (0.11, -0.25)):
        cx, y = pt(s, u, lat)
        wd = 150 * u
        out.append(f'<rect x="{cx - wd * 1.1:.0f}" y="{y - wd * 0.8:.0f}" width="{wd * 2.2:.0f}" height="{wd * 0.8:.0f}" '
                   f'rx="{wd * 0.2:.0f}" fill="#d0342c"/>')
        out.append(f'<rect x="{cx - wd * 0.9:.0f}" y="{y - wd * 1.25:.0f}" width="{wd * 1.8:.0f}" height="{wd * 0.5:.0f}" '
                   f'rx="{wd * 0.2:.0f}" fill="#1a1e26"/>')
        for dx in (-1, 1):
            lx = cx + dx * wd * 0.78
            out.append(f'<rect x="{lx - wd * 0.2:.0f}" y="{y - wd * 0.55:.0f}" width="{wd * 0.4:.0f}" height="{wd * 0.18:.0f}" '
                       f'rx="2" fill="#ff7a5a"/>')
            if s.night or 'sun' in s.features:
                out.append(f'<circle cx="{lx:.0f}" cy="{y - wd * 0.46:.0f}" r="{wd * 0.4:.0f}" fill="#ff3b30" '
                           f'opacity="0.45" filter="url(#blur6)"/>')
    return '\n'.join(out)


def dust(r, s):
    out = []
    cx, y = pt(s, 0.24, 0.15)
    for _ in range(14):
        out.append(f'<ellipse cx="{cx + r.uniform(-200, 200):.0f}" cy="{y + r.uniform(-60, 20):.0f}" '
                   f'rx="{r.uniform(80, 220):.0f}" ry="{r.uniform(30, 80):.0f}" fill="#e8cfa4" opacity="0.25" filter="url(#blur24)"/>')
    return '\n'.join(out)


def svg(s):
    r = random.Random(s.seed)
    parts = [f'<svg xmlns="http://www.w3.org/2000/svg" width="{W}" height="{H}" viewBox="0 0 {W} {H}">', defs(s), sky(r, s)]
    if 'clouds' in s.features:
        parts.append(clouds(r, s, 9, 80, HORIZON - 260))
    if 'clouds_big' in s.features:
        parts.append(clouds(r, s, 7, 60, HORIZON - 320, big=True))
    if 'clouds_low' in s.features:
        parts.append(clouds(r, s, 6, HORIZON - 260, HORIZON - 80))
    if 'floodlights' in s.features:
        parts.append(floodlights(r, s))
    if 'city' in s.features:
        parts.append(city(r, s))
    if not ({'city', 'speedway', 'jungle'} & s.features):
        parts.append(hills(r, s, HORIZON - 20, 90, mix(s.ground, s.sky_low, 0.55)))
    parts.append(road(r, s))
    if 'cliffs' in s.features:
        parts.append(cliffs(r, s))
    if 'jungle' in s.features:
        parts.append(jungle(r, s))
    if 'speedway' in s.features:
        parts.append(speedway(r, s))
    parts.append(tail_lights(r, s))
    if 'dust' in s.features:
        parts.append(dust(r, s))
    if 'palms' in s.features:
        parts.append(palms(r, s))
    parts.append(f'<rect width="{W}" height="{H}" fill="url(#vig)"/>')
    parts.append('</svg>')
    return '\n'.join(parts)


def main():
    out = sys.argv[1]
    os.makedirs(out, exist_ok=True)
    from PIL import Image
    for s in SCENES:
        svg_path = os.path.join(out, s.slug + '.svg')
        png_path = os.path.join(out, s.slug + '.png')
        with open(svg_path, 'w') as f:
            f.write(svg(s))
        subprocess.run(['rsvg-convert', '-o', png_path, svg_path], check=True)
        im = Image.open(png_path).convert('RGB')
        grain = Image.frombytes('L', im.size, random.Random(s.seed).randbytes(im.size[0] * im.size[1]))
        im = Image.blend(im, grain.convert('RGB'), 0.025)
        im.resize((1920, 1080), Image.LANCZOS).save(os.path.join(out, s.slug + '.jpg'), quality=86, optimize=True, progressive=True)
        im.resize((512, 288), Image.LANCZOS).save(os.path.join(out, s.slug + '-tile.jpg'), quality=86)
        os.remove(png_path)
        os.remove(svg_path)
        print(s.slug)


if __name__ == '__main__':
    main()

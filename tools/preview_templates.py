"""Render a generated template DXF to PNG for quick layout review.

The app is the final judge of a template, but rendering the plain DXF with Pillow is
much faster while tuning title-block geometry: lines, circles, arcs, polylines and the
TEXT placeholders are drawn to scale on the paper extent.

Usage:  py -3.13 tools/preview_templates.py templates/iec/A3_landscape.dxf [out.png]
"""

from __future__ import annotations

import math
import os
import sys

from PIL import Image, ImageDraw, ImageFont

import dxfio

FONT_CANDIDATES = [
    r"C:\Windows\Fonts\arial.ttf",
    r"C:\Windows\Fonts\segoeui.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
]


def font_for(size_px):
    for path in FONT_CANDIDATES:
        if os.path.exists(path):
            try:
                return ImageFont.truetype(path, max(4, int(round(size_px))))
            except OSError:
                pass
    return ImageFont.load_default()


def entity_points(entity):
    kind = entity.kind
    if kind == "LINE":
        return [(entity.f(10), entity.f(20)), (entity.f(11), entity.f(21))]
    if kind == "CIRCLE":
        cx, cy, r = entity.f(10), entity.f(20), entity.f(40)
        return [(cx - r, cy - r), (cx + r, cy + r)]
    if kind == "ARC":
        cx, cy, r = entity.f(10), entity.f(20), entity.f(40)
        return [(cx - r, cy - r), (cx + r, cy + r)]
    if kind == "LWPOLYLINE":
        points = []
        for code, value in entity.pairs:
            if code == 10:
                points.append([float(value), 0.0])
            elif code == 20 and points:
                points[-1][1] = float(value)
        return [tuple(point) for point in points]
    if kind == "TEXT":
        x, y, height = entity.f(10), entity.f(20), entity.f(40)
        value = entity.s(1)
        return [(x, y), (x + len(value) * height * 0.62, y + height)]
    return []


def render(path, out_path, pixels_per_mm=3.2, margin_mm=6.0):
    drawing = dxfio.load(path)
    points = []
    for entity in drawing.entities:
        points.extend(entity_points(entity))
    if not points:
        raise SystemExit(f"no geometry in {path}")
    min_x = min(p[0] for p in points) - margin_mm
    max_x = max(p[0] for p in points) + margin_mm
    min_y = min(p[1] for p in points) - margin_mm
    max_y = max(p[1] for p in points) + margin_mm

    scale = pixels_per_mm
    width = int(round((max_x - min_x) * scale))
    height = int(round((max_y - min_y) * scale))

    image = Image.new("RGB", (width, height), "white")
    draw = ImageDraw.Draw(image)

    def to_px(x, y):
        return ((x - min_x) * scale, (max_y - y) * scale)

    for entity in drawing.entities:
        kind = entity.kind
        if kind == "LINE":
            draw.line(
                [to_px(entity.f(10), entity.f(20)), to_px(entity.f(11), entity.f(21))],
                fill="black",
                width=1,
            )
        elif kind == "CIRCLE":
            cx, cy, r = entity.f(10), entity.f(20), entity.f(40)
            x0, y0 = to_px(cx - r, cy + r)
            x1, y1 = to_px(cx + r, cy - r)
            draw.ellipse([x0, y0, x1, y1], outline="black", width=1)
        elif kind == "ARC":
            cx, cy, r = entity.f(10), entity.f(20), entity.f(40)
            start, end = entity.f(50), entity.f(51)
            points_px = [
                to_px(*dxfio.arc_points(cx, cy, r, start, end, steps=1)[0])
            ]
            steps = 48
            points_px = []
            for step in range(steps + 1):
                angle = math.radians(start + (end - start) * step / steps)
                points_px.append(
                    to_px(cx + r * math.cos(angle), cy + r * math.sin(angle))
                )
            draw.line(points_px, fill="black", width=1)
        elif kind == "LWPOLYLINE":
            pairs = []
            for code, value in entity.pairs:
                if code == 10:
                    pairs.append([float(value), 0.0])
                elif code == 20 and pairs:
                    pairs[-1][1] = float(value)
            points_px = [to_px(x, y) for x, y in pairs]
            if entity.i(70) & 1 and points_px:
                points_px.append(points_px[0])
            if len(points_px) > 1:
                draw.line(points_px, fill="black", width=1)
        elif kind == "TEXT":
            x, y, height_mm = entity.f(10), entity.f(20), entity.f(40)
            value = entity.s(1)
            if not value:
                continue
            px_height = max(4, int(round(height_mm * scale * 1.30)))
            font = font_for(px_height)
            position = to_px(x, y)
            # DXF TEXT is anchored at its baseline; Pillow's default anchor is the top
            # left corner, which would make every caption look shifted down.
            draw.text(position, value, fill="black", font=font, anchor="ls")

    image.save(out_path)
    print(f"{out_path}  {width}x{height}px  ({max_x - min_x:.1f} x {max_y - min_y:.1f} mm)")
    return out_path


def main(argv):
    if len(argv) < 2:
        print(__doc__)
        return 2
    source = argv[1]
    target = argv[2] if len(argv) > 2 else os.path.splitext(source)[0] + ".png"
    render(source, target)
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))

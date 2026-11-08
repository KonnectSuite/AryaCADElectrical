"""Generate parametric electrical symbols: N-channel modules and terminal strips.

The app builds one module interactively with ``ELECMAKEMODULE``; this tool does the same
family in bulk and writes plain library DXFs, so a range such as 4/8/16/32 channels can
be produced in one command and loaded with ``ELECLIB`` straight away.

Geometry matches ``ELECMAKEMODULE`` in ``src/app/commands/elec.rs`` on purpose: body width
0.8 units, a lead 0.15 long per channel, invisible ``X1TERMnn``/``X4TERMnn`` pin attributes
on the leads (the tag the netlist understands) and the four component attributes down the
right-hand side.

Usage::

    py -3.13 tools/symbol_maker.py --out library/modules MODULE-8:8 MODULE-16:16
    py -3.13 tools/symbol_maker.py --out library/modules --sides left STRIP-12:12
    py -3.13 tools/symbol_maker.py --out library/modules --class "Terminal Strip" STRIP-24:24

A spec is ``NAME:CHANNELS`` or ``NAME:CHANNELS:SIDES`` where SIDES is ``both`` (default),
``left`` or ``right``. Existing files are left alone unless ``--force`` is given, so the
tool is safe to re-run after adding one size.
"""

from __future__ import annotations

import argparse
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import dxfio

WIDTH = 0.8
LEAD = 0.15
MANIFEST = "library.tsv"
HEADER = "symbol\tclass\tattributes\tsheets"
COMPONENT_ATTRIBUTES = (
    ("TAG1", "Tag"),
    ("CAT", "Part number"),
    ("MFG", "Manufacturer"),
    ("DESC1", "Description"),
)


def parse_spec(spec: str, default_sides: str) -> tuple[str, int, str]:
    parts = spec.split(":")
    if len(parts) not in (2, 3):
        raise ValueError(f"{spec!r} is not NAME:CHANNELS[:SIDES]")
    name, channels = parts[0].strip(), parts[1]
    sides = (parts[2] if len(parts) == 3 else default_sides).strip().lower()
    if sides not in ("both", "left", "right"):
        raise ValueError(f"{spec!r}: sides must be both, left or right")
    count = int(channels)
    if not 1 <= count <= 64:
        raise ValueError(f"{spec!r}: channels must be between 1 and 64")
    if not name:
        raise ValueError(f"{spec!r}: the symbol needs a name")
    return name, count, sides


def build_module(path: str, name: str, channels: int, sides: str, pitch: float) -> list[str]:
    """Write one module DXF and return its attribute tags in manifest order."""
    with_left = sides != "right"
    with_right = sides != "left"
    top = channels * pitch
    label = min(max(pitch * 0.4, 0.05), 0.5)

    document = dxfio.DxfDocument()
    document.add_lwpolyline(
        [(0.0, 0.0), (WIDTH, 0.0), (WIDTH, top), (0.0, top)], closed=True
    )
    tags: list[str] = []
    pins: list[tuple[str, float, float]] = []
    for channel in range(channels):
        y = (channel + 0.5) * pitch
        if with_left:
            document.add_line(-LEAD, y, 0.0, y)
            pins.append((f"X4TERM{channel + 1:02d}", -LEAD, y))
        if with_right:
            document.add_line(WIDTH, y, WIDTH + LEAD, y)
            pins.append((f"X1TERM{channel + 1:02d}", WIDTH + LEAD, y))

    for tag, x, y in pins:
        document.add_raw(dxfio.DxfDocument.attdef(tag, "Terminal", "", label, x, y, flags=1))
        tags.append(tag)

    for row, (tag, prompt) in enumerate(COMPONENT_ATTRIBUTES):
        y = top - (row + 1) * pitch * 1.4
        document.add_raw(dxfio.DxfDocument.attdef(tag, prompt, "", label, WIDTH + 0.3, y))
        tags.append(tag)

    document.save(path)
    return tags


def append_manifest(folder: str, name: str, symbol_class: str, tags: list[str]) -> None:
    path = os.path.join(folder, MANIFEST)
    lines: list[str] = []
    if os.path.exists(path):
        with open(path, "r", encoding="utf-8") as handle:
            lines = handle.read().splitlines()
        if not lines or not lines[0].startswith("symbol\t"):
            lines.insert(0, HEADER)
    else:
        lines = [HEADER]
    row = f"{name}\t{symbol_class}\t{'|'.join(tags)}\t1"
    lines = [line for line in lines if line.split("\t", 1)[0] != name]
    lines.append(row)
    with open(path, "w", encoding="utf-8", newline="\n") as handle:
        handle.write("\n".join(lines) + "\n")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Generate parametric module symbols.")
    parser.add_argument("--out", required=True, help="target library folder")
    parser.add_argument("--class", dest="symbol_class", default="Module", help="palette class")
    parser.add_argument("--pitch", type=float, default=0.25, help="channel spacing in units")
    parser.add_argument("--sides", default="both", choices=("both", "left", "right"))
    parser.add_argument("--force", action="store_true", help="overwrite existing DXFs")
    parser.add_argument("specs", nargs="+", help="NAME:CHANNELS[:SIDES]")
    args = parser.parse_args(argv)

    os.makedirs(args.out, exist_ok=True)
    pitch = min(max(args.pitch, 0.05), 5.0)
    written = skipped = 0
    for spec in args.specs:
        try:
            name, channels, sides = parse_spec(spec, args.sides)
        except ValueError as error:
            print(f"symbol_maker: {error}", file=sys.stderr)
            return 2
        path = os.path.join(args.out, f"{name}.dxf")
        if os.path.exists(path) and not args.force:
            print(f"symbol_maker: {name}.dxf exists, skipped (use --force)")
            skipped += 1
            continue
        tags = build_module(path, name, channels, sides, pitch)
        append_manifest(args.out, name, args.symbol_class, tags)
        print(f"symbol_maker: {name} - {channels} channel(s), {sides} leads -> {path}")
        written += 1
    print(f"symbol_maker: {written} written, {skipped} skipped in {args.out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

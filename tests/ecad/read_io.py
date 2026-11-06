#!/usr/bin/env python3
"""Read a saved DXF back and show the PLC modules ELECIOIMPORT generated.

Verification is done against the file on disk, not against the command's own output:

    python tests/ecad/read_io.py <saved.dxf>

It prints every `PLC-*` insert with its attributes and checks that the addresses the
import should have produced are all there. Exit code 0 when the expected addresses are
present, 1 when they are not, 2 when the file cannot be read.
"""

from __future__ import annotations

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "tools"))

import dxfio  # noqa: E402  (path set up above)

# What tests/ecad/io_list_sample.xlsx describes: three cards, 18 points.
EXPECTED = (
    [f"I0.{bit}" for bit in range(8)]
    + [f"Q0.{bit}" for bit in range(6)]
    + [f"%AI0.{bit}" for bit in range(4)]
)


def attributes(insert) -> list[tuple[str, str]]:
    return [(attrib.s(2), attrib.s(1)) for attrib in insert.attributes]


def main(argv: list[str]) -> int:
    if len(argv) != 2:
        print("usage: read_io.py <saved.dxf>", file=sys.stderr)
        return 2
    path = Path(argv[1])
    try:
        drawing = dxfio.load(str(path))
    except OSError as error:
        print(f"cannot read {path}: {error}", file=sys.stderr)
        return 2

    modules = []
    for entity in drawing.entities:
        if entity.kind != "INSERT":
            continue
        block = entity.s(2)
        if not block.upper().startswith("PLC-"):
            continue
        modules.append(entity)

    print(f"{path.name}: {len(drawing.entities)} model-space entity/entities, "
          f"{len(modules)} PLC module(s)")
    for number, insert in enumerate(modules, start=1):
        attributes_ = attributes(insert)
        values = dict(attributes_)
        addresses = [value for tag, value in attributes_ if tag.upper().startswith("ADDR")]
        descs = [value for tag, value in attributes_ if tag.upper().startswith("DESC0")]
        print(f"\n  module {number}: block={insert.s(2)} at "
              f"({insert.f(10):.2f},{insert.f(20):.2f})")
        print(f"    TAG1={values.get('TAG1', '')!r}  CARD={values.get('CARD', '')!r}  "
              f"IOTYPE={values.get('IOTYPE', '')!r}")
        print(f"    DESC1={values.get('DESC1', '')!r}  MODULE={values.get('MODULE', '')!r}")
        print(f"    addresses ({len(addresses)}): {', '.join(addresses)}")
        if descs:
            print(f"    point descriptions ({len(descs)}): {descs[0]!r} ... {descs[-1]!r}")
        x1 = [value for tag, value in attributes_ if tag.upper().startswith("X1TERM")]
        if x1:
            print(f"    X1TERM pin coordinates ({len(x1)}): {x1[0]} ... {x1[-1]}")

    found = []
    for insert in modules:
        found.extend(value for tag, value in attributes(insert) if tag.upper().startswith("ADDR"))
    missing = [address for address in EXPECTED if address not in found]
    extra = [address for address in found if address not in EXPECTED]
    print(f"\naddresses read back: {len(found)} (expected {len(EXPECTED)})")
    if missing:
        print(f"MISSING: {missing}")
    if extra:
        print(f"UNEXPECTED: {extra}")
    if not missing and not extra and len(found) == len(EXPECTED):
        print("PASS: every expected address is present on the saved drawing")
        return 0
    print("FAIL: the saved drawing does not carry the expected I/O points")
    return 1


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))

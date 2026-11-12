"""Check the shipped symbol libraries: every symbol's connection points are unique.

The electrical layer reads the digits after ``TERM`` in an ``X4TERM01``-style attribute
as the pin number, and the digit after ``X`` only says which side the wire approaches
from (``crates/elec/src/pins.rs``).  A symbol that numbers both edges from 1 therefore
has two connection points that read as one pin, and the wire list prints
``-K1:1 -> -K1:1`` instead of ``-K1:1 -> -M1:2``.  This tool fails the build if that
happens again, and it also checks that the manifest lists the pins the symbol declares.

    py -3.13 tools/pin_check.py                 # the shipped libraries
    py -3.13 tools/pin_check.py library/starter # one folder
"""

from __future__ import annotations

import os
import re
import sys

PIN = re.compile(r"^X(\d)TERM(\d+)$")


def entities(path):
    """Yield (type, {code: [values...]}) for every entity in a DXF text file.

    The shipped symbol files are fragments - geometry plus ATTDEF definitions in model
    space, with no BLOCK record of their own, because ``ELECLIB`` turns each one into a
    block when it loads it.  So the symbol name is the file stem.  The scanner resyncs
    on a non-numeric line instead of trusting that group codes sit on even offsets.
    """
    with open(path, encoding="utf-8", errors="replace") as handle:
        lines = handle.read().splitlines()
    current = None
    fields = {}
    index = 0
    while index < len(lines):
        code = lines[index].strip()
        if not code.isdigit():
            index += 1
            continue
        value = lines[index + 1].strip() if index + 1 < len(lines) else ""
        index += 2
        if code == "0":
            if current is not None:
                yield current, fields
            current = value
            fields = {}
        else:
            fields.setdefault(code, []).append(value)
    if current is not None:
        yield current, fields


def symbol_pins(path):
    """The connection points a symbol file declares, as (tag, number, x, y)."""
    pins = []
    for etype, fields in entities(path):
        if etype != "ATTDEF":
            continue
        tag = (fields.get("2") or [""])[0].strip().upper()
        match = PIN.match(tag)
        if not match:
            continue
        pins.append(
            (
                tag,
                int(match.group(2)),
                float((fields.get("10") or ["0"])[0]),
                float((fields.get("20") or ["0"])[0]),
            )
        )
    return pins


def check_folder(folder):
    """Return (problems, symbols_with_pins) for one library folder."""
    problems = []
    manifest = {}
    manifest_path = os.path.join(folder, "library.tsv")
    if os.path.exists(manifest_path):
        with open(manifest_path, encoding="utf-8") as handle:
            for line in handle.read().splitlines():
                if not line or line.startswith("#") or line.startswith("symbol\t"):
                    continue
                columns = line.split("\t")
                if len(columns) < 3:
                    continue
                manifest[columns[0]] = set(columns[2].split("|"))
    pinned = 0
    for name in sorted(os.listdir(folder)):
        if not name.endswith(".dxf"):
            continue
        path = os.path.join(folder, name)
        pins = symbol_pins(path)
        if not pins:
            continue
        pinned += 1
        symbol = os.path.splitext(name)[0]
        numbers = {}
        for tag, number, x, y in pins:
            numbers.setdefault(number, []).append((tag, x, y))
        for number, holders in sorted(numbers.items()):
            if len(holders) > 1:
                where = ", ".join(f"{tag} at {x:+.3f},{y:+.3f}" for tag, _, _ in holders)
                problems.append(
                    f"{symbol}: pin {number} is claimed by {len(holders)} connection "
                    f"points ({where})"
                )
        listed = manifest.get(symbol)
        if listed is None:
            problems.append(f"{symbol}: no row in library.tsv")
            continue
        declared = {tag for tag, _, _, _ in pins}
        missing = sorted(declared - listed)
        if missing:
            problems.append(f"{symbol}: library.tsv does not list {', '.join(missing)}")
        extra = sorted(tag for tag in listed - declared if PIN.match(tag))
        if extra:
            problems.append(f"{symbol}: library.tsv lists {', '.join(extra)}, which the symbol does not declare")
    return problems, pinned


def main(argv):
    roots = argv[1:] or [
        os.path.join("library", "starter"),
        os.path.join("library", "nec"),
    ]
    problems = []
    pinned = 0
    for root in roots:
        if not os.path.isdir(root):
            print(f"pin_check: no such folder: {root}")
            return 2
        found, count = check_folder(root)
        pinned += count
        problems.extend(f"{root}: {problem}" for problem in found)
    if problems:
        print(f"pin_check: {len(problems)} problem(s) in {pinned} pinned symbol(s)")
        for problem in problems:
            print(f"  {problem}")
        return 1
    print(f"pin_check: {pinned} pinned symbol(s), every connection point uniquely numbered")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))

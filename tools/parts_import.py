#!/usr/bin/env python3
"""Normalise a rough parts spreadsheet into AryaCAD's ``parts.tsv``.

What it is for
--------------
The local parts catalogue (``library/parts/parts.tsv``) is the offline replacement for a
vendor's online parts library. A workshop already has its parts in a spreadsheet exported
from the ERP, on a purchase-order sheet, or typed up once in whatever column order made
sense that day. This tool turns such a sheet into the catalogue's one canonical table
without asking anyone to retype it.

It accepts a CSV whose header may be spelled the way people actually spell it —
``PN``/``Mfr``/``Cat``/``Desc``/``Spec`` — and writes the nine canonical columns:

    part_number  manufacturer  description  class  symbol  rating  terminals  footprint  datasheet

Usage
-----
    py -3.13 tools/parts_import.py --sample
    py -3.13 tools/parts_import.py rough.csv
    py -3.13 tools/parts_import.py rough.csv library/parts/parts.tsv
    py -3.13 tools/parts_import.py --sample sample.csv

``--sample`` prints a small rough CSV (with deliberately non-canonical headers) that shows
the shape the importer accepts; give it a path to write the file instead of printing it.
Without ``--sample`` the first argument is the rough sheet to read and the second is the
``parts.tsv`` to write (default ``library/parts/parts.tsv``).

Exit code is 0 on success, 1 when the sheet has no usable part-number column or no rows.
"""

from __future__ import annotations

import csv
import io
import os
import sys

# Canonical output columns, in the order parts.tsv stores them.
COLUMNS = [
    "part_number",
    "manufacturer",
    "description",
    "class",
    "symbol",
    "rating",
    "terminals",
    "footprint",
    "datasheet",
]

# Header synonyms, matched after normalising the cell (lower-case, `_`/`-` -> space).
ALIASES = {
    "part_number": ["part number", "part", "partnumber", "part no", "cat", "catalogue id", "pn"],
    "manufacturer": ["manufacturer", "mfg", "mfr", "make", "brand"],
    "description": ["description", "desc", "details", "detail"],
    "class": ["class", "category", "part class", "type"],
    "symbol": ["symbol", "symbol name", "block", "drawing symbol"],
    "rating": ["rating", "ratings", "spec", "specification"],
    "terminals": ["terminals", "terminal", "terminal count", "poles", "connections", "ways"],
    "footprint": ["footprint", "mounting", "package"],
    "datasheet": ["datasheet", "datasheet url", "datasheet reference", "documentation", "doc"],
}

# A deliberately rough sheet: the headers are the kind of thing a hand-made export has.
SAMPLE_HEADERS = ["PN", "Mfr", "Desc", "Category", "Symbol", "Spec", "Terminals"]
SAMPLE_ROWS = [
    ["LPJ-100SP", "Bussmann", "Fuse, Class J, time-delay, 100 A, 600 V", "Fuse", "FUSE", "100 A / 600 V", "2"],
    ["FAZ-C6/1-NA-SP", "Eaton", "Miniature circuit breaker, 1-pole, C-curve, 6 A", "Circuit Breaker", "MCB", "6 A / 277 V AC", "2"],
    ["100-C09EJ300", "Allen-Bradley", "Contactor, 9 A, 3-pole, 24 V DC coil", "Contactor", "CONTACT_NO", "9 A / 24 V DC coil", "6"],
]


def normalise_header(value: str) -> str:
    """`Part_Number`, `part  number`, `PART NUMBER` all become `part number`."""
    cleaned = value.strip().lower().replace("_", " ").replace("-", " ")
    return " ".join(cleaned.split())


def build_column_map(header: list[str]) -> dict[str, int]:
    """Map canonical column name -> index in the input row."""
    normalised = [normalise_header(cell) for cell in header]
    mapping: dict[str, int] = {}
    for column, names in ALIASES.items():
        for index, cell in enumerate(normalised):
            if cell in names:
                mapping[column] = index
                break
    return mapping


def row_value(cells: list[str], mapping: dict[str, int], column: str) -> str:
    index = mapping.get(column)
    if index is None or index >= len(cells):
        return ""
    return cells[index].strip()


def read_rough_sheet(text: str) -> tuple[list[list[str]], dict[str, int]]:
    """Read a CSV/TSV with comma, semicolon, tab or pipe delimiter."""
    sample = text[:4096]
    try:
        dialect = csv.Sniffer().sniff(sample, delimiters=",;\t|")
    except csv.Error:
        dialect = csv.excel
    reader = csv.reader(io.StringIO(text), dialect)
    rows = [row for row in reader if any(cell.strip() for cell in row)]
    if not rows:
        return [], {}
    return rows[1:], build_column_map(rows[0])


def normalise_rows(rows: list[list[str]], mapping: dict[str, int]) -> tuple[list[dict[str, str]], list[str]]:
    """Turn rough rows into canonical ones; later duplicates win."""
    warnings: list[str] = []
    by_part: dict[str, dict[str, str]] = {}
    order: list[str] = []
    for number, cells in enumerate(rows, start=2):
        part_number = row_value(cells, mapping, "part_number")
        if not part_number:
            warnings.append(f"line {number}: no part number — row skipped")
            continue
        row = {column: row_value(cells, mapping, column) for column in COLUMNS}
        row["terminals"] = row["terminals"] or "0"
        try:
            int(row["terminals"])
        except ValueError:
            warnings.append(
                f"line {number}: terminals {row['terminals']!r} is not a count — using 0"
            )
            row["terminals"] = "0"
        key = part_number.upper()
        if key in by_part:
            warnings.append(f"line {number}: part {part_number} appears earlier — the later row wins")
        else:
            order.append(key)
        by_part[key] = row
    return [by_part[key] for key in order], warnings


def write_parts_tsv(rows: list[dict[str, str]], path: str) -> None:
    """Write the canonical tab-separated catalogue (UTF-8, LF, no BOM)."""
    parent = os.path.dirname(os.path.abspath(path))
    if parent:
        os.makedirs(parent, exist_ok=True)
    lines = ["\t".join(COLUMNS)]
    for row in sorted(rows, key=lambda entry: entry["part_number"].upper()):
        lines.append("\t".join(row.get(column, "") for column in COLUMNS))
    with open(path, "w", encoding="utf-8", newline="\n") as handle:
        handle.write("\n".join(lines) + "\n")


def sample_csv() -> str:
    buffer = io.StringIO()
    writer = csv.writer(buffer, lineterminator="\n")
    writer.writerow(SAMPLE_HEADERS)
    for row in SAMPLE_ROWS:
        writer.writerow(row)
    return buffer.getvalue()


def main(argv: list[str]) -> int:
    for stream in (sys.stdout, sys.stderr):
        if hasattr(stream, "reconfigure"):
            stream.reconfigure(encoding="utf-8", errors="replace")

    args = [arg for arg in argv[1:] if arg]
    root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    default_out = os.path.join(root, "library", "parts", "parts.tsv")

    if any(arg in ("-h", "--help") for arg in args):
        print(__doc__)
        return 0

    if "--sample" in args:
        rest = [arg for arg in args if arg != "--sample"]
        text = sample_csv()
        if rest:
            with open(rest[0], "w", encoding="utf-8", newline="\n") as handle:
                handle.write(text)
            print(f"wrote a {len(SAMPLE_ROWS)}-row sample sheet to {rest[0]}")
        else:
            sys.stdout.write(text)
        return 0

    if not args:
        print("parts_import: give a rough CSV to import, or --sample. Example:")
        print("    py -3.13 tools/parts_import.py rough.csv library/parts/parts.tsv")
        return 1

    source = args[0]
    target = args[1] if len(args) > 1 else default_out
    try:
        with open(source, "r", encoding="utf-8-sig", errors="replace") as handle:
            text = handle.read()
    except OSError as error:
        print(f"parts_import: cannot read {source}: {error}")
        return 1

    rows, mapping = read_rough_sheet(text)
    if "part_number" not in mapping:
        print(f"parts_import: {source}: no part-number column in the header row.")
        print(f"             accepted spellings: {', '.join(ALIASES['part_number'])}")
        return 1
    normalised, warnings = normalise_rows(rows, mapping)
    if not normalised:
        print(f"parts_import: {source}: no part rows with a part number.")
        return 1

    write_parts_tsv(normalised, target)
    print(f"parts_import: read {len(rows)} rough row(s) from {source}")
    print(f"  columns matched: {', '.join(sorted(mapping))}")
    missing = [column for column in COLUMNS if column not in mapping]
    if missing:
        print(f"  not in the sheet (left empty): {', '.join(missing)}")
    print(f"  wrote {len(normalised)} part(s) to {target}")
    for warning in warnings[:5]:
        print(f"  warning: {warning}")
    if len(warnings) > 5:
        print(f"  ... {len(warnings) - 5} more warning(s)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))

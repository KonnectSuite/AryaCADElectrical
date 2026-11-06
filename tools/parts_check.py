#!/usr/bin/env python3
"""Validate the local parts catalogue, or prove the whole parts workflow headless.

Two jobs, one file:

* ``py -3.13 tools/parts_check.py --check`` validates ``library/parts/parts.tsv`` — the
  header, the mandatory columns, duplicate/blank part numbers, a terminal count that is a
  count, and that every ``symbol`` named really is a symbol the loaded symbol library draws
  (``library/library.tsv`` or ``library/starter/library.tsv``). Errors exit 1; warnings do
  not, because a hand-made catalogue is allowed to be incomplete while it is being filled.

* ``py -3.13 tools/parts_check.py --smoke`` runs the real binary on ``tests/ecad/parts_batch.txt``
  against a copy of ``tests/fixtures/embedded-image-print.dxf``, prints its stdout verbatim,
  then re-reads the *saved* DXF and the *written* BOM CSV with Python and checks that
  ``ELECASSIGN`` really put CAT/MFG/DESC1/RATING1 on the placed symbol. Nothing is trusted
  from the app's own printed summary alone.

Both jobs are read-only with respect to the repository: the smoke run works on a copy under
``tests/ecad/_work/parts/``.

Usage
-----
    py -3.13 tools/parts_check.py
    py -3.13 tools/parts_check.py --check
    py -3.13 tools/parts_check.py --smoke [--binary target\\release\\OpenCADStudio.exe]
"""

from __future__ import annotations

import argparse
import csv
import os
import shutil
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
PARTS = os.path.join(ROOT, "library", "parts", "parts.tsv")
MANIFESTS = [
    os.path.join(ROOT, "library", "library.tsv"),
    os.path.join(ROOT, "library", "starter", "library.tsv"),
]
FIXTURE = os.path.join(ROOT, "tests", "fixtures", "embedded-image-print.dxf")
BATCH = os.path.join(ROOT, "tests", "ecad", "parts_batch.txt")
WORK = os.path.join(ROOT, "tests", "ecad", "_work", "parts")
BINARY = os.path.join(ROOT, "target", "release", "OpenCADStudio.exe")

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

CHECKS: list[tuple[str, bool, str]] = []


def check(label: str, ok: bool, detail: str = "") -> bool:
    CHECKS.append((label, bool(ok), detail))
    mark = "PASS" if ok else "FAIL"
    print(f"[{mark}] {label}" + (f"  ({detail})" if detail else ""))
    return ok


def section(title: str) -> None:
    print()
    print(f"=== {title}")
    print()


# --------------------------------------------------------------------------- #
# catalogue parsing and validation
# --------------------------------------------------------------------------- #


def normalise_header(value: str) -> str:
    return " ".join(value.strip().lower().replace("_", " ").replace("-", " ").split())


def read_catalogue(path: str) -> tuple[list[dict[str, str]], list[str], list[str]]:
    """Return (rows, errors, warnings) for a parts.tsv; rows are canonical dicts."""
    errors: list[str] = []
    warnings: list[str] = []
    if not os.path.exists(path):
        return [], [f"{path} does not exist"], warnings
    with open(path, "r", encoding="utf-8-sig", errors="replace") as handle:
        lines = [line.rstrip("\r\n") for line in handle.read().splitlines()]
    lines = [line for line in lines if line.strip() and not line.strip().startswith("#")]
    if not lines:
        return [], [f"{path} is empty"], warnings

    header = [normalise_header(cell) for cell in lines[0].split("\t")]
    mapping: dict[str, int] = {}
    for column, names in ALIASES.items():
        for index, cell in enumerate(header):
            if cell in names:
                mapping[column] = index
                break
    if "part_number" not in mapping:
        return [], [f"{path}: the header has no part-number column ({', '.join(header)})"], warnings
    for column in ("manufacturer", "description", "class"):
        if column not in mapping:
            warnings.append(f"the {column} column is missing; those parts will list as blank")

    rows: list[dict[str, str]] = []
    seen: dict[str, int] = {}
    for number, line in enumerate(lines[1:], start=2):
        cells = line.split("\t")
        row = {
            column: (cells[index].strip() if index < len(cells) else "")
            for column, index in mapping.items()
        }
        for column in COLUMNS:
            row.setdefault(column, "")
        part_number = row["part_number"]
        if not part_number:
            errors.append(f"line {number}: no part number")
            continue
        key = part_number.upper()
        if key in seen:
            errors.append(
                f"line {number}: part {part_number} is already on line {seen[key]} "
                "(the app keeps the later row; a catalogue should not need that)"
            )
        else:
            seen[key] = number
        row["_line"] = str(number)
        rows.append(row)
    return rows, errors, warnings


def load_symbols() -> dict[str, str]:
    """Symbol name -> class, from every library manifest that exists."""
    symbols: dict[str, str] = {}
    for path in MANIFESTS:
        if not os.path.exists(path):
            continue
        with open(path, "r", encoding="ascii", errors="replace") as handle:
            lines = [line for line in handle.read().splitlines() if line.strip()]
        for line in lines[1:]:
            cells = line.split("\t")
            if len(cells) >= 2:
                symbols[cells[0].strip().upper()] = cells[1].strip()
    return symbols


def validate(rows: list[dict[str, str]], errors: list[str], warnings: list[str]) -> None:
    symbols = load_symbols()
    for row in rows:
        line = row["_line"]
        part = row["part_number"]
        for column in ("manufacturer", "description", "class"):
            if not row[column]:
                errors.append(f"line {line}: {part} has no {column}")
        if not row["rating"]:
            warnings.append(f"line {line}: {part} has no rating")
        if row["terminals"]:
            try:
                if int(row["terminals"]) < 0:
                    errors.append(f"line {line}: {part} has a negative terminal count")
            except ValueError:
                errors.append(f"line {line}: {part} terminal count {row['terminals']!r} is not a number")
        if row["symbol"]:
            if row["symbol"].upper() not in symbols:
                errors.append(
                    f"line {line}: {part} names symbol {row['symbol']!r}, "
                    "which no library.tsv under library/ defines"
                )
            elif symbols[row["symbol"].upper()] and row["class"] and \
                    symbols[row["symbol"].upper()].lower() != row["class"].lower():
                warnings.append(
                    f"line {line}: {part} class {row['class']!r} sits on a "
                    f"{symbols[row['symbol'].upper()]!r} symbol (legitimate for e.g. an earth "
                    "terminal; check it is intended)"
                )
        datasheet = row["datasheet"]
        if datasheet:
            if not datasheet.lower().startswith(("http://", "https://")):
                reference = datasheet if os.path.isabs(datasheet) else os.path.join(
                    os.path.dirname(PARTS), datasheet
                )
                if not os.path.exists(reference):
                    warnings.append(
                        f"line {line}: {part} datasheet {datasheet!r} is neither a URL nor a file next to parts.tsv"
                    )


def print_summary(rows, errors, warnings) -> None:
    classes: dict[str, int] = {}
    for row in rows:
        classes[row["class"] or "(none)"] = classes.get(row["class"] or "(none)", 0) + 1
    with_symbol = sum(1 for row in rows if row["symbol"])
    with_rating = sum(1 for row in rows if row["rating"])
    print()
    print(
        f"parts_check: {len(rows)} part(s), {len(classes)} class(es), "
        f"{with_symbol} naming a symbol, {with_rating} with a rating."
    )
    for name, count in sorted(classes.items()):
        print(f"  {count:>3}  {name}")
    print()
    print(f"  {len(errors)} error(s), {len(warnings)} warning(s).")
    for message in errors:
        print(f"  ERROR   {message}")
    for message in warnings[:12]:
        print(f"  warning {message}")
    if len(warnings) > 12:
        print(f"  ... {len(warnings) - 12} more warning(s)")


# --------------------------------------------------------------------------- #
# headless smoke run
# --------------------------------------------------------------------------- #


def run(binary: str, args: list[str]) -> tuple[int, str]:
    print("$ " + " ".join(args))
    proc = subprocess.run(
        args, capture_output=True, text=True, cwd=ROOT, encoding="utf-8", errors="replace"
    )
    text = proc.stdout + proc.stderr
    logs = os.path.join(WORK, "logs")
    os.makedirs(logs, exist_ok=True)
    with open(os.path.join(logs, "parts_smoke.log"), "w", encoding="utf-8") as handle:
        handle.write("$ " + " ".join(args) + "\n")
        handle.write(f"[exit code: {proc.returncode}]\n")
        handle.write(text)
    for line in text.splitlines():
        print("  | " + line)
    return proc.returncode, text


def find_insert(drawing, tag: str):
    for entity in drawing.entities:
        if entity.kind != "INSERT":
            continue
        for attribute in entity.attributes:
            if attribute.s(2).strip().upper() in {"TAG", "TAG1", "TAG2", "TAGSTR", "WORKID"}:
                if attribute.s(1).strip().upper() == tag.upper():
                    return entity
    return None


def attribute_map(insert) -> dict[str, str]:
    return {attribute.s(2).strip().upper(): attribute.s(1) for attribute in insert.attributes}


def fuse_matches(rows) -> list[dict[str, str]]:
    """Same predicate the Rust filter applies: any displayed field contains 'fuse'."""
    fields = ("part_number", "manufacturer", "description", "class", "rating", "symbol")
    return [
        row for row in rows
        if any("fuse" in row[column].lower() for column in fields)
    ]


def smoke(binary: str, batch: str) -> None:
    section("headless parts workflow (ELECPARTS -> ELECPARTLIST -> ELECINSERT -> ELECASSIGN -> ELECLIST -> ELECBOM)")
    if not os.path.exists(binary):
        check("the release binary exists", False, binary)
        return
    if not os.path.exists(FIXTURE):
        check("the fixture drawing exists", False, FIXTURE)
        return
    if os.path.isdir(WORK):
        shutil.rmtree(WORK)
    os.makedirs(WORK)
    drawing = os.path.join(WORK, "parts_smoke.dxf")
    shutil.copy2(FIXTURE, drawing)
    bom = os.path.join(WORK, "bom.csv")

    code, text = run(binary, [binary, "--each", batch, drawing])
    check("parts batch exits 0", code == 0, f"exit {code}")

    rows, errors, _ = read_catalogue(PARTS)
    if errors:
        print("  (catalogue has errors; run --check)")
    expected_count = len(rows)

    # ---- what the app printed ------------------------------------------- #
    check(
        "ELECPARTS loaded the seeded catalogue",
        f"ELECPARTS: loaded {expected_count} part(s)" in text,
        f"expected {expected_count}",
    )
    matches = fuse_matches(rows)
    expected_list = f'ELECPARTLIST: {len(matches)} of {expected_count} part(s) match "fuse".'
    check("ELECPARTLIST filtered to the fuses", expected_list in text, expected_list)
    for part in ("LPJ-100SP", "FAZ-", "Eaton", "Fuse"):
        check(f"ELECPARTLIST printed {part!r}", part in text, "")
    check("ELECINSERT placed the contactor as -K1",
          "ELECINSERT: placed CONTACT_NO as -K1" in text, "")
    check("ELECASSIGN reported writing the part",
          "ELECASSIGN: -K1 <- LPJ-100SP" in text, "")
    check("ELECASSIGN reported the fields it set",
          "set CAT, MFG, DESC1, RATING1 on 1 insert(s)" in text, "")
    check("ELECASSIGN <tag> reads the assignment back",
          "ELECASSIGN: -K1 -> LPJ-100SP" in text, "")
    check("ELECLIST shows the assigned part number",
          any(line.startswith("Contactor ") and "LPJ-100SP" in line for line in text.splitlines()),
          "")

    # ---- the saved drawing ---------------------------------------------- #
    section("read the SAVED drawing back with Python (no app summary trusted)")
    try:
        import dxfio
    except ImportError:
        sys.path.insert(0, os.path.join(ROOT, "tools"))
        import dxfio  # type: ignore

    drawing_data = dxfio.load(drawing)
    insert = find_insert(drawing_data, "-K1")
    check("the saved DXF carries an INSERT tagged -K1", insert is not None)
    if insert is not None:
        attributes = attribute_map(insert)
        print(f"         -K1 attributes on {insert.s(2)}: {attributes}")
        expected = {
            "CAT": "LPJ-100SP",
            "MFG": "Bussmann",
            "DESC1": "Fuse, Class J, time-delay, 100 A, 600 V",
            "RATING1": "100 A / 600 V",
        }
        for name, value in expected.items():
            check(f"attribute {name} = {value!r} in the saved DXF",
                  attributes.get(name) == value, attributes.get(name, "<missing>"))
        heights = {
            attribute.s(2): attribute.f(40) for attribute in insert.attributes
        }
        print(f"         attribute text heights: {heights}")
        check("the added RATING1 attribute is not at zero height",
              heights.get("RATING1", 0.0) > 0.0, heights.get("RATING1"))

    # ---- the written bill of material ----------------------------------- #
    section("read the written BOM CSV back with Python")
    check("ELECBOM wrote the CSV", os.path.exists(bom), bom)
    if os.path.exists(bom):
        with open(bom, "r", encoding="utf-8", newline="") as handle:
            bom_rows = list(csv.reader(handle))
        check(
            "BOM header is the template header",
            bom_rows and bom_rows[0] == [
                "Class name", "Part number", "Manufacturer", "Description", "Qty", "Where used"
            ],
            str(bom_rows[0] if bom_rows else None),
        )
        data = bom_rows[1:]
        check("BOM has one row", len(data) == 1, f"{len(data)} rows: {data}")
        if data:
            row = data[0]
            print(f"         BOM row: {row}")
            check("BOM row carries the assigned catalogue values",
                  row[1] == "LPJ-100SP" and row[2] == "Bussmann" and row[3].startswith("Fuse, Class J"),
                  str(row))
            check("BOM row counts one and names where it is used",
                  row[4] == "1" and row[5] == "-K1", str(row[5:]))


def main(argv: list[str] | None = None) -> int:
    global PARTS
    for stream in (sys.stdout, sys.stderr):
        if hasattr(stream, "reconfigure"):
            stream.reconfigure(encoding="utf-8", errors="replace")
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="validate library/parts/parts.tsv")
    parser.add_argument("--smoke", action="store_true", help="run the headless parts workflow")
    parser.add_argument("--binary", default=BINARY, help="the OpenCADStudio.exe to run")
    parser.add_argument("--batch", default=BATCH, help="the headless batch to run")
    parser.add_argument("--parts", default=PARTS, help="the parts.tsv to validate")
    args = parser.parse_args(argv)

    PARTS = args.parts

    rows, errors, warnings = read_catalogue(PARTS)
    section(f"catalogue: {PARTS}")
    validate(rows, errors, warnings)
    print_summary(rows, errors, warnings)
    for row in rows:
        print(f"  {row['part_number']:<24} {row['class']:<20} {row['symbol'] or '-':<18} {row['terminals']:>3}")
    check("the catalogue validates", not errors, f"{len(errors)} error(s)")

    if args.smoke:
        smoke(args.binary, args.batch)

    failed = [label for label, ok, _ in CHECKS if not ok]
    print()
    if failed:
        print(f"FAILED: {len(failed)} check(s): {failed}")
        return 1
    print(f"OK: {len(CHECKS)} check(s) passed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

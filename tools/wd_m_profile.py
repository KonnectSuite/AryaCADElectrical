"""Extract the project's AutoCAD Electrical drawing setup from the real sheets.

The ACADE per-sheet settings live in the ``WD_M`` block insert: 77 attributes
that decide tag/wire formats, layer names, rung geometry and cross-reference
styles.  Every sheet in ``import/Prints`` carries two identical ``WD_M`` inserts
in model space (measured: 68 inserts on 34 sheets, no sheet disagrees with
itself), so this script de-duplicates them and reports, per attribute, the value
and how many sheets agree with it.

Output: ``library/wd_m_profile.tsv``

    attribute       WD_M attribute tag
    value           the value every sheet agrees on (most common value otherwise)
    sheets_agreeing number of sheets carrying that exact value
    sheets_total    sheets that carry a WD_M insert at all
    sheets_present  sheets that define this attribute
    other_values    "value (n); ..." for every other observed value

Usage
-----
    py -3.13 tools/wd_m_profile.py [--prints DIR] [--out FILE] [--check]
"""

from __future__ import annotations

import argparse
import collections
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import harvest_library as H  # noqa: E402

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
DEFAULT_PRINTS = os.path.join(ROOT, "import", "Prints")
DEFAULT_OUT = os.path.join(ROOT, "library", "wd_m_profile.tsv")

HEADER = "attribute\tvalue\tsheets_agreeing\tsheets_total\tsheets_present\tother_values"


def sheet_paths(prints_dir):
    paths = []
    for base, _, names in os.walk(prints_dir):
        for name in sorted(names):
            if name.lower().endswith(".dxf"):
                paths.append(os.path.join(base, name))
    paths.sort()
    return paths


def build_profile(prints_dir):
    """Return (tsv text, summary dict) for the WD_M setup across the sheets."""
    sheets = [H.parse_sheet(path) for path in sheet_paths(prints_dir)]
    per_sheet = []          # list of dict tag -> value
    conflicts = []
    total = 0
    for sheet in sheets:
        wdms = [insert for insert in sheet.inserts
                if insert["block"].upper() == "WD_M"]
        if not wdms:
            continue
        total += 1
        maps = [{tag: value for tag, value in insert["attributes"]} for insert in wdms]
        if any(other != maps[0] for other in maps[1:]):
            conflicts.append(sheet.name)
        per_sheet.append(maps[0])

    present = collections.Counter()
    values = collections.defaultdict(collections.Counter)
    for profile in per_sheet:
        for tag, value in profile.items():
            present[tag] += 1
            values[tag][value] += 1

    rows = [HEADER]
    for tag in sorted(values):
        counts = values[tag]
        value, agreeing = counts.most_common(1)[0]
        others = "; ".join(f"{other} ({count})"
                           for other, count in counts.most_common()[1:])
        rows.append("\t".join([
            tag,
            value,
            str(agreeing),
            str(total),
            str(present[tag]),
            others or "-",
        ]))
    summary = {
        "sheets": total,
        "attributes": len(values),
        "uniform": sum(1 for tag in values if len(values[tag]) == 1),
        "conflicts": conflicts,
    }
    return "\n".join(rows) + "\n", summary


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--prints", default=DEFAULT_PRINTS)
    parser.add_argument("--out", default=DEFAULT_OUT)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args(argv)

    text, summary = build_profile(args.prints)
    if args.check:
        if not os.path.exists(args.out):
            print(f"missing {args.out}")
            return 1
        with open(args.out, "r", encoding="ascii") as handle:
            if handle.read() != text:
                print(f"{args.out} differs from the extraction")
                return 1
        print(f"wd_m_profile.tsv is in sync: {summary['attributes']} attributes, "
              f"{summary['sheets']} sheets")
        return 0

    os.makedirs(os.path.dirname(args.out), exist_ok=True)
    with open(args.out, "w", encoding="ascii", newline="\n") as handle:
        handle.write(text)
    print(f"wrote {args.out}: {summary['attributes']} attributes from "
          f"{summary['sheets']} sheets; {summary['uniform']} agree on every sheet; "
          f"per-sheet disagreements: {summary['conflicts'] or 'none'}")
    return 0


if __name__ == "__main__":
    sys.exit(main())

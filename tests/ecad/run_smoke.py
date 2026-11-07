#!/usr/bin/env python3
"""Headless end-to-end proof of the ECAD symbol library.

What it does
------------
1. Makes a blank DXF sheet.
2. Runs ``smoke_batch.txt`` through the already-built binary with
   ``--each <batch> sheet.dxf``: ELECNEW, ELECLIB, ELECINSERT, ELECPLACE,
   ELECSETATTR, ELECLIST, ELECBOM, ELECWIRE, ELECJUNCTIONS, ELECNUMBER, QSAVE.
3. Re-reads the **saved DXF** with ``tools/dxfio.py`` and checks it, instead of
   trusting what the app printed.  The printed summary is checked too, where it
   is the only observable (the loader's own count).
4. Re-opens the saved file through the app (``--export``) to prove the drawing
   survives a load/save round trip: insert points, tags and block origins.
5. Runs ``makesymbol_batch.txt`` against a COPY of the library, then
   ``reload_batch.txt`` to prove an authored symbol is usable, and
   ``fabricated_batch.txt`` to record what the loader's attribute-fabrication
   branch actually produces.

Every check prints PASS/FAIL; the process exits non-zero if any fails.  Raw
stdout of each run is kept under ``tests/ecad/_work/logs/`` so the report can
quote it verbatim.

Usage
-----
    py -3.13 tests/ecad/run_smoke.py
    py -3.13 tests/ecad/run_smoke.py --baseline C:\\path\\to\\OpenCADStudio.exe

The baseline defaults to ``%TEMP%\\ocs_baseline.exe`` (a copy of the built
release binary, taken before the source tree is rebuilt).  This script never
builds anything.
"""

from __future__ import annotations

import argparse
import os
import re
import shutil
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))
TOOLS = os.path.join(ROOT, "tools")
sys.path.insert(0, TOOLS)
import dxfio  # noqa: E402

# The app's output carries Unicode markers (the info glyph, the middle dot); a
# Windows console defaulting to cp1252 would raise on them.
for _stream in (sys.stdout, sys.stderr):
    if hasattr(_stream, "reconfigure"):
        _stream.reconfigure(encoding="utf-8", errors="replace")

WORK = os.path.join(HERE, "_work")
LOGS = os.path.join(WORK, "logs")
# library/ holds the harvested client library (from import/Prints);
# library/starter/ holds the licence-clean generated starter set.
CLIENT = os.path.join(ROOT, "library")
STARTER = os.path.join(CLIENT, "starter")
PRINTS_DXF = os.path.join(ROOT, "import", "Prints", "Prints", "DXF")

CHECKS: list[tuple[str, bool, str]] = []


def check(label, ok, detail=""):
    CHECKS.append((label, bool(ok), detail))
    mark = "PASS" if ok else "FAIL"
    print(f"[{mark}] {label}" + (f"  ({detail})" if detail else ""))
    return ok


def section(title):
    print()
    print(f"=== {title}")
    print()


# --------------------------------------------------------------------------- #
# process plumbing
# --------------------------------------------------------------------------- #


def run(baseline, args, label):
    """Run the binary, print and log its output, return (exit, stdout+stderr)."""
    print("$ " + " ".join(args))
    proc = subprocess.run(args, capture_output=True, text=True, cwd=ROOT,
                          encoding="utf-8", errors="replace")
    text = proc.stdout + proc.stderr
    os.makedirs(LOGS, exist_ok=True)
    with open(os.path.join(LOGS, f"{label}.log"), "w", encoding="utf-8") as handle:
        handle.write("$ " + " ".join(args) + "\n")
        handle.write(f"[exit code: {proc.returncode}]\n")
        handle.write(text)
    for line in text.splitlines():
        print("  | " + line)
    return proc.returncode, text


def write_batch(template, target, substitutions):
    with open(template, "r", encoding="utf-8") as handle:
        text = handle.read()
    for key, value in substitutions.items():
        text = text.replace(key, value)
    if "@" in text:
        leftover = sorted(set(re.findall(r"@[A-Z_]+@", text)))
        raise SystemExit(f"{template}: unsubstituted placeholders {leftover}")
    with open(target, "w", encoding="ascii", newline="\n") as handle:
        handle.write(text)
    return target


def blank_drawing(path):
    dxfio.DxfDocument().save(path)
    return path


# --------------------------------------------------------------------------- #
# expectations derived from the library the app is about to load
# --------------------------------------------------------------------------- #


def library_dir():
    """Which library folder this run is about: the starter set unless told otherwise."""
    return os.environ.get("ECAD_LIBRARY_DIR", STARTER)


def read_manifest(folder):
    path = os.path.join(folder, "library.tsv")
    with open(path, "r", encoding="ascii") as handle:
        lines = [line for line in handle.read().splitlines() if line.strip()]
    rows = []
    for line in lines[1:]:
        columns = line.split("\t")
        rows.append(
            dict(
                symbol=columns[0].strip(),
                cls=columns[1].strip(),
                attrs=[tag for tag in columns[2].strip().split("|") if tag],
                scale=float(columns[3]) if len(columns) > 3 and columns[3].strip() else 1.0,
            )
        )
    return rows


def kind_counts(drawing):
    counts = {}
    for entity in drawing.entities:
        counts[entity.kind] = counts.get(entity.kind, 0) + 1
    return counts


def palette_segments(drawing):
    """Reimplementation of elec_symbol_segments' segment count for one symbol.

    Mirrors the loader exactly, including its ``MAX_SEGMENTS = 400`` cap: an
    entity is skipped once 400 segments have been produced, and one entity's
    segments are added whole. FAN hits the cap (428 -> 410 segments).
    """
    total = 0
    for entity in drawing.entities:
        if total >= 400:
            continue
        if entity.kind == "LINE":
            total += 1
        elif entity.kind == "CIRCLE":
            total += 16          # steps = 16, 17 sampled points
        elif entity.kind == "ARC":
            total += 8           # steps = 8
        elif entity.kind == "LWPOLYLINE":
            vertices = sum(1 for code, _ in entity.pairs if code == 10)
            total += max(0, vertices - 1)
    return total


def geometry_span(drawing):
    """Span the loader measures: max(width, height) of LINE/CIRCLE/ARC/LWPOLYLINE."""
    min_x = min_y = float("inf")
    max_x = max_y = float("-inf")

    def take(x, y):
        nonlocal min_x, min_y, max_x, max_y
        min_x = min(min_x, x)
        min_y = min(min_y, y)
        max_x = max(max_x, x)
        max_y = max(max_y, y)

    for entity in drawing.entities:
        if entity.kind == "LINE":
            take(entity.f(10), entity.f(20))
            take(entity.f(11), entity.f(21))
        elif entity.kind == "CIRCLE":
            cx, cy, r = entity.f(10), entity.f(20), entity.f(40)
            take(cx - r, cy - r)
            take(cx + r, cy + r)
        elif entity.kind == "LWPOLYLINE":
            xs = [v for code, v in entity.pairs if code == 10]
            ys = [v for code, v in entity.pairs if code == 20]
            for x, y in zip(xs, ys):
                take(float(x), float(y))
    if min_x == float("inf"):
        return 0.0
    return max(max_x - min_x, max_y - min_y)


def expected_symbols(folder):
    rows = read_manifest(folder)
    table = {}
    for row in rows:
        drawing = dxfio.load(os.path.join(folder, row["symbol"] + ".dxf"))
        table[row["symbol"]] = dict(
            row=row,
            drawing=drawing,
            counts=kind_counts(drawing),
            segments=palette_segments(drawing),
            span=geometry_span(drawing),
        )
    return table


# --------------------------------------------------------------------------- #
# DXF accessors
# --------------------------------------------------------------------------- #

TAG_ATTRIBUTES = {"TAG", "TAG1", "TAG2", "TAGSTR", "WORKID"}


def attribute_map(insert):
    return {a.s(2).strip().upper(): a for a in insert.attributes}


def insert_by_tag(drawing, tag):
    for entity in drawing.entities:
        if entity.kind != "INSERT":
            continue
        for attribute in entity.attributes:
            if attribute.s(2).strip().upper() in TAG_ATTRIBUTES:
                if attribute.s(1).strip().upper() == tag.upper():
                    return entity
    return None


def insert_of_block(drawing, block):
    found = [e for e in drawing.entities if e.kind == "INSERT" and
             e.s(2).strip().upper() == block.upper()]
    return found[0] if len(found) == 1 else None


def on_layer(drawing, layer):
    return [e for e in drawing.entities if e.s(8).strip().upper() == layer.upper()]


def insert_scale(insert):
    """INSERT scale factors; DXF omits 41/42/43 when they are 1.0."""
    return tuple(insert.f(code, 1.0) for code in (41, 42, 43))



# --------------------------------------------------------------------------- #
# the runs
# --------------------------------------------------------------------------- #


def appdata_cache_path():
    base = os.environ.get("APPDATA")
    if not base:
        return None
    return os.path.join(base, "AryaCAD", "elec_library.txt")


def prepare_work(library):
    if os.path.isdir(WORK):
        shutil.rmtree(WORK)
    os.makedirs(LOGS, exist_ok=True)
    # A writable copy for ELECMAKESYMBOL (it appends to library.tsv and writes a DXF).
    shutil.copytree(library, os.path.join(WORK, "lib_makesymbol"))
    # A one-symbol library whose DXF carries no ATTDEFs, to exercise the
    # loader's fabrication branch.
    fab = os.path.join(WORK, "fablib")
    os.makedirs(fab)
    source = dxfio.load(os.path.join(library, "CONTACT_NO.dxf"))
    geometry = [e for e in source.entities if e.kind != "ATTDEF"]
    doc = dxfio.DxfDocument()
    for entity in geometry:
        if entity.kind == "LINE":
            doc.entities.append(dxfio.DxfDocument._line(
                entity.f(10), entity.f(20), entity.f(11), entity.f(21), entity.s(8, "0")))
        elif entity.kind == "CIRCLE":
            doc.entities.append(dxfio.DxfDocument._circle(
                entity.f(10), entity.f(20), entity.f(40), entity.s(8, "0")))
        elif entity.kind == "LWPOLYLINE":
            points = [(float(x), float(y)) for (code, x), (_, y) in
                      zip([p for p in entity.pairs if p[0] == 10],
                          [p for p in entity.pairs if p[0] == 20])]
            doc.entities.append(dxfio.DxfDocument._lwpolyline(points))
        elif entity.kind == "TEXT":
            doc.entities.append(dxfio.DxfDocument._text(
                entity.s(1), entity.f(10), entity.f(20), entity.f(40), entity.s(8, "0")))
    doc.save(os.path.join(fab, "NOATTR_SYM.dxf"))
    with open(os.path.join(fab, "library.tsv"), "w", encoding="ascii", newline="\n") as handle:
        handle.write("symbol\tclass\tattributes\tscale\n")
        handle.write("NOATTR_SYM\tContactor\tTAG1|CAT\t1\n")


def write_lib(folder, files, manifest_text):
    """Materialise a small library: {file name: DXF text} plus its manifest."""
    os.makedirs(folder, exist_ok=False)
    for name, text in files.items():
        with open(os.path.join(folder, name), "w", encoding="ascii", newline="\n") as handle:
            handle.write(text)
    with open(os.path.join(folder, "library.tsv"), "w", encoding="ascii", newline="\n") as handle:
        handle.write(manifest_text)
    return folder



def run_smoke(baseline, symbols):
    section("1. main drafting path (smoke_batch.txt) on the generated starter library")
    sheet = blank_drawing(os.path.join(WORK, "sheet.dxf"))
    bom = os.path.join(WORK, "bom.csv")
    batch = write_batch(
        os.path.join(HERE, "smoke_batch.txt"),
        os.path.join(WORK, "smoke_batch.txt"),
        {"@LIB@": STARTER, "@BOM@": bom},
    )
    code, text = run(baseline, [baseline, "--each", batch, sheet], "smoke")

    check("smoke batch exits 0", code == 0, f"exit {code}")

    match = re.search(
        r"ELECLIB: loaded (\d+) symbol\(s\).*?(\d+) failed, (\d+) in the catalogue, (\d+) preview segment",
        text,
    )
    if not match:
        check("ELECLIB summary line present", False, "no 'ELECLIB: loaded ...' line")
        return
    loaded, failed, catalogue, preview = (int(value) for value in match.groups())
    expected = len(symbols)
    check("ELECLIB loaded every symbol", loaded == expected, f"loaded {loaded} of {expected}")
    check("ELECLIB reported no failures", failed == 0, f"{failed} failed")
    check("ELECLIB read every manifest row", catalogue == expected,
          f"catalogue {catalogue} of {expected}")
    total_segments = sum(entry["segments"] for entry in symbols.values())
    check("ELECLIB measured preview geometry for every symbol",
          preview == total_segments, f"{preview} of {total_segments} palette segments")

    drawing = dxfio.load(sheet)

    library_blocks = [e for e in symbols]
    block_names = {name for name in drawing.blocks if not name.startswith("*")}
    missing = [name for name in library_blocks if name.upper() not in block_names]
    check("every library symbol became a block definition",
          not missing, f"{len(block_names)} blocks, missing: {missing or 'none'}")

    wrong = []
    for name, entry in symbols.items():
        block = drawing.blocks.get(name.upper())
        if block is None:
            continue
        actual = {}
        for entity in block.entities:
            actual[entity.kind] = actual.get(entity.kind, 0) + 1
        expected_counts = entry["counts"]
        if actual != expected_counts:
            wrong.append(f"{name}: {actual} != {expected_counts}")
        expected_attdefs = len(entry["row"]["attrs"])
        if actual.get("ATTDEF", 0) != expected_attdefs:
            wrong.append(f"{name}: {actual.get('ATTDEF', 0)} ATTDEF != {expected_attdefs}")
    check("block geometry and attribute definitions match the source DXFs",
          not wrong, f"{len(wrong)} mismatches: {wrong[:3]}")

    inserts = [e for e in drawing.entities if e.kind == "INSERT"]
    check("every ELECINSERT/ELECPLACE produced an INSERT", len(inserts) == 6,
          f"{len(inserts)} inserts: {[e.s(2) for e in inserts]}")

    expected_tags = {
        "CONTACT_NO": "-K1",
        "CONTACT_NC": "-K2",
        "MOTOR": "-M1",
        "TERMINAL": "X1:1",
        "EARTH": "-E1",
        "FUSE": "-F1",
    }
    problems = []
    for block, tag in expected_tags.items():
        found = insert_by_tag(drawing, tag)
        if found is None:
            problems.append(f"no insert tagged {tag}")
        elif found.s(2).strip().upper() != block:
            problems.append(f"{tag} is {found.s(2)}, expected {block}")
    check("tags follow IEC prefixes and the terminal keeps its strip tag",
          not problems, "; ".join(problems) or "all six found")

    contactor = insert_by_tag(drawing, "-K1")
    if contactor is not None:
        attrs = attribute_map(contactor)
        wanted = {
            "CAT": "3RT2016-1AB01",
            "MFG": "Siemens",
            "DESC1": "Contactor 9A",
            "LOC": "+MCC1",
            "RATING": "9A",
        }
        actual = {tag: attrs[tag].s(1) for tag in wanted if tag in attrs}
        check("ELECSETATTR wrote the requested attribute values",
              actual == wanted, f"{actual}")

        # The added attribute is sized by the height ELECLIB measured for the
        # symbol (span / 8), not by a fixed constant.
        added_height = attrs["RATING"].f(40) if "RATING" in attrs else 0.0
        expected_height = round(symbols["CONTACT_NO"]["span"] / 8.0, 4)
        check("ELECSETATTR sizes a new attribute from the measured label height",
              abs(added_height - expected_height) < 1e-6,
              f"{added_height} vs span/8 = {expected_height}")

    earth = insert_by_tag(drawing, "-E1")
    if earth is not None:
        attrs = attribute_map(earth)
        height = attrs["RATING"].f(40) if "RATING" in attrs else 0.0
        expected_height = round(symbols["EARTH"]["span"] / 8.0, 4)
        check("label height is measured per symbol, not globally",
              abs(height - expected_height) < 1e-6,
              f"EARTH {height} vs span/8 = {expected_height}")

    terminal = insert_by_tag(drawing, "X1:1")
    if terminal is not None:
        attrs = attribute_map(terminal)
        check("ELECSETATTR retagged a terminal to strip:index",
              attrs.get("TAG1") is not None and attrs["TAG1"].s(1) == "X1:1",
              "TAG1 = X1:1")
        check("terminal keeps CAT through renumbering",
              attrs.get("CAT") is not None and attrs["CAT"].s(1) == "UK2.5B",
              f"CAT = {attrs['CAT'].s(1) if 'CAT' in attrs else '-'}")

    layers = {name.upper() for name in drawing.layers}
    wanted_layers = {"WIRES", "JUNCTIONS", "WIRENO", "XREF", "ELEC-SYMBOLS"}
    check("ELECNEW created the electrical layers",
          wanted_layers <= layers, f"missing {sorted(wanted_layers - layers) or 'none'}")

    wires = on_layer(drawing, "WIRES")
    segments = sorted(
        tuple(round(value, 3) for value in (e.f(10), e.f(20), e.f(11), e.f(21)))
        for e in wires if e.kind == "LINE"
    )
    expected_segments = sorted([
        (0.0, 0.0, 40.0, 0.0),
        (40.0, 0.0, 40.0, 20.0),
        (20.0, 0.0, 20.0, 10.0),
    ])
    check("ELECWIRE drew three conductors on the WIRES layer",
          segments == expected_segments, f"{segments}")

    junctions = on_layer(drawing, "JUNCTIONS")
    dots = [(e.f(10), e.f(20), e.f(40)) for e in junctions if e.kind == "CIRCLE"]
    check("ELECJUNCTIONS put one dot where the branch meets the trunk",
          len(dots) == 1 and abs(dots[0][0] - 20.0) < 1e-6
          and abs(dots[0][1]) < 1e-6 and abs(dots[0][2] - 1.0) < 1e-6,
          f"{dots}")

    listing = [line for line in text.splitlines()
               if line.startswith("CLASS ")
               or (len(line) > 40 and line.split()[0] in (
                   "Contactor", "Motor", "Terminal", "Earth", "Fuse", "Button",
                   "Selector", "Relay", "Lamp", "Sensor", "Drive", "Transformer",
                   "Power Supply", "Circuit Breaker", "Cable"))]
    check("ELECLIST printed the placed components",
          any("X1:1" in line for line in listing) and any("-K1" in line for line in listing),
          f"{len(listing)} listing rows")

    if os.path.exists(bom):
        with open(bom, "r", encoding="utf-8") as handle:
            rows = [line for line in handle.read().splitlines() if line.strip()]
        header = rows[0]
        check("ELECBOM wrote the CSV header",
              header == "Class name,Part number,Manufacturer,Description,Qty,Where used",
              header)
        check("ELECBOM wrote one row per (class, part)", len(rows) - 1 == 5,
              f"{len(rows) - 1} rows: {rows[1:]}")
        joined = "\n".join(rows)
        check("BOM carries the part number and location",
              "3RT2016-1AB01" in joined and "+MCC1:-K1" in joined, "part + where-used present")
    else:
        check("ELECBOM wrote a CSV file", False, "missing " + bom)

    check("ELECNUMBER reported the components it numbered",
          "ELECNUMBER:" in text and "-K1" in text, "")

    # ELECINSERT offsets the attributes by the block's own ATTDEF positions;
    # ELECPLACE re-stacks them at the insertion point instead. Both are visible
    # in the same drawing, so this measures the difference rather than assuming it.
    def attdef_offsets(block_name):
        block = drawing.blocks.get(block_name.upper())
        insert = insert_of_block(drawing, block_name)
        if block is None or insert is None:
            return {}
        definitions = {e.s(2).strip().upper(): e.f(20) for e in block.entities if e.kind == "ATTDEF"}
        out = {}
        for tag, attribute in attribute_map(insert).items():
            # Attributes the library template adds (ELECSETATTR) have no ATTDEF.
            if tag not in definitions:
                continue
            out[tag] = (round(attribute.f(20) - insert.f(20), 4),
                        round(definitions[tag], 4))
        return out

    contactor_offsets = attdef_offsets("CONTACT_NO")
    honoured = [tag for tag, (actual, defined) in contactor_offsets.items()
                if abs(actual - defined) < 1e-6]
    check("ELECINSERT honours the symbol's own ATTDEF positions",
          len(honoured) == len(contactor_offsets) and contactor_offsets,
          f"{len(honoured)} of {len(contactor_offsets)} attributes at their defined offset")

    fuse_offsets = attdef_offsets("FUSE")
    deltas = {tag: round(actual - defined, 4)
              for tag, (actual, defined) in fuse_offsets.items()}
    worst = max((abs(delta) for delta in deltas.values()), default=0.0)
    check("ELECPLACE does NOT honour the symbol's ATTDEF positions (defect observed)",
          worst > 1e-6,
          f"attribute y offsets differ from the block's definitions by up to {worst} units")

    # ---- round trip through the app's own reader/writer ------------------ #
    section("2. load/save round trip of the saved drawing")
    roundtrip = os.path.join(WORK, "sheet_roundtrip.dxf")
    code, _ = run(baseline, [baseline, "--export", sheet, roundtrip], "roundtrip")
    check("round-trip export exits 0", code == 0, f"exit {code}")
    if os.path.exists(roundtrip):
        again = dxfio.load(roundtrip)
        problems = []
        for block, tag in expected_tags.items():
            before = insert_by_tag(drawing, tag)
            after = insert_by_tag(again, tag)
            if before is None or after is None:
                problems.append(f"{tag} missing after round trip")
                continue
            if (round(before.f(10), 6), round(before.f(20), 6)) != (
                    round(after.f(10), 6), round(after.f(20), 6)):
                problems.append(f"{tag} moved {before.point()} -> {after.point()}")
        nonzero = [name for name, block in again.blocks.items()
                   if not name.startswith("*") and any(abs(v) > 1e-9 for v in block.base)]
        check("insert points survive the round trip", not problems, "; ".join(problems) or "stable")
        check("block origin survives the round trip", not nonzero,
              f"non-zero bases: {nonzero or 'none'}")
        attrs_before = attribute_map(insert_by_tag(drawing, "-K1"))
        attrs_after = attribute_map(insert_by_tag(again, "-K1"))
        check("attribute values survive the round trip",
              {k: v.s(1) for k, v in attrs_before.items()} ==
              {k: v.s(1) for k, v in attrs_after.items()}, "identical")


def run_makesymbol(baseline):
    section("3. ELECMAKESYMBOL: author a symbol from drawing geometry")
    lib_copy = os.path.join(WORK, "lib_makesymbol")
    sheet = blank_drawing(os.path.join(WORK, "makesymbol.dxf"))
    batch = write_batch(
        os.path.join(HERE, "makesymbol_batch.txt"),
        os.path.join(WORK, "makesymbol_batch.txt"),
        {"@LIB@": lib_copy},
    )
    code, text = run(baseline, [baseline, "--each", batch, sheet], "makesymbol")
    check("makesymbol batch exits 0", code == 0, f"exit {code}")
    check("ELECMAKESYMBOL reported writing a symbol",
          "ELECMAKESYMBOL:" in text and "written to" in text, "")

    symbol_path = os.path.join(lib_copy, "SMOKE_SYM.dxf")
    check("ELECMAKESYMBOL wrote library/SMOKE_SYM.dxf", os.path.exists(symbol_path), symbol_path)
    if os.path.exists(symbol_path):
        symbol = dxfio.load(symbol_path)
        counts = kind_counts(symbol)
        check("authored symbol keeps the selected geometry",
              counts.get("LINE") == 4, f"{counts}")
        check("authored symbol gets TAG1/CAT/MFG/DESC1 definitions",
              counts.get("ATTDEF") == 4, f"{counts.get('ATTDEF', 0)} ATTDEFs")
    with open(os.path.join(lib_copy, "library.tsv"), "r", encoding="ascii") as handle:
        manifest_text = handle.read()
    check("ELECMAKESYMBOL appended a manifest row",
          any(line.startswith("SMOKE_SYM\t") for line in manifest_text.splitlines()),
          [line for line in manifest_text.splitlines() if line.startswith("SMOKE_SYM")])

    drawing = dxfio.load(sheet)
    block = drawing.blocks.get("SMOKE_SYM")
    check("authored symbol exists as a block in the drawing",
          block is not None and len([e for e in block.entities if e.kind == "LINE"]) == 4,
          f"{len(block.entities) if block else 0} entities")

    section("4. reload the library and place the authored symbol")
    sheet2 = blank_drawing(os.path.join(WORK, "reload.dxf"))
    batch2 = write_batch(
        os.path.join(HERE, "reload_batch.txt"),
        os.path.join(WORK, "reload_batch.txt"),
        {"@LIB@": lib_copy},
    )
    code, text = run(baseline, [baseline, "--each", batch2, sheet2], "reload")
    check("reload batch exits 0", code == 0, f"exit {code}")
    match = re.search(r"ELECLIB: loaded (\d+) symbol", text)
    # The copied starter library count grows whenever symbols are added to
    # library\starter; ELECMAKESYMBOL has already written its own DXF into the
    # copy, so the expected count is simply the folder's DXF count.
    expected_reloaded = len(
        [name for name in os.listdir(lib_copy) if name.lower().endswith(".dxf")]
    )
    check("the rewritten library reloads with the new symbol",
          match is not None and int(match.group(1)) == expected_reloaded,
          f"loaded {match.group(1) if match else '?'} of {expected_reloaded}")
    drawing2 = dxfio.load(sheet2)
    placed = insert_of_block(drawing2, "SMOKE_SYM")
    check("the authored symbol can be placed with ELECINSERT",
          placed is not None, "INSERT SMOKE_SYM")
    if placed is not None:
        attrs = attribute_map(placed)
        check("the placed authored symbol is tagged",
              "TAG1" in attrs and attrs["TAG1"].s(1).startswith("-S"),
              f"TAG1 = {attrs['TAG1'].s(1) if 'TAG1' in attrs else '-'}")


def run_edge_cases(baseline):
    """Loader contract edge cases: the scale column, block-only symbols, failure modes."""
    section("6. the manifest scale column, and what ELECINSERT does with it")

    fuse_text = open(os.path.join(STARTER, "FUSE.dxf"), "r", encoding="ascii").read()
    attributes = "TAG1|CAT|MFG|DESC1|LOC|TERM01|TERM02"

    # 5 columns, the layout STATUS.md §6 says the historical library had:
    # symbol, class, attributes, sheets, scale.  The loader parses column 4 as
    # `scale`, so `sheets` is read as the scale and the real scale is ignored.
    lib5 = write_lib(
        os.path.join(WORK, "edge_scale5"),
        {"FUSE.dxf": fuse_text},
        "symbol\tclass\tattributes\tsheets\tscale\n"
        f"FUSE\tFuse\t{attributes}\t12\t2\n",
    )
    sheet = blank_drawing(os.path.join(WORK, "scale5.dxf"))
    batch = write_batch(os.path.join(HERE, "scale_batch.txt"),
                        os.path.join(WORK, "scale_batch5.txt"), {"@LIB@": lib5})
    code, _ = run(baseline, [baseline, "--each", batch, sheet], "scale5")
    check("5-column manifest batch exits 0", code == 0, f"exit {code}")
    drawing = dxfio.load(sheet)
    scales = sorted(
        (round(e.f(10), 3), round(insert_scale(e)[0], 3))
        for e in drawing.entities if e.kind == "INSERT"
    )
    print(f"         (x, insert scale) from a 5-column row ...sheets=12, scale=2: {scales}")
    by_x = dict(scales)
    check("ELECINSERT inserts at scale 1.0 (it ignores the scale column)",
          abs(by_x.get(0.0, 0.0) - 1.0) < 1e-6, f"scale {by_x.get(0.0)}")
    check("ELECPLACE reads column 4 (the historical `sheets` column) as the scale",
          abs(by_x.get(60.0, 0.0) - 12.0) < 1e-6,
          f"scale {by_x.get(60.0)} for a row whose real scale column says 2")

    # 4 columns: symbol, class, attributes, scale - the layout the loader reads
    # and the layout ELECMAKESYMBOL appends.
    lib4 = write_lib(
        os.path.join(WORK, "edge_scale4"),
        {"FUSE.dxf": fuse_text},
        "symbol\tclass\tattributes\tscale\n"
        f"FUSE\tFuse\t{attributes}\t2\n",
    )
    sheet = blank_drawing(os.path.join(WORK, "scale4.dxf"))
    batch = write_batch(os.path.join(HERE, "scale_batch.txt"),
                        os.path.join(WORK, "scale_batch4.txt"), {"@LIB@": lib4})
    code, _ = run(baseline, [baseline, "--each", batch, sheet], "scale4")
    check("4-column manifest batch exits 0", code == 0, f"exit {code}")
    drawing = dxfio.load(sheet)
    by_x = {round(e.f(10), 3): round(insert_scale(e)[0], 3)
            for e in drawing.entities if e.kind == "INSERT"}
    check("with the 4-column manifest ELECPLACE inserts at the declared scale",
          abs(by_x.get(60.0, 0.0) - 2.0) < 1e-6, f"scale {by_x.get(60.0)}")
    check("ELECINSERT still inserts at scale 1.0 regardless of the manifest",
          abs(by_x.get(0.0, 0.0) - 1.0) < 1e-6, f"scale {by_x.get(0.0)}")

    section("7. block-only symbol layout (geometry inside a BLOCK definition)")
    doc = dxfio.DxfDocument()
    inner = [
        dxfio.DxfDocument._line(-3, -2, 3, -2),
        dxfio.DxfDocument._line(3, -2, 3, 2),
        dxfio.DxfDocument._line(3, 2, -3, 2),
        dxfio.DxfDocument._line(-3, 2, -3, -2),
        dxfio.DxfDocument._line(-5, 0, -3, 0),
        dxfio.DxfDocument._line(3, 0, 5, 0),
        dxfio.DxfDocument.attdef("TAG1", "Tag", "", height=1.25, x=-3, y=-3.0),
        dxfio.DxfDocument.attdef("CAT", "Part number", "", height=1.25, x=-3, y=-4.6),
    ]
    doc.define_block("BOXSYM", entities=inner)
    lib_box = write_lib(
        os.path.join(WORK, "edge_blockonly"),
        {"BOXSYM.dxf": doc.render()},
        "symbol\tclass\tattributes\tscale\nBOXSYM\tContactor\tTAG1|CAT\t1\n",
    )
    sheet = blank_drawing(os.path.join(WORK, "blockonly.dxf"))
    batch = write_batch(os.path.join(HERE, "blockonly_batch.txt"),
                        os.path.join(WORK, "blockonly_batch.txt"), {"@LIB@": lib_box})
    code, text = run(baseline, [baseline, "--each", batch, sheet], "blockonly")
    check("block-only batch exits 0", code == 0, f"exit {code}")
    loaded = re.search(r"ELECLIB: loaded (\d+) symbol", text)
    preview = re.search(r"(\d+) preview segment", text)
    check("ELECLIB loads a symbol whose geometry is inside a block definition",
          loaded is not None and int(loaded.group(1)) == 1,
          f"loaded {loaded.group(1) if loaded else '?'} of 1")
    check("the palette finds that block's segments",
          preview is not None and int(preview.group(1)) == 6,
          f"{preview.group(1) if preview else '?'} segments, expected 6")
    drawing = dxfio.load(sheet)
    block = drawing.blocks.get("BOXSYM")
    counts = kind_counts(block) if block else {}
    check("the block keeping its geometry is defined in the drawing",
          counts.get("LINE") == 6 and counts.get("ATTDEF") == 2, f"{counts}")
    check("the block-only symbol places", insert_of_block(drawing, "BOXSYM") is not None, "")

    section("8. mixed symbol file: model space AND a block definition")
    doc = dxfio.DxfDocument()
    doc.add_line(-5, 0, 5, 0, "0")            # model space
    doc.define_block("MIXED", entities=[
        dxfio.DxfDocument._line(0, 0, 10, 0),
        dxfio.DxfDocument._line(0, 5, 10, 5),
    ])
    lib_mixed = write_lib(
        os.path.join(WORK, "edge_mixed"),
        {"MIXED.dxf": doc.render()},
        "symbol\tclass\tattributes\tscale\nMIXED\tContactor\tTAG1\t1\n",
    )
    sheet = blank_drawing(os.path.join(WORK, "mixed.dxf"))
    batch = write_batch(os.path.join(HERE, "mixed_batch.txt"),
                        os.path.join(WORK, "mixed_batch.txt"), {"@LIB@": lib_mixed})
    code, text = run(baseline, [baseline, "--each", batch, sheet], "mixed")
    check("mixed batch exits 0", code == 0, f"exit {code}")
    drawing = dxfio.load(sheet)
    block = drawing.blocks.get("MIXED")
    lines = len([e for e in block.entities if e.kind == "LINE"]) if block else 0
    check("ELECLIB merges model-space and block-definition geometry into one block",
          lines == 3, f"{lines} LINE entities (1 model space + 2 in the block)")

    section("9. failure modes: empty DXF, unknown symbol")
    empty = dxfio.DxfDocument()
    lib_fail = write_lib(
        os.path.join(WORK, "edge_failure"),
        {"EMPTY.dxf": empty.render()},
        "symbol\tclass\tattributes\tscale\nEMPTY\tFuse\tTAG1\t1\n",
    )
    sheet = blank_drawing(os.path.join(WORK, "failure.dxf"))
    batch = write_batch(os.path.join(HERE, "failure_batch.txt"),
                        os.path.join(WORK, "failure_batch.txt"), {"@LIB@": lib_fail})
    code, text = run(baseline, [baseline, "--each", batch, sheet], "failure")
    check("failure batch still exits 0", code == 0, f"exit {code}")
    check("ELECLIB reports an empty DXF instead of counting it",
          "EMPTY: no geometry" in text, "reported as a failed symbol")
    check("ELECINSERT refuses an unknown block with a printed reason",
          "ELECINSERT: no block named" in text, "")
    drawing = dxfio.load(sheet)
    dangling = insert_of_block(drawing, "NOSUCH")
    check("ELECPLACE writes an INSERT for a block that does not exist (defect observed)",
          dangling is not None and "NOSUCH" not in drawing.blocks,
          "dangling INSERT in the drawing with no block definition")
    # Does a drawing carrying that dangling insert survive a load?
    roundtrip = os.path.join(WORK, "failure_roundtrip.dxf")
    code, text = run(baseline, [baseline, "--export", sheet, roundtrip], "failure_roundtrip")
    check("a drawing with a dangling INSERT still loads and saves",
          code == 0 and os.path.exists(roundtrip), f"exit {code}")


def run_fabricated(baseline):
    section("10. what the loader fabricates when a symbol has no ATTDEFs")
    fab = os.path.join(WORK, "fablib")
    sheet = blank_drawing(os.path.join(WORK, "fabricated.dxf"))
    batch = write_batch(
        os.path.join(HERE, "fabricated_batch.txt"),
        os.path.join(WORK, "fabricated_batch.txt"),
        {"@LIB@": fab},
    )
    code, text = run(baseline, [baseline, "--each", batch, sheet], "fabricated")
    check("fabricated batch exits 0", code == 0, f"exit {code}")
    drawing = dxfio.load(sheet)
    block = drawing.blocks.get("NOATTR_SYM")
    if block is None:
        check("fabricated symbol became a block", False, "NOATTR_SYM missing")
        return
    attdefs = [e for e in block.entities if e.kind == "ATTDEF"]
    heights = [round(e.f(40), 3) for e in attdefs]
    ys = [round(e.f(20), 3) for e in attdefs]
    check("the loader fabricated one ATTDEF per manifest tag", len(attdefs) == 2, f"{len(attdefs)}")
    print(f"         fabricated ATTDEF heights={heights} ys={ys} "
          f"(symbol span is 10, so ELECLIB's own label height would be 1.25)")
    check("fabricated definitions use a FIXED height of 2.0",
          heights == [2.0, 2.0], f"heights {heights}")
    check("fabricated definitions start at y=4.0 and step down by 2.5",
          ys == [4.0, 1.5], f"ys {ys}")


# --------------------------------------------------------------------------- #
# the harvested client library, the rebuilt binary, and the real sheets
# --------------------------------------------------------------------------- #


def default_prefix(cls):
    """Reimplementation of elec::default_prefix, so the tag checks test the rules."""
    key = cls.strip().lower()
    if key.startswith(("contactor", "relay", "timer", "overload")):
        letter = "K"
    elif key.startswith("motor"):
        letter = "M"
    elif key.startswith("terminal"):
        letter = "X"
    elif key.startswith("fuse"):
        letter = "F"
    elif key.startswith("circuit breaker") or key.startswith("breaker"):
        letter = "Q"
    elif key.startswith(("button", "switch", "selector")):
        letter = "S"
    elif key.startswith("sensor") or key.startswith("prox"):
        letter = "B"
    elif key.startswith("plc"):
        letter = "P"
    elif key.startswith("power supply"):
        letter = "G"
    elif key.startswith("drive") or key.startswith("soft starter"):
        letter = "U"
    elif key.startswith("transformer"):
        letter = "T"
    elif key.startswith("cable"):
        letter = "W"
    elif key.startswith("solenoid") or key.startswith("valve"):
        letter = "Y"
    elif key.startswith("lamp") or key.startswith("indicator"):
        letter = "H"
    else:
        letter = next((c.upper() for c in cls if c.isalpha()), "X")
    return "-" + letter


def expected_tag_map(rows, batch_path):
    """Tags ELECINSERT assigns, in the order the batch places them: one sequence per prefix."""
    by_symbol = {row["symbol"].upper(): row for row in rows}
    order = []
    with open(batch_path, "r", encoding="utf-8") as handle:
        for line in handle:
            line = line.strip()
            if line.upper().startswith("ELECINSERT "):
                order.append(line.split()[1])
    counters = {}
    tags = {}
    for symbol in order:
        row = by_symbol.get(symbol.upper())
        if row is None:
            continue
        prefix = default_prefix(row["cls"])
        counters[prefix] = counters.get(prefix, 0) + 1
        tags[symbol] = f"{prefix}{counters[prefix]}"
    return tags


def run_client_library(baseline, current, client_symbols):
    section("11. the harvested client library (library/, from import/Prints)")
    binary = current if os.path.exists(current) else baseline
    print(f"         using {binary}")
    sheet = blank_drawing(os.path.join(WORK, "client.dxf"))
    batch = write_batch(
        os.path.join(HERE, "client_batch.txt"),
        os.path.join(WORK, "client_batch.txt"),
        {"@LIB@": CLIENT},
    )
    code, text = run(binary, [binary, "--each", batch, sheet], "client")
    check("client library batch exits 0", code == 0, f"exit {code}")

    match = re.search(
        r"ELECLIB: loaded (\d+) symbol\(s\).*?(\d+) failed, (\d+) in the catalogue, (\d+) preview segment",
        text,
    )
    if not match:
        check("client ELECLIB summary present", False, "no summary line")
        return
    loaded, failed, catalogue, preview = (int(value) for value in match.groups())
    check("ELECLIB loads every harvested symbol",
          loaded == len(client_symbols), f"loaded {loaded} of {len(client_symbols)}")
    check("ELECLIB reports no failures on the harvested set", failed == 0, f"{failed} failed")
    check("ELECLIB reads the harvested manifest", catalogue == len(client_symbols),
          f"catalogue {catalogue}")
    total_segments = sum(entry["segments"] for entry in client_symbols.values())
    check("the palette gets geometry for the harvested symbols",
          preview == total_segments, f"{preview} of {total_segments} segments")

    drawing = dxfio.load(sheet)
    problems = []
    for name, entry in client_symbols.items():
        block = drawing.blocks.get(name.upper())
        if block is None:
            problems.append(f"{name}: no block")
            continue
        actual = {}
        for entity in block.entities:
            actual[entity.kind] = actual.get(entity.kind, 0) + 1
        expected = dict(entry["counts"])
        expected["ATTDEF"] = len(entry["row"]["attrs"])
        if actual != expected:
            problems.append(f"{name}: {actual} != {expected}")
    check("harvested geometry reaches the drawing unchanged (kinds + ATTDEFs)",
          not problems, f"{len(problems)} mismatches: {problems[:3]}")

    # Authenticity spot checks against the measured real geometry.
    def block_bbox(block_name):
        block = drawing.blocks.get(block_name.upper())
        if block is None:
            return None
        xs, ys = [], []
        for entity in block.entities:
            if entity.kind == "LINE":
                xs.extend([entity.f(10), entity.f(11)])
                ys.extend([entity.f(20), entity.f(21)])
            elif entity.kind == "CIRCLE":
                r = abs(entity.f(40))
                xs.extend([entity.f(10) - r, entity.f(10) + r])
                ys.extend([entity.f(20) - r, entity.f(20) + r])
        if not xs:
            return None
        return (min(xs), min(ys), max(xs), max(ys))

    contact = block_bbox("CONT-NO")
    check("CONT-NO keeps the real 0.875 x 0.25 unit geometry",
          contact is not None and abs((contact[2] - contact[0]) - 0.875) < 1e-3
          and abs((contact[3] - contact[1]) - 0.25) < 1e-3,
          f"bbox {contact}")
    fan = block_bbox("FAN")
    check("FAN's 0.125 insert scale is baked into the symbol",
          fan is not None and abs((fan[2] - fan[0]) - 1.25) < 1e-3,
          f"width {fan[2] - fan[0] if fan else '?'} (10 x 0.125)")
    prox = drawing.blocks.get("24VDCPRX")
    prox_kinds = kind_counts(prox) if prox else {}
    check("24VDCPRX carries its inlined Arrow-l SOLID",
          prox_kinds.get("SOLID") == 1 and prox_kinds.get("LINE") == 10,
          f"{prox_kinds}")

    # Tag prefixes: one numbering sequence per IEC letter.
    inserts = [e for e in drawing.entities if e.kind == "INSERT"]
    check("every harvested symbol placed", len(inserts) == len(client_symbols),
          f"{len(inserts)} inserts")
    tags = expected_tag_map(read_manifest(CLIENT),
                            os.path.join(HERE, "client_batch.txt"))
    problems = []
    for symbol, expected_tag in tags.items():
        found = insert_by_tag(drawing, expected_tag)
        if found is None:
            problems.append(f"{expected_tag} missing")
        elif found.s(2).strip().upper() != symbol.upper():
            problems.append(f"{expected_tag} is {found.s(2)} not {symbol}")
    check("class names give each family its own IEC tag prefix",
          not problems, "; ".join(problems[:4]) or "18 tags as expected")
    by_prefix = {}
    for row in read_manifest(CLIENT):
        by_prefix.setdefault(default_prefix(row["cls"]), set()).add(row["cls"])
    shared = {prefix: sorted(classes) for prefix, classes in by_prefix.items()
              if len(classes) > 1}
    check("no two unrelated classes share a tag prefix in the client set",
          not shared, f"shared: {shared}" if shared else f"{len(by_prefix)} distinct prefixes")

    # Attribute definitions in the harvested symbols are the project's text size.
    block = drawing.blocks.get("CONT-NO")
    attdef_heights = sorted({round(e.f(40), 4) for e in block.entities
                             if e.kind == "ATTDEF"}) if block else []
    check("harvested attribute definitions use the project text height 0.125",
          attdef_heights == [0.125], f"heights {attdef_heights}")

    # The delivered manifest must not contain the ACADE setup block.
    check("WD_M is not a library symbol",
          "WD_M" not in {row["symbol"].upper() for row in read_manifest(CLIENT)}
          and not os.path.exists(os.path.join(CLIENT, "WD_M.dxf")), "")

    section("12. the rebuilt binary on the starter library")
    if not os.path.exists(current):
        check("rebuilt binary is available", False, current)
        return
    sheet = blank_drawing(os.path.join(WORK, "current_lib.dxf"))
    batch = write_batch(
        os.path.join(HERE, "current_lib_batch.txt"),
        os.path.join(WORK, "current_lib_batch.txt"),
        {"@LIB@": STARTER},
    )
    code, text = run(current, [current, "--each", batch, sheet], "current_lib")
    check("rebuilt binary runs the library path", code == 0, f"exit {code}")
    check("rebuilt binary loads the starter library",
          f"loaded {len(read_manifest(STARTER))} symbol(s)" in text, "")
    drawing = dxfio.load(sheet)
    placed = insert_of_block(drawing, "CONTACT_NO")
    check("rebuilt binary places and tags a library symbol",
          placed is not None and attribute_map(placed).get("TAG1") is not None
          and attribute_map(placed)["TAG1"].s(1) == "-K1", "TAG1 = -K1")


def prepare_realsheet(name):
    """Copy one real sheet into the scratch folder so runs never touch the source."""
    source = os.path.join(PRINTS_DXF, name)
    target_dir = os.path.join(WORK, "real")
    os.makedirs(target_dir, exist_ok=True)
    target = os.path.join(target_dir, name)
    shutil.copy2(source, target)
    return target


def run_real_sheets(current):
    section("13. baseline on the real client sheets (rebuilt binary, read-only)")
    if not os.path.exists(current):
        check("rebuilt binary is available for the real-sheet baseline", False, current)
        return

    # --- sheet 002: the classic symbols, which carry no attributes at all ----- #
    sheet_002 = prepare_realsheet("FC1000495-002.dxf")
    before = dxfio.load(sheet_002)
    inserts_002 = [e for e in before.entities if e.kind == "INSERT"]
    with_attrs_002 = [e for e in inserts_002 if e.attributes]
    print(f"         FC1000495-002.dxf: {len(before.entities)} model-space entities, "
          f"{len(inserts_002)} inserts, {len(with_attrs_002)} with attributes, "
          f"{sum(len(e.attributes) for e in inserts_002)} ATTRIBs")

    bom = os.path.join(WORK, "real_bom_002.csv")
    batch = write_batch(os.path.join(HERE, "real_sheet_batch.txt"),
                        os.path.join(WORK, "real_batch_002.txt"), {"@BOM@": bom})
    code, text = run(current, [current, "--each", batch, sheet_002], "real_002")
    check("real sheet 002 batch exits 0", code == 0, f"exit {code}")
    # The sheet's symbols were exploded, so the tags survive only as TEXT. Round 2's
    # recovery reads them: 13 components on this sheet, 4 BOM rows.
    check("ELECLIST recovers the component tags from the sheet's TEXT",
          "CLASS" in text and "264CB" in text and "4214CON" in text, "")
    check("ELECBOM counts the recovered components",
          "ELECBOM: 13 component(s)" in text and "4 bill-of-material row(s)" in text, "")
    check("ELECBOM writes the CSV for that sheet", os.path.exists(bom),
          f"{bom} exists" if os.path.exists(bom) else "no file")
    check("ELECNUMBERPREVIEW still sees no attributed inserts on that sheet",
          "ELECNUMBER: no electrical components found" in text,
          "the exploded symbols carry no attributes")

    # --- sheet 003: the only sheets whose module inserts carry attributes ----- #
    sheet_003 = prepare_realsheet("FC1000495-003.dxf")
    before = dxfio.load(sheet_003)
    inserts_003 = [e for e in before.entities if e.kind == "INSERT"]
    with_attrs_003 = [e for e in inserts_003 if e.attributes]
    print(f"         FC1000495-003.dxf: {len(before.entities)} model-space entities, "
          f"{len(inserts_003)} inserts, {len(with_attrs_003)} with attributes, "
          f"{sum(len(e.attributes) for e in inserts_003)} ATTRIBs")

    bom = os.path.join(WORK, "real_bom_003.csv")
    batch = write_batch(os.path.join(HERE, "real_sheet_batch.txt"),
                        os.path.join(WORK, "real_batch_003.txt"), {"@BOM@": bom})
    code, text = run(current, [current, "--each", batch, sheet_003], "real_003")
    check("real sheet 003 batch exits 0", code == 0, f"exit {code}")
    listing = [line for line in text.split("ELECBOM:")[0].splitlines()
               if line.startswith(("HMS21", "HMO13"))]
    check("ELECLIST reports the module inserts that do carry attributes",
          len(listing) == 4, f"{len(listing)} component rows: {listing[:2]}")
    check("ELECBOM counts the attributed and recovered components",
          "ELECBOM: 10 component(s)" in text, "")
    check("ELECNUMBERPREVIEW sees the same components", "ELECNUMBER: 4 component" in text, "")
    if os.path.exists(bom):
        with open(bom, "r", encoding="utf-8") as handle:
            rows = [line for line in handle.read().splitlines() if line.strip()]
        check("ELECBOM CSV on real data has one row per (class, part)",
              len(rows) - 1 == 4, f"{len(rows) - 1} data rows: {rows[1:]}")
        check("the BOM 'Class name' column is the BLOCK NAME, not the ACADE class",
              any(row.startswith("HMS21,") for row in rows[1:])
              and any(row.startswith("HMO13,") for row in rows[1:]),
              "rows start with HMS21 / HMO13")
    else:
        check("ELECBOM wrote a CSV for sheet 003", False, "missing " + bom)

    # --- does ELECSHEET run once on a real sheet? --------------------------- #
    sheet_frame = prepare_realsheet("FC1000495-002.dxf")
    batch = write_batch(os.path.join(HERE, "real_sheet_sheet_batch.txt"),
                        os.path.join(WORK, "real_sheet_frame_batch.txt"), {})
    code, text = run(current, [current, "--each", batch, sheet_frame], "real_sheet_frame")
    check("ELECSHEET batch on a real sheet exits 0", code == 0, f"exit {code}")
    check("ELECSHEET reports exactly one summary (ran once)",
          text.count("ELECSHEET:") == 1, f"{text.count('ELECSHEET:')} summary lines")
    after = dxfio.load(sheet_frame)
    frame = on_layer(after, "ELEC-FRAME")
    frame_lines = len([e for e in frame if e.kind == "LINE"])
    frame_texts = len([e for e in frame if e.kind == "TEXT"])
    check("ELECSHEET drew one frame (16 lines, 5 title texts), not two",
          frame_lines == 16 and frame_texts == 5,
          f"{frame_lines} lines, {frame_texts} texts")
    match = re.search(r"fitted to the drawing at 1:(\d+) \(([\d.]+)x([\d.]+) units\)", text)
    print(f"         frame: {match.group(0) if match else 'no size in output'}")
    surviving = [e for e in after.entities if e.kind == "INSERT"]
    check("the real sheet's own geometry survives the ELECSHEET run",
          len(surviving) >= len(inserts_002), f"{len(surviving)} inserts before/after")


def run_machine_files():
    section("14. the machine-readable artifacts")
    # WD_M profile
    profile_path = os.path.join(CLIENT, "wd_m_profile.tsv")
    if not os.path.exists(profile_path):
        check("library/wd_m_profile.tsv exists", False, profile_path)
        return
    with open(profile_path, "r", encoding="ascii") as handle:
        lines = [line for line in handle.read().splitlines() if line.strip()]
    rows = {}
    for line in lines[1:]:
        columns = line.split("\t")
        rows[columns[0]] = dict(zip(
            ("value", "agreeing", "total", "present", "others"), columns[1:]))
    check("wd_m_profile.tsv has the full WD_M attribute set", len(rows) == 77,
          f"{len(rows)} attributes")
    check("wd_m_profile.tsv is uniform across the sheets",
          all(row["agreeing"] == "34" and row["total"] == "34" for row in rows.values()),
          f"all {len(rows)} attributes 34/34")
    wanted = {
        "TAGFMT": "%F%N", "TAGMODE": "R", "TAG-START": "1",
        "TAG-RSUF": ",A,B,C,D,E,F,G,H,J",
        "WIREFMT": "%N", "WIREMODE": "R", "WIRE-START": "1",
        "XREFFMT": "%N", "ALT_XREFFMT": "%N", "REFNUMS": "1",
        "WNUM_FLAGS": "1", "RUNGHORV": "H", "PH3SPACE": "0.5",
        "DISTH": "2.0", "DISTV": "2.0", "MISC_FLAGS": "0",
        "GAP_STYLE": "1", "HORIZ_FIRST": "1",
    }
    problems = [f"{key}={rows[key]['value']!r} (want {value!r})"
                for key, value in wanted.items()
                if key not in rows or rows[key]["value"] != value]
    check("the project numbering/wire settings are recorded verbatim",
          not problems, "; ".join(problems[:3]) or "18 settings as measured")

    # Harvest report
    report_path = os.path.join(CLIENT, "harvest_report.tsv")
    with open(report_path, "r", encoding="ascii") as handle:
        report = [line.split("\t") for line in handle.read().splitlines() if line.strip()]
    header, body = report[0], report[1:]
    check("harvest_report.tsv documents every symbol", len(body) == 18, f"{len(body)} rows")
    check("harvest_report.tsv records source sheet and geometry per symbol",
          all(row[2] != "-" and row[5] != "0" for row in body),
          "all rows carry a source sheet and entity count")
    check("harvest_report.tsv records the omitted nested blocks",
          any("anonymous nested block" in row[8] for row in body),
          next((row[8] for row in body if "anonymous" in row[8]), "none"))



def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baseline", default=None,
                        help="path to the pre-rebuild binary copy "
                             "(default: %%TEMP%%/ocs_baseline.exe)")
    parser.add_argument("--no-real-sheets", action="store_true",
                        help="skip the customer-sheet baseline: those fixtures are not in "
                             "the repository, so CI runs the rest of the suite")
    parser.add_argument("--current", default=None,
                        help="path to the current build "
                             "(default: %%TEMP%%/ocs_current.exe, else target/release)")
    args = parser.parse_args(argv)

    baseline = args.baseline or os.path.join(os.environ.get("TEMP", ""), "ocs_baseline.exe")
    if not os.path.exists(baseline):
        candidate = os.path.join(ROOT, "target", "release", "OpenCADStudio.exe")
        if os.path.exists(candidate):
            shutil.copy2(candidate, baseline)
        else:
            raise SystemExit(
                f"no baseline binary at {baseline}; copy the built release exe there first")
    current = args.current or os.path.join(os.environ.get("TEMP", ""), "ocs_current.exe")
    if not os.path.exists(current):
        candidate = os.path.join(ROOT, "target", "release", "OpenCADStudio.exe")
        if os.path.exists(candidate):
            shutil.copy2(candidate, current)
    print(f"baseline: {baseline}")
    print(f"current : {current}" + ("" if os.path.exists(current) else "  (MISSING)"))
    print(f"starter : {STARTER}")
    print(f"client  : {CLIENT}")

    symbols = expected_symbols(STARTER)
    print(f"starter manifest: {len(symbols)} symbols, "
          f"{sum(len(s['row']['attrs']) for s in symbols.values())} attribute tags, "
          f"{sum(s['segments'] for s in symbols.values())} palette segments")
    client_symbols = expected_symbols(CLIENT)
    print(f"client manifest : {len(client_symbols)} symbols, "
          f"{sum(len(s['row']['attrs']) for s in client_symbols.values())} attribute tags, "
          f"{sum(s['segments'] for s in client_symbols.values())} palette segments")

    prepare_work(STARTER)

    # ELECNEW/ELECPLACE fall back to %APPDATA%\AryaCAD\elec_library.txt, which a
    # previous session may have pointed somewhere else. Park it for the run.
    cache = appdata_cache_path()
    backup = None
    if cache and os.path.exists(cache):
        backup = cache + ".smoke_backup"
        shutil.move(cache, backup)
        print(f"parked remembered-library cache at {backup}")

    try:
        run_smoke(baseline, symbols)
        run_makesymbol(baseline)
        run_edge_cases(baseline)
        run_fabricated(baseline)
        run_client_library(baseline, current, client_symbols)
        if args.no_real_sheets:
            section("13. baseline on the real client sheets (skipped: --no-real-sheets)")
            run_machine_files()
        else:
            run_real_sheets(current)
            run_machine_files()
    finally:
        if cache and os.path.exists(cache):
            os.remove(cache)
        if backup and os.path.exists(backup):
            shutil.move(backup, cache)

    passed = sum(1 for _, ok, _ in CHECKS if ok)
    failed = [(label, detail) for label, ok, detail in CHECKS if not ok]
    print()
    print("=" * 70)
    print(f"{passed}/{len(CHECKS)} checks passed")
    if failed:
        for label, detail in failed:
            print(f"  FAILED: {label}" + (f"  ({detail})" if detail else ""))
    summary = os.path.join(WORK, "summary.txt")
    with open(summary, "w", encoding="utf-8") as handle:
        for label, ok, detail in CHECKS:
            handle.write(f"{'PASS' if ok else 'FAIL'}\t{label}\t{detail}\n")
    print(f"checks written to {summary}")
    return 0 if not failed else 1


if __name__ == "__main__":
    sys.exit(main())

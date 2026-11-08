#!/usr/bin/env python3
"""Headless proof of the ADVANCED feature rows of the SkyCAD-style table.

`run_smoke.py` covers the drafting path (library, insert, place, wire, junctions,
numbering, BOM).  This script covers the rows that no test touched before:
automated cross-references, terminal strips, PLC generation, I/O-list import,
cabling and cable schedules, panel layout, project reports, systems and
variants, revision control with rollback, built-project control and project
numbering, and the navigation commands.

Every check runs the already-built release binary with
``--each <batch> <drawing>`` and then verifies the *saved DXF* with
``tools/dxfio.py`` wherever the file is the honest observable.  Raw logs are kept
under ``tests/ecad/_work/features/logs/``.

Usage
-----
    py -3.13 tests/ecad/run_features.py
    py -3.13 tests/ecad/run_features.py --exe C:\\path\\to\\OpenCADStudio.exe
"""

from __future__ import annotations

import argparse
import os
import shutil
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))
TOOLS = os.path.join(ROOT, "tools")
sys.path.insert(0, TOOLS)
import dxfio  # noqa: E402

for _stream in (sys.stdout, sys.stderr):
    if hasattr(_stream, "reconfigure"):
        _stream.reconfigure(encoding="utf-8", errors="replace")

WORK = os.path.join(HERE, "_work", "features")
LOGS = os.path.join(WORK, "logs")
LIB = os.path.join(ROOT, "library", "starter")
EXE = os.path.join(ROOT, "target", "release", "OpenCADStudio.exe")

FINDINGS: list[tuple[str, bool, str]] = []


def check(label, ok, detail=""):
    FINDINGS.append((label, bool(ok), detail))
    print(f"[{'PASS' if ok else 'FAIL'}] {label}" + (f"  ({detail})" if detail else ""))
    return ok


def section(title):
    print()
    print(f"=== {title}")
    print()


def run(exe, args, label):
    print("$ " + " ".join(args))
    proc = subprocess.run(args, capture_output=True, text=True, cwd=ROOT,
                          encoding="utf-8", errors="replace")
    text = proc.stdout + proc.stderr
    os.makedirs(LOGS, exist_ok=True)
    with open(os.path.join(LOGS, f"{label}.log"), "w", encoding="utf-8") as handle:
        handle.write("$ " + " ".join(args) + "\n")
        handle.write(f"[exit code: {proc.returncode}]\n")
        handle.write(text)
    return proc.returncode, text


def blank(path):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    dxfio.DxfDocument().save(path)
    return path


def batch(name, lines):
    path = os.path.join(WORK, name)
    os.makedirs(WORK, exist_ok=True)
    with open(path, "w", encoding="ascii", newline="\n") as handle:
        handle.write("\n".join(lines) + "\n")
    return path


def each(exe, tag, lines, drawing):
    return run(exe, [exe, "--each", batch(f"{tag}_batch.txt", lines), drawing], tag)


def load(path):
    return dxfio.load(path)


def attribute_map(insert):
    return {a.s(2).strip().upper(): a.s(1).strip() for a in insert.attributes}


def insert_with_tag(drawing, tag):
    for entity in drawing.entities:
        if entity.kind != "INSERT":
            continue
        for attribute in entity.attributes:
            if attribute.s(2).strip().upper() == "TAG1" and \
                    attribute.s(1).strip().upper() == tag.upper():
                return entity
    return None


def texts(drawing):
    return [e.s(1) for e in drawing.entities if e.kind == "TEXT"]


# --------------------------------------------------------------------------- #
# sections
# --------------------------------------------------------------------------- #


def feature_cross_references(exe):
    section("1. Automated cross-references")
    sheet = blank(os.path.join(WORK, "xref.dxf"))
    code, text = each(exe, "xref", [
        f"ELECLIB {LIB}",
        "ELECNEW",
        "ELECINSERT CONTACTOR_COIL",
        "ELECINSERT CONTACTOR_COIL",
        "ELECSETATTR -K1 TAG1=-K1",
        "ELECSETATTR -K2 TAG1=-K1",
        "ELECXREF",
        "ELECXREFWRITE",
        "QSAVE",
    ], sheet)
    check("two parts of one device are paired", "1 device(s); 1 with more than one part" in text)
    check("both cross-reference labels are written", "2 label(s) created" in text)
    drawing = load(sheet)
    labels = [e.s(1) for e in drawing.entities
              if e.kind == "TEXT" and e.s(8).upper() == "XREF"]
    check("the saved sheet carries one cross-reference per part", len(labels) == 2,
          ", ".join(sorted(labels)))
    check("a cross-reference names the other part by tag and ordinal",
          sorted(labels) == ["-K1:1", "-K1:2"], ", ".join(sorted(labels)))


def feature_terminals(exe):
    section("2. Terminal strip management")
    sheet = blank(os.path.join(WORK, "terminals.dxf"))
    code, text = each(exe, "terminals", [
        f"ELECLIB {LIB}",
        "ELECNEW",
        "ELECINSERT CONTACTOR_COIL",
        "ELECINSERT TERMINAL",
        "ELECINSERT TERMINAL",
        "ELECSETATTR -X1 TAG1=X1:1",
        "ELECSETATTR -X2 TAG1=X1:2",
        "ELECTERMINALS",
        "ELECTSIDES",
        "QSAVE",
    ], sheet)
    check("a strip counts each terminal once", "1 strip(s), 2 terminal(s)" in text)
    check("both terminals are listed", "X1:1" in text and "X1:2" in text)
    check("non-terminals are listed once", "not terminals: -K1" in text)
    check("terminal sides are reported for both", "ELECTSIDES: 2 terminal(s)" in text)


def feature_plc(exe):
    section("3. Automated PLC drawings from an I/O list")
    sheet = blank(os.path.join(WORK, "plc.dxf"))
    code, text = each(exe, "plc", [
        f"ELECLIB {LIB}",
        "ELECNEW",
        "ELECPLC -P1 8 I0.0",
        "QSAVE",
    ], sheet)
    check("PLC module is generated", "-P1 with 8 point(s), I0.0..I0.7" in text)
    drawing = load(sheet)
    insert = insert_with_tag(drawing, "-P1")
    check("generated module is placed", insert is not None)
    if insert is not None:
        values = list(attribute_map(insert).values())
        check("first and last I/O addresses are on the block", "I0.0" in values and "I0.7" in values,
              ", ".join(sorted(values)))


def feature_io_import(exe):
    section("4. I/O list becomes PLC drawings (Excel import)")
    workbook = os.path.join(HERE, "io_list_sample.xlsx")
    if not os.path.exists(workbook):
        check("io_list_sample.xlsx present", False, workbook)
        return
    sheet = blank(os.path.join(WORK, "ioimport.dxf"))
    code, text = each(exe, "ioimport", [
        f"ELECLIB {LIB}",
        "ELECNEW",
        f"ELECIOIMPORT {workbook}",
        "ELECIO",
        "QSAVE",
    ], sheet)
    check("import reports generated modules", "ELECIOIMPORT:" in text)
    check("imported points are listed", "ELECIO:" in text)


def feature_cables(exe):
    section("5. Cabling and cable schedules")
    folder = os.path.join(WORK, "proj_cables")
    shutil.rmtree(folder, ignore_errors=True)
    blank(os.path.join(folder, "sheet1.dxf"))
    schedule = os.path.join(WORK, "cables.csv")
    sheet = os.path.join(folder, "sheet1.dxf")
    code, text = each(exe, "cables", [
        f"ELECPROJNEW {folder}",
        "ELECNEW",
        "ELECINSERT MOTOR",
        "ELECCABLE W1 CORES=3",
        "ELECCABLE W2 CORES=2 SHIELD",
        f"ELECCABLELIST {schedule}",
        "QSAVE",
    ], sheet)
    check("first cable is added", "added W1 — 3 core(s)" in text)
    check("schedule CSV is written", "2 row(s)" in text)
    rows = []
    if os.path.exists(schedule):
        with open(schedule, "r", encoding="utf-8", errors="replace") as handle:
            rows = [line for line in handle.read().splitlines() if line.strip()]
    check("schedule CSV has header and both cables", len(rows) == 3, f"{len(rows)} line(s)")


def feature_panel_report(exe):
    section("6. Panel layout and project reports")
    sheet = blank(os.path.join(WORK, "panel.dxf"))
    panel = os.path.join(WORK, "panel_export.dxf")
    report = os.path.join(WORK, "project_report.xlsx")
    code, text = each(exe, "panel", [
        f"ELECLIB {LIB}",
        "ELECNEW",
        "ELECINSERT MOTOR",
        "ELECINSERT CONTACTOR_COIL",
        "ELECINSERT TERMINAL",
        f"ELECPANEL {panel}",
        f"ELECREPORT {report}",
        "QSAVE",
    ], sheet)
    check("panel drawing is reported", "ELECPANEL: 3 device(s)" in text or "device(s); enclosure" in text)
    check("panel DXF exists", os.path.exists(panel))
    check("report workbook is written", "ELECREPORT: wrote" in text and os.path.exists(report))


def feature_systems(exe):
    section("7. Design by systems / sub-assemblies, and variants")
    folder = os.path.join(WORK, "proj_systems")
    shutil.rmtree(folder, ignore_errors=True)
    sheet = blank(os.path.join(folder, "sheet1.dxf"))
    code, text = each(exe, "systems", [
        f"ELECPROJNEW {folder}",
        f"ELECLIB {LIB}",
        "ELECNEW",
        "ELECSYSTEM LIST",
        "ELECSYSTEM INSERT DOL_STARTER 0,0",
        "ELECVARIANT NEW AuditVariant",
        "ELECVARIANT LIST",
        "QSAVE",
    ], sheet)
    check("shipped systems are listed", "DOL_STARTER" in text and "REVERSING_STARTER" in text)
    check("system is inserted with its devices and wires", "inserted DOL_STARTER" in text and "4 device(s)" in text)
    check("a project variant is created", "variant-AuditVariant.eprj" in text)


def feature_revisions(exe):
    section("8. Revision control and rollback")
    folder = os.path.join(WORK, "proj_rev")
    shutil.rmtree(folder, ignore_errors=True)
    sheet = blank(os.path.join(folder, "sheet1.dxf"))
    code, text = each(exe, "revisions", [
        f"ELECPROJNEW {folder}",
        "ELECNEW",
        "ELECINSERT MOTOR",
        "QSAVE",
        "ELECREV SNAPSHOT A",
        "ELECINSERT FUSE",
        "QSAVE",
        "ELECREV SNAPSHOT B",
        "ELECREV LIST",
        "ELECREV COMPARE 001 002",
        "ELECREV RESTORE 001",
        "ELECLIST",
        "QSAVE",
    ], sheet)
    check("snapshots are taken", "snapshot 001-A" in text and "snapshot 002-B" in text)
    check("snapshot list is printed", "001-A" in text and "002-B" in text)
    check("compare accepts the bare numbers people type", "ELECREV: 001-A -> 002-B" in text)
    check("restore rewrites the project files", "restored 2 file(s) from 001-A" in text)
    check("an open drawing is reloaded after restore", "Reloaded sheet1.dxf." in text)
    check("the restored design no longer has the fuse", "Fuse" not in text.split("Reloaded")[-1])


def feature_built_control(exe):
    section("9. Built-project control and project numbering")
    folder = os.path.join(WORK, "proj_built")
    shutil.rmtree(folder, ignore_errors=True)
    sheet = blank(os.path.join(folder, "sheet1.dxf"))
    each(exe, "built_setup", [
        f"ELECPROJNEW {folder}",
        f"ELECLIB {LIB}",
        "ELECNEW",
        "ELECINSERT MOTOR",
        "ELECSETATTR -M1 TAG1=-M9",
        "QSAVE",
    ], sheet)
    code, text = each(exe, "built_control", [
        f"ELECPROJOPEN {folder}\\project.eprj",
        f"ELECLIB {LIB}",
        "ELECNEW",
        "ELECPROJNUM",
        "ELECBUILT on",
        "ELECPROJNUM apply",
        "ELECBUILT off",
        "ELECPROJNUM apply",
        "ELECGOTO -M1",
        "ELECHIGHLIGHT -M1",
        "QSAVE",
    ], sheet)
    check("preview counts the device that needs a new number",
          "1 device(s); would renumber 1" in text, first(text, "ELECPROJNUM"))
    check("a built project refuses to renumber", "flagged built — refusing to renumber" in text)
    check("design mode renumbers the project", "renumbered 1" in text)
    check("the open sheet is reloaded with the new number",
          "1 open drawing(s) reloaded." in text)
    check("goto finds the renumbered tag", "ELECGOTO: centred on -M1" in text)
    check("highlight selects it", "ELECHIGHLIGHT: selected 1 part(s)" in text)
    drawing = load(sheet)
    insert = insert_with_tag(drawing, "-M1")
    check("the saved drawing carries the new tag", insert is not None)


def first(text, needle):
    for line in text.splitlines():
        if needle in line:
            return line.strip()
    return ""


# --------------------------------------------------------------------------- #
# main
# --------------------------------------------------------------------------- #


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--exe", default=EXE, help="built release binary")
    args = parser.parse_args(argv)
    exe = os.path.abspath(args.exe)
    print(f"features: {exe}")
    if not os.path.exists(exe):
        raise SystemExit(f"missing binary: {exe}")
    shutil.rmtree(WORK, ignore_errors=True)
    os.makedirs(WORK, exist_ok=True)

    feature_cross_references(exe)
    feature_terminals(exe)
    feature_plc(exe)
    feature_io_import(exe)
    feature_cables(exe)
    feature_panel_report(exe)
    feature_systems(exe)
    feature_revisions(exe)
    feature_built_control(exe)

    failed = [f for f in FINDINGS if not f[1]]
    print()
    print(f"features: {len(FINDINGS) - len(failed)}/{len(FINDINGS)} checks passed")
    if failed:
        for label, _, detail in failed:
            print(f"  FAILED {label}" + (f"  ({detail})" if detail else ""))
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

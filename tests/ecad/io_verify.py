#!/usr/bin/env python3
"""Run an ELECIOIMPORT batch headless and read the saved drawing back.

    python tests/ecad/io_verify.py

The executable is a GUI-subsystem program, so a shell does not wait for it; this runner
starts it with redirected pipes, waits for the exit code, quotes everything it printed, and
then verifies the *saved file* with read_io.py.

The batch's own save row is replaced with `SAVEAS <scratch>/out.dxf`, so the run never has
to overwrite the drawing it opened. That keeps the canonical batches (which end in `QSAVE`)
usable on a normal desktop and makes the verification work in a sandbox that refuses to
replace an open file.

Options:
    --exe PATH    release executable (default target/release/OpenCADStudio.exe)
    --batch PATH  batch to run (default tests/ecad/io_batch.txt)
    --xlsx PATH   workbook the ELECIOIMPORT row should import
                  (default tests/ecad/io_list_sample.xlsx)
"""

from __future__ import annotations

import argparse
import os
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
FIXTURE = ROOT / "tests" / "fixtures" / "embedded-image-print.dxf"


def prepare_batch(text: str, xlsx: Path, out: Path, keep_imports: bool = False) -> str:
    """Point the import at this workbook and the save at a fresh file."""
    lines: list[str] = []
    for line in text.splitlines():
        stripped = line.strip()
        upper = stripped.upper()
        if upper.startswith("ELECIOIMPORT ") and not keep_imports:
            parts = stripped.split(maxsplit=2)
            tail = f" {parts[2]}" if len(parts) > 2 else ""
            lines.append(f"ELECIOIMPORT {xlsx}{tail}")
        elif upper.startswith(("QSAVE", "SAVEAS")):
            lines.append(f"SAVEAS {out}")
        else:
            lines.append(line)
    return "\n".join(lines) + "\n"


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--exe", default=str(ROOT / "target" / "release" / "OpenCADStudio.exe"))
    parser.add_argument("--batch", default=str(ROOT / "tests" / "ecad" / "io_batch.txt"))
    parser.add_argument("--xlsx", default=str(ROOT / "tests" / "ecad" / "io_list_sample.xlsx"))
    parser.add_argument("--keep-imports", action="store_true",
                        help="do not rewrite ELECIOIMPORT rows (for the refusal batch)")
    parser.add_argument("--expect-exit", type=int, default=0,
                        help="the exit code the app run should have (the refusal batch is 1)")
    parser.add_argument("--skip-read", action="store_true",
                        help="do not read the saved drawing back (nothing was generated)")
    args = parser.parse_args(argv[1:])

    exe = Path(args.exe).resolve()
    if not exe.is_file():
        print(f"no executable at {exe}", file=sys.stderr)
        return 2
    xlsx = Path(args.xlsx).resolve()
    if not xlsx.is_file():
        print(f"no workbook at {xlsx}", file=sys.stderr)
        return 2
    source_batch = Path(args.batch).read_text(encoding="utf-8")

    # A plain mkdir under target/ inherits the directory ACL, so the app and this script can
    # both write here; tempfile.mkdtemp creates a directory the sandbox then refuses.
    scratch = ROOT / "target" / f"io-verify-{os.getpid()}"
    if scratch.exists():
        shutil.rmtree(scratch)
    scratch.mkdir(parents=True)
    drawing = scratch / "io.dxf"
    saved = scratch / "out.dxf"
    shutil.copyfile(FIXTURE, drawing)
    batch = scratch / "io_batch.txt"
    batch.write_text(
        prepare_batch(source_batch, xlsx, saved, args.keep_imports), encoding="utf-8"
    )

    print(f"exe    : {exe}")
    print(f"batch  : {batch}")
    print(f"xlsx   : {xlsx}")
    print(f"drawing: {drawing}")
    print(f"saved  : {saved}")
    print("-" * 78)

    proc = subprocess.run(
        [str(exe), "--each", str(batch), str(drawing)],
        cwd=str(ROOT),
        capture_output=True,
        text=True,
        timeout=600,
    )
    print(proc.stdout, end="")
    if proc.stderr.strip():
        print("--- stderr ---")
        print(proc.stderr, end="")
    print("-" * 78)
    print(f"exit code: {proc.returncode} (expected {args.expect_exit})")
    if proc.returncode != args.expect_exit:
        print("FAIL: the command did not exit with the expected code", file=sys.stderr)
        return 1
    if args.skip_read:
        return 0
    if not saved.is_file():
        print(f"FAIL: no drawing was saved at {saved}", file=sys.stderr)
        return 1
    print(f"saved drawing: {saved.stat().st_size} bytes")
    print("-" * 78)

    verify = subprocess.run(
        [sys.executable, str(ROOT / "tests" / "ecad" / "read_io.py"), str(saved)],
        cwd=str(ROOT),
        capture_output=True,
        text=True,
        timeout=300,
    )
    print(verify.stdout, end="")
    if verify.stderr.strip():
        print(verify.stderr, end="", file=sys.stderr)
    return 0 if proc.returncode == 0 and verify.returncode == 0 else 1


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))

"""Capture the electrical UI screenshots in docs/screenshots/ from the real app.

Every picture is a compositor capture of the running editor, taken through the
GUI REST bridge (``--http PORT``), so the screenshots document the shipped
windows rather than a mock-up.  One app instance is started per picture: a
dialog modal would otherwise have to be dismissed between shots, and closing a
modal over HTTP is exactly the kind of UI choreography a capture script should
not depend on.

Usage
-----
    py -3.13 scripts/capture_ecad_screenshots.py [--exe PATH] [--only NAME ...]

The default executable is ``target/release/OpenCADStudio.exe`` and the default
sheet is a copy of ``samples/demo-line/sheet1.dxf`` (copied to ``target/tmp``
so the sample folder keeps no lock file).
"""

from __future__ import annotations

import argparse
import json
import os
import shutil
import subprocess
import sys
import time
import urllib.error
import urllib.request

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
DEFAULT_EXE = os.path.join(ROOT, "target", "release", "OpenCADStudio.exe")
SAMPLE = os.path.join(ROOT, "samples", "demo-line", "sheet1.dxf")
OUT = os.path.join(ROOT, "docs", "screenshots")
LIBRARY = os.path.join(ROOT, "library", "starter")

# The GUI bridge authenticates every loopback request with a bearer token. Pin
# it before launch so the script does not have to parse the one-time token the
# editor prints on startup.
TOKEN = "ecad-doc-capture"

# name -> command lines run, in order, before the capture
SHOTS = [
    ("ecad-palette", ["ELECLIB " + LIBRARY, "ELECPALETTE"]),
    ("ecad-symbol-manager", ["ELECLIB " + LIBRARY, "ELECLIBMANAGER"]),
    ("ecad-numbering-setup", ["ELECSETUPEDIT"]),
    ("ecad-bill-of-material", ["ELECBOM"]),
    ("ecad-terminal-strips", ["ELECTERMINALS"]),
    ("ecad-wire-list", ["ELECWIRELIST"]),
]


def post(port: int, route: str, body: dict, timeout: float = 120.0) -> dict:
    data = json.dumps(body).encode("utf-8")
    request = urllib.request.Request(
        f"http://127.0.0.1:{port}/api/v1/{route}",
        data=data,
        headers={
            "Content-Type": "application/json",
            "Authorization": f"Bearer {TOKEN}",
        },
        method="POST",
    )
    try:
        with urllib.request.urlopen(request, timeout=timeout) as response:
            return json.loads(response.read().decode("utf-8"))
    except urllib.error.HTTPError as error:
        detail = error.read().decode("utf-8", "replace")[:500]
        raise RuntimeError(f"HTTP {error.code} on {route}: {detail}") from error


def get(port: int, route: str, timeout: float = 10.0) -> dict:
    request = urllib.request.Request(
        f"http://127.0.0.1:{port}/api/v1/{route}",
        headers={"Authorization": f"Bearer {TOKEN}"},
    )
    with urllib.request.urlopen(request, timeout=timeout) as response:
        return json.loads(response.read().decode("utf-8"))


def wait_ready(port: int, process: subprocess.Popen, deadline: float) -> dict:
    """Wait until the editor answers AND the sample drawing is the active tab."""
    while time.time() < deadline:
        if process.poll() is not None:
            raise RuntimeError(f"editor exited early with {process.returncode}")
        try:
            state = get(port, "state")
            documents = state.get("documents") or []
            opened = any(document.get("path") for document in documents)
            if state.get("ok") and state.get("document_id") and opened:
                time.sleep(0.8)
                return state
        except (urllib.error.URLError, OSError, RuntimeError):
            pass
        time.sleep(0.25)
    raise RuntimeError(f"editor did not answer on port {port}")


def capture(name: str, commands: list[str], exe: str, sheet: str, port: int) -> str:
    process = subprocess.Popen(
        [exe, sheet, "--http", str(port)],
        cwd=ROOT,
        env={**os.environ, "OCS_API_TOKEN": TOKEN},
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    try:
        state = wait_ready(port, process, time.time() + 90.0)
        document = state.get("document_id")
        for command in commands:
            reply = post(port, "run", {"cmd": command, "document_id": document})
            if reply.get("ok") is False:
                raise RuntimeError(f"{name}: {command} failed: {reply}")
            time.sleep(0.6)
        target = os.path.join(OUT, name + ".png")
        reply = post(
            port,
            "capture",
            {"path": target, "document_id": document},
            timeout=180.0,
        )
        if reply.get("ok") is False:
            raise RuntimeError(f"{name}: capture failed: {reply}")
        if not os.path.exists(target):
            raise RuntimeError(f"{name}: capture reported ok but wrote nothing")
        return target
    finally:
        process.terminate()
        try:
            process.wait(timeout=15)
        except subprocess.TimeoutExpired:
            process.kill()


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--exe", default=DEFAULT_EXE)
    parser.add_argument("--only", nargs="*", default=None)
    parser.add_argument("--port", type=int, default=8642)
    args = parser.parse_args()

    if not os.path.exists(args.exe):
        print(f"missing executable: {args.exe}", file=sys.stderr)
        return 2
    os.makedirs(OUT, exist_ok=True)
    work = os.path.join(ROOT, "target", "tmp", "shots")
    os.makedirs(work, exist_ok=True)
    sheet = os.path.join(work, "sheet1.dxf")
    shutil.copyfile(SAMPLE, sheet)

    failures = 0
    for name, commands in SHOTS:
        if args.only and name not in args.only:
            continue
        try:
            path = capture(name, commands, args.exe, sheet, args.port)
            print(f"captured {os.path.relpath(path, ROOT)}")
        except Exception as error:  # noqa: BLE001 - the report is the point
            failures += 1
            print(f"FAILED {name}: {error}", file=sys.stderr)
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())

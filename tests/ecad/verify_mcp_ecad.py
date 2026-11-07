#!/usr/bin/env python3
"""End-to-end check of the ECAD side of the MCP server and node graph.

The editor must already be open on an electrical drawing (or pass --drawing and
this script launches one). It talks to ``OpenCADStudio.exe --mcp`` over stdio
using newline-delimited JSON-RPC, exactly like an AI client would.

Checks:
  * the ECAD read tool and intent tool are advertised
  * the ``cad://guides/ecad`` resource exists and is readable
  * ``ocs_read {"op":"electrical"}`` returns the sheet wire graph
  * ``detail: summary`` trims the bulky arrays
  * the node graph library lists the Electrical category and its nodes
  * an Electrical node can be added, run and read back

Usage:
    py -3.13 tests/ecad/verify_mcp_ecad.py [--exe target/release/OpenCADStudio.exe]
                                           [--drawing samples/demo-line/sheet1.dxf]
                                           [--keep-open]
"""

from __future__ import annotations

import argparse
import json
import os
import queue
import subprocess
import sys
import threading
import time

CHECKS: list[tuple[str, bool, str]] = []


def check(name: str, ok: bool, detail: str = "") -> bool:
    CHECKS.append((name, bool(ok), detail))
    state = "PASS" if ok else "FAIL"
    suffix = f"  ({detail})" if detail else ""
    print(f"[{state}] {name}{suffix}")
    return bool(ok)


class McpBridge:
    """Minimal MCP stdio client: one JSON object per line, ids on responses."""

    def __init__(self, exe: str, cwd: str) -> None:
        env = dict(os.environ)
        env["RUST_BACKTRACE"] = "0"
        self.proc = subprocess.Popen(
            [exe, "--mcp"],
            cwd=cwd,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            encoding="utf-8",
            errors="replace",
            bufsize=1,
            env=env,
        )
        self.lines: "queue.Queue[str]" = queue.Queue()
        threading.Thread(target=self._pump, daemon=True).start()
        self.next_id = 0

    def _pump(self) -> None:
        assert self.proc.stdout is not None
        for line in self.proc.stdout:
            self.lines.put(line)
        self.lines.put("")

    def _send(self, message: dict) -> None:
        assert self.proc.stdin is not None
        self.proc.stdin.write(json.dumps(message) + "\n")
        self.proc.stdin.flush()

    def request(self, method: str, params: dict | None = None, timeout: float = 180.0) -> dict:
        self.next_id += 1
        request_id = self.next_id
        message = {"jsonrpc": "2.0", "id": request_id, "method": method}
        if params is not None:
            message["params"] = params
        self._send(message)
        deadline = time.monotonic() + timeout
        while True:
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise TimeoutError(f"{method} did not answer in {timeout:.0f}s")
            try:
                line = self.lines.get(timeout=min(0.5, remaining))
            except queue.Empty:
                continue
            if line == "":
                raise RuntimeError(f"MCP server closed while waiting for {method}")
            try:
                payload = json.loads(line)
            except json.JSONDecodeError:
                continue
            if payload.get("id") == request_id:
                if "error" in payload:
                    raise RuntimeError(f"{method} failed: {payload['error']}")
                return payload.get("result", {})

    def notify(self, method: str, params: dict | None = None) -> None:
        message = {"jsonrpc": "2.0", "method": method}
        if params is not None:
            message["params"] = params
        self._send(message)

    def close(self) -> None:
        try:
            if self.proc.stdin is not None:
                self.proc.stdin.close()
        except OSError:
            pass
        try:
            self.proc.wait(timeout=5)
        except subprocess.TimeoutExpired:
            self.proc.kill()


def call_tool(bridge: McpBridge, name: str, arguments: dict) -> dict:
    result = bridge.request("tools/call", {"name": name, "arguments": arguments})
    if result.get("isError"):
        raise RuntimeError(f"{name} returned an error: {result}")
    return result


_REQUEST_SEQUENCE = 0


def _request_id() -> str:
    """Every read/execute call needs its own short request id (max 128 bytes)."""
    global _REQUEST_SEQUENCE
    _REQUEST_SEQUENCE += 1
    # The editor remembers request ids for its lifetime, so keep them unique
    # across repeated script runs against the same session.
    return f"ecad-{os.getpid()}-{_REQUEST_SEQUENCE}"


def call_ocs_read(bridge: McpBridge, op: str, session_id: str, **parameters: object) -> dict:
    # ocs_read takes the operation in "op" and the op-specific request in "parameters".
    payload = {
        "op": op,
        "ocs_session_id": session_id,
        "parameters": {**parameters, "request_id": _request_id()},
    }
    result = call_tool(bridge, "ocs_read", payload)
    return result


def call_ocs_execute(bridge: McpBridge, op: str, session_id: str, **arguments: object) -> dict:
    # ocs_execute takes an editor request envelope under "request".
    request = {"op": op, "request_id": _request_id(), **arguments}
    payload = {"ocs_session_id": session_id, "request": request}
    result = call_tool(bridge, "ocs_execute", payload)
    return result


def structured(result: dict) -> dict:
    """tools/call answers carry the op result as JSON text plus structuredContent."""
    if isinstance(result.get("structuredContent"), dict):
        return result["structuredContent"]
    content = result.get("content") or []
    for item in content:
        if item.get("type") == "text":
            try:
                return json.loads(item["text"])
            except json.JSONDecodeError:
                continue
    return {}


def text_of(result: dict) -> str:
    content = result.get("content") or []
    return "\n".join(item.get("text", "") for item in content if item.get("type") == "text")


def session_id_from(bridge: McpBridge) -> str:
    result = call_tool(bridge, "ocs_sessions", {})
    payload = structured(result)
    sessions = payload.get("result")
    if isinstance(sessions, dict):
        sessions = sessions.get("sessions")
    if sessions is None:
        sessions = payload.get("sessions") or payload.get("items") or []
    if not sessions:
        raise RuntimeError(f"no editor session: {json.dumps(payload)[:400]}")
    # Prefer the most recently active session.
    def key(row: dict) -> float:
        return float(row.get("updated_unix") or row.get("created_unix") or 0.0)

    sessions = sorted(sessions, key=key)
    entry = sessions[-1]
    return entry.get("session_id") or entry.get("id") or entry.get("name")


def wait_for_session(bridge: McpBridge, timeout: float = 90.0) -> str:
    deadline = time.monotonic() + timeout
    last = ""
    while time.monotonic() < deadline:
        try:
            return session_id_from(bridge)
        except Exception as error:  # noqa: BLE001 - retried until the deadline
            last = str(error)
            time.sleep(1.0)
    raise TimeoutError(f"no editor session appeared: {last}")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--exe", default=os.path.join("target", "release", "OpenCADStudio.exe"))
    parser.add_argument("--drawing", default=os.path.join("samples", "demo-line", "sheet1.dxf"))
    parser.add_argument("--keep-open", action="store_true", help="leave the launched editor running")
    args = parser.parse_args()

    root = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
    os.chdir(root)
    exe = os.path.abspath(args.exe)
    drawing = os.path.abspath(args.drawing)
    if not os.path.isfile(exe):
        print(f"missing editor: {exe}")
        return 2
    if not os.path.isfile(drawing):
        print(f"missing drawing: {drawing}")
        return 2

    editor = None
    if not args.keep_open:
        editor = subprocess.Popen(
            [exe, "--automation", drawing],
            cwd=root,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        time.sleep(6.0)

    bridge = McpBridge(exe, root)
    session = ""
    try:
        init = bridge.request(
            "initialize",
            {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {"name": "ecad-verify", "version": "1.0"},
            },
        )
        bridge.notify("notifications/initialized")
        check("MCP initialize answers with server info", bool(init.get("serverInfo")), str(init.get("protocolVersion")))

        tools = bridge.request("tools/list")
        names = {tool["name"] for tool in tools.get("tools", [])}
        check("ocs_electrical ECAD read tool is advertised", "ocs_electrical" in names)
        check("ocs_read and ocs_execute are advertised", {"ocs_read", "ocs_execute"} <= names)

        session = wait_for_session(bridge)
        check("editor session discovered", bool(session), session)

        # Reads that target the drawing must name its document explicitly.
        state = structured(call_ocs_read(bridge, "state", session))
        document_id = state.get("document_id") or ""
        if not document_id:
            documents = state.get("documents") or []
            if documents:
                document_id = documents[0].get("document_id") or ""
        check("editor reports the open document", bool(document_id), document_id)

        resources = bridge.request("resources/list")
        uris = {row.get("uri") for row in resources.get("resources", [])}
        check("cad://guides/ecad resource is listed", "cad://guides/ecad" in uris)

        guide = bridge.request("resources/read", {"uri": "cad://guides/ecad"})
        guide_text = text_of(guide) or json.dumps(guide)
        check("ECAD guide explains the wiring commands", "ELECWIRE" in guide_text and "WIRES" in guide_text)
        check("ECAD guide names the MCP electrical read", "electrical" in guide_text.lower())

        full = structured(call_ocs_read(bridge, "electrical", session, document_id=document_id))
        check("electrical read succeeds", bool(full.get("ok")), str(full.get("error", "")))
        nets = full.get("nets") or []
        components = full.get("components") or []
        pins = full.get("pins") or []
        check(
            "electrical read returns the sheet wire graph",
            isinstance(nets, list) and len(nets) > 0 and len(pins) > 0,
            f"{len(nets)} net(s), {len(pins)} pin(s), {len(components)} component(s)",
        )
        if nets:
            first = nets[0]
            check(
                "net rows carry a name and member pins",
                isinstance(first, dict) and bool(first.get("pins")),
                f"{first.get('name')}: {' '.join(map(str, first.get('pins') or []))[:80]}",
            )

        summary = structured(call_ocs_read(bridge, "electrical", session, document_id=document_id, detail="summary"))
        check(
            "summary detail omits the bulky arrays",
            "pins" not in summary and "components" not in summary and "pin_count" in summary,
            f"pin_count={summary.get('pin_count')}",
        )

        library = structured(call_ocs_execute(bridge, "node_graph", session, action="library"))
        library_text = json.dumps(library)
        check("node graph library includes the Electrical category", "Electrical" in library_text)
        check(
            "Electrical node types are listed",
            all(
                needle in library_text
                for needle in ("Electrical Netlist", "Electrical Components", "Electrical Wire Length")
            ),
        )

        call_ocs_execute(bridge, "node_graph", session, action="clear")
        add = structured(
            call_ocs_execute(bridge, "node_graph", session, action="add", type="Electrical Netlist")
        )
        state_text = json.dumps(add)
        check("Electrical Netlist node can be added", "Netlist" in state_text)

        run = structured(call_ocs_execute(bridge, "node_graph", session, action="run"))
        run_text = json.dumps(run)
        check(
            "Electrical node evaluates net data",
            ("-K" in run_text) or ("net" in run_text.lower() and "N1" in run_text),
            run_text[:120],
        )
        call_ocs_execute(bridge, "node_graph", session, action="clear")
    except Exception as error:  # noqa: BLE001 - report the failure as a check
        check("MCP ECAD end-to-end run", False, str(error)[:300])
    finally:
        bridge.close()
        if editor is not None and not args.keep_open:
            editor.terminate()
            try:
                editor.wait(timeout=5)
            except subprocess.TimeoutExpired:
                editor.kill()

    passed = sum(1 for _, ok, _ in CHECKS if ok)
    print()
    print(f"{passed}/{len(CHECKS)} MCP ECAD checks passed")
    return 0 if passed == len(CHECKS) and CHECKS else 1


if __name__ == "__main__":
    sys.exit(main())

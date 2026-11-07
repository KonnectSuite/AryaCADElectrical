"""Capture the ECAD documentation screenshots through the MCP capture tool.

The editor must already be running with ``--automation``; this script never
starts or focuses a window.  It opens the shipped templates, frames them with
``view: extents`` and writes the PNGs under ``docs/screenshots``.
"""

from __future__ import annotations

import base64
import json
import os
import shutil
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

from verify_mcp_ecad import McpBridge, call_tool, structured, call_ocs_execute, session_id_from  # noqa: E402


ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
OUT = os.path.join(ROOT, "docs", "screenshots")


def capture(
    bridge: McpBridge,
    session: str,
    name: str,
    *,
    scope: str,
    view: str = "current",
    bounds: list[float] | None = None,
) -> str:
    # The capture op writes a real PNG and reports its path; inline image bytes
    # are only part of the content when the editor is not waiting on a modal.
    arguments = {
        "ocs_session_id": session,
        "scope": scope,
        "view": view,
        "delivery": "inline",
        "max_dimension": 2000,
    }
    if bounds is not None:
        arguments["bounds"] = bounds
    result = bridge.request("tools/call", {"name": "ocs_capture", "arguments": arguments})
    target = os.path.join(OUT, name)
    for item in result.get("content") or []:
        if item.get("type") != "image":
            continue
        data = item.get("data") or (item.get("source") or {}).get("data") or ""
        if data:
            with open(target, "wb") as handle:
                handle.write(base64.b64decode(data))
            print(f"wrote {target} ({os.path.getsize(target)} bytes)")
            return target
    for item in result.get("content") or []:
        if item.get("type") != "text":
            continue
        try:
            payload = json.loads(item["text"])
        except json.JSONDecodeError:
            continue
        path = ((payload.get("result") or {}).get("path")) or ""
        if path and os.path.isfile(path):
            shutil.copyfile(path, target)
            print(f"wrote {target} ({os.path.getsize(target)} bytes)")
            return target
    raise RuntimeError(f"capture {name} returned no image: {json.dumps(result)[:300]}")


def open_drawing(bridge: McpBridge, session: str, relative: str) -> None:
    path = os.path.join(ROOT, relative)
    if not os.path.isfile(path):
        raise FileNotFoundError(path)
    response = structured(call_ocs_execute(bridge, "open", session, path=path))
    if not response.get("ok", False):
        raise RuntimeError(f"open {relative} failed: {json.dumps(response)[:300]}")


def main() -> int:
    os.makedirs(OUT, exist_ok=True)
    bridge = McpBridge(os.path.join(ROOT, "target", "release", "OpenCADStudio.exe"), ROOT)
    try:
        bridge.request(
            "initialize",
            {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {"name": "ecad-screens", "version": "1.0"},
            },
        )
        bridge.notify("notifications/initialized")
        session = session_id_from(bridge)

        # Startup chooser: reopen it after the sample drawing has loaded.
        structured(call_ocs_execute(bridge, "run", session, cmd="ELECSTART"))
        capture(bridge, session, "startup-chooser.png", scope="window")
        # The chooser is modal; close it before opening another drawing.
        structured(call_ocs_execute(bridge, "action", session, name="close_modal"))

        open_drawing(bridge, session, os.path.join("templates", "iec", "A3_landscape.dxf"))
        # Frame the sheet with margin so the command-line overlay does not hide
        # the title block's bottom row; extra space below lifts the sheet clear
        # of the overlay that the app draws over the bottom of the canvas.
        capture(
            bridge,
            session,
            "template-iec-a3.png",
            scope="window",
            view="region",
            bounds=[-25.0, -95.0, 445.0, 322.0],
        )

        open_drawing(bridge, session, os.path.join("templates", "nec", "ANSI_B_tabloid.dxf"))
        capture(
            bridge,
            session,
            "template-nec-b.png",
            scope="window",
            view="region",
            bounds=[-30.0, -95.0, 462.0, 309.0],
        )

        # The Electrical nodes in the node graph, with the sample schematic loaded.
        open_drawing(bridge, session, os.path.join("samples", "demo-line", "sheet1.dxf"))
        structured(call_ocs_execute(bridge, "node_graph", session, action="show"))
        structured(call_ocs_execute(bridge, "node_graph", session, action="add", type="Electrical Netlist"))
        structured(call_ocs_execute(bridge, "node_graph", session, action="run"))
        capture(bridge, session, "node-graph-electrical.png", scope="window")
        structured(call_ocs_execute(bridge, "node_graph", session, action="clear"))
        structured(call_ocs_execute(bridge, "node_graph", session, action="hide"))
        return 0
    finally:
        bridge.close()


if __name__ == "__main__":
    sys.exit(main())

"""Minimal, dependency-free DXF R12 (AC1009) writer and reader.

Why this exists
---------------
The electrical symbol library the app expects is "one plain DXF per symbol, the
file name giving the block name" (``src/app/commands/elec.rs``, ``elec_lib``).
Writing those files by hand does not scale, and no third-party DXF package may
be assumed on the machine, so this module carries just enough DXF to:

* emit ASCII DXF R12 -- the oldest and most widely accepted flavour, and the one
  the document codec's own ``r12`` writer round-trips through;
* read back what the app saved, so verification is done against the file on
  disk instead of against console output.

Writer
------
``DxfDocument`` builds a document with layers, optional block definitions and
model-space entities, then ``save()`` writes it.  Entities: LINE, CIRCLE, ARC,
LWPOLYLINE, TEXT, POINT, INSERT (with ATTRIB children), ATTDEF.

Reader
------
``load()`` parses a DXF into sections and returns a ``Drawing`` with:
``version`` (``$ACADVER``), ``layers`` (list of names from the LAYER table),
``blocks`` (name -> ``Block`` with ``base`` and ``entities``) and ``entities``
(model-space entities).  Entities are ``Entity`` objects exposing ``kind``,
``layer`` and raw group-code access (``e.s(8)``, ``e.f(10)`` ...).  INSERT
entities carry ``attributes`` (a list of ``Entity`` ATTRIBs).  POLYLINE +
VERTEX is folded into ``vertices``.

Only what the ECAD verification needs is parsed; unknown entities are preserved
verbatim rather than dropped.
"""

from __future__ import annotations

# --------------------------------------------------------------------------- #
# value formatting
# --------------------------------------------------------------------------- #


def _num(value) -> str:
    """Format a coordinate the way DXF files in the wild do: shortest sane form."""
    if isinstance(value, int):
        return str(value)
    text = f"{float(value):.6f}".rstrip("0").rstrip(".")
    if text in ("", "-"):
        text = "0"
    # DXF R12 readers accept a bare integer, but a trailing ".0" is conventional.
    if "." not in text:
        text += ".0"
    return text


class DxfDocument:
    """An R12 drawing under construction."""

    def __init__(self, extents=None):
        self.layers = {}          # name -> (color_index, linetype)
        self.blocks = []          # list of dicts: name, base, layer, flags, entities
        self.entities = []        # model-space entity token lists
        self.raw_entities = []    # model-space token lists emitted verbatim (harvester)
        self.extents = extents    # (xmin, ymin, xmax, ymax) or None
        self.add_layer("0", 7)

    def add_raw(self, token):
        """Append an already-built entity token list (used by the harvester)."""
        self.raw_entities.append(list(token))
        return self


    # -- construction ------------------------------------------------------ #

    def add_layer(self, name, color=7, linetype="CONTINUOUS"):
        if name not in self.layers:
            self.layers[name] = (color, linetype)
        return self

    @staticmethod
    def _line(x1, y1, x2, y2, layer="0"):
        return [
            (0, "LINE"), (8, layer),
            (10, _num(x1)), (20, _num(y1)), (30, _num(0)),
            (11, _num(x2)), (21, _num(y2)), (31, _num(0)),
        ]

    def add_line(self, x1, y1, x2, y2, layer="0"):
        self.entities.append(self._line(x1, y1, x2, y2, layer))
        return self

    @staticmethod
    def _circle(cx, cy, r, layer="0"):  # noqa: D401 - uniform entity builder
        return [
            (0, "CIRCLE"), (8, layer),
            (10, _num(cx)), (20, _num(cy)), (30, _num(0)),
            (40, _num(r)),
        ]

    def add_circle(self, cx, cy, r, layer="0"):
        self.entities.append(self._circle(cx, cy, r, layer))
        return self

    @staticmethod
    def _arc(cx, cy, r, start_deg, end_deg, layer="0"):
        return [
            (0, "ARC"), (8, layer),
            (10, _num(cx)), (20, _num(cy)), (30, _num(0)),
            (40, _num(r)), (50, _num(start_deg)), (51, _num(end_deg)),
        ]

    def add_arc(self, cx, cy, r, start_deg, end_deg, layer="0"):
        self.entities.append(self._arc(cx, cy, r, start_deg, end_deg, layer))
        return self

    @staticmethod
    def _lwpolyline(points, closed=False, layer="0"):
        token = [
            (0, "LWPOLYLINE"), (8, layer),
            (90, str(len(points))), (70, "1" if closed else "0"),
        ]
        for x, y in points:
            token.extend([(10, _num(x)), (20, _num(y))])
        return token

    def add_lwpolyline(self, points, closed=False, layer="0"):
        self.entities.append(self._lwpolyline(points, closed, layer))
        return self

    @staticmethod
    def _text(value, x, y, height, layer="0", rotation=0.0):
        return [
            (0, "TEXT"), (8, layer),
            (10, _num(x)), (20, _num(y)), (30, _num(0)),
            (40, _num(height)), (1, value), (50, _num(rotation)), (7, "STANDARD"),
        ]

    def add_text(self, value, x, y, height, layer="0", rotation=0.0):
        self.entities.append(self._text(value, x, y, height, layer, rotation))
        return self

    @staticmethod
    def _point(x, y, layer="0"):
        return [(0, "POINT"), (8, layer), (10, _num(x)), (20, _num(y)), (30, _num(0))]

    def add_point(self, x, y, layer="0"):
        self.entities.append(self._point(x, y, layer))
        return self

    # -- blocks ------------------------------------------------------------ #

    def define_block(self, name, base=(0.0, 0.0, 0.0), entities=None, layer="0"):
        """Register a block definition. ``entities`` are the token lists built above."""
        self.blocks.append(
            {
                "name": name,
                "base": tuple(float(v) for v in base),
                "layer": layer,
                "flags": 0,
                "entities": list(entities or []),
            }
        )
        return self

    # -- attributes -------------------------------------------------------- #

    @staticmethod
    def attdef(tag, prompt, default="", height=1.0, x=0.0, y=0.0, layer="0", flags=0):
        """An attribute *definition* (what a block carries)."""
        return [
            (0, "ATTDEF"), (8, layer),
            (10, _num(x)), (20, _num(y)), (30, _num(0)),
            (40, _num(height)),
            (1, default), (3, prompt), (2, tag),
            (70, str(flags)),
        ]

    @staticmethod
    def insert(block_name, x, y, attributes=None, layer="0",
               scale=(1.0, 1.0, 1.0), rotation=0.0, attribute_flags=0):
        """An INSERT, optionally with ATTRIB children and the SEQEND that closes them.

        ``attribute_flags`` is the ATTRIB flag byte (1 = invisible, 4 = constant); the
        template blocks use 1 so the numbering contract never prints.
        """
        attributes = list(attributes or [])
        token = [
            (0, "INSERT"), (8, layer),
            (2, block_name),
            (10, _num(x)), (20, _num(y)), (30, _num(0)),
            (41, _num(scale[0])), (42, _num(scale[1])), (43, _num(scale[2])),
            (50, _num(rotation)),
        ]
        if attributes:
            token.append((66, "1"))
        for tag, value in attributes:
            token.extend(
                [
                    (0, "ATTRIB"), (8, layer),
                    (10, _num(x)), (20, _num(y)), (30, _num(0)),
                    (40, _num(1.0)), (1, value), (2, tag),
                    (70, str(attribute_flags)),
                ]
            )
        if attributes:
            token.extend([(0, "SEQEND"), (8, layer)])
        return token

    # -- serialisation ----------------------------------------------------- #

    def _emit(self, token):
        out = []
        for code, value in token:
            out.append(str(code))
            out.append(str(value))
        return out

    def render(self) -> str:
        lines = []
        # HEADER
        lines += ["0", "SECTION", "2", "HEADER"]
        lines += ["9", "$ACADVER", "1", "AC1009"]
        lines += ["9", "$INSBASE", "10", "0.0", "20", "0.0", "30", "0.0"]
        if self.extents:
            xmin, ymin, xmax, ymax = self.extents
            lines += ["9", "$EXTMIN", "10", _num(xmin), "20", _num(ymin), "30", "0.0"]
            lines += ["9", "$EXTMAX", "10", _num(xmax), "20", _num(ymax), "30", "0.0"]
        lines += ["0", "ENDSEC"]
        # TABLES
        lines += ["0", "SECTION", "2", "TABLES"]
        lines += ["0", "TABLE", "2", "LTYPE", "70", "1"]
        lines += ["0", "LTYPE", "2", "CONTINUOUS", "70", "0",
                  "3", "Solid line", "72", "65", "73", "0", "40", "0.0"]
        lines += ["0", "ENDTAB"]
        lines += ["0", "TABLE", "2", "LAYER", "70", str(len(self.layers))]
        for name, (color, linetype) in self.layers.items():
            lines += ["0", "LAYER", "2", name, "70", "0",
                      "62", str(color), "6", linetype]
        lines += ["0", "ENDTAB"]
        lines += ["0", "ENDSEC"]
        # BLOCKS
        lines += ["0", "SECTION", "2", "BLOCKS"]
        for block in self.blocks:
            lines += ["0", "BLOCK", "8", block["layer"], "2", block["name"],
                      "70", str(block["flags"]),
                      "10", _num(block["base"][0]),
                      "20", _num(block["base"][1]),
                      "30", _num(block["base"][2]),
                      "3", block["name"], "1", ""]
            for token in block["entities"]:
                lines += self._emit(token)
            lines += ["0", "ENDBLK", "8", block["layer"]]
        lines += ["0", "ENDSEC"]
        # ENTITIES
        lines += ["0", "SECTION", "2", "ENTITIES"]
        for token in self.entities:
            lines += self._emit(token)
        for token in self.raw_entities:
            lines += self._emit(token)
        lines += ["0", "ENDSEC"]
        lines += ["0", "EOF"]
        return "\n".join(lines) + "\n"

    def save(self, path):
        with open(path, "w", encoding="ascii", newline="\n") as handle:
            handle.write(self.render())
        return path


# --------------------------------------------------------------------------- #
# reader
# --------------------------------------------------------------------------- #


class Entity:
    """One parsed entity: kind, layer and verbatim group codes."""

    __slots__ = ("kind", "pairs", "attributes", "vertices", "children")

    def __init__(self, kind, pairs):
        self.kind = kind
        self.pairs = list(pairs)
        self.attributes = []   # ATTRIBs following an INSERT
        self.vertices = []     # VERTEX entities following a POLYLINE
        self.children = []     # generic children (unused for now)

    # code lookup: first occurrence wins, matching how DXF semantic codes read
    def s(self, code, default=""):
        for c, v in self.pairs:
            if c == code:
                return v
        return default

    def f(self, code, default=0.0):
        text = self.s(code, "")
        try:
            return float(text)
        except ValueError:
            return default

    def i(self, code, default=0):
        text = self.s(code, "")
        try:
            return int(float(text))
        except ValueError:
            return default

    def has(self, code):
        return any(c == code for c, _ in self.pairs)

    def point(self, code=10):
        return (self.f(code), self.f(code + 10), self.f(code + 20))

    def __repr__(self):  # pragma: no cover - debugging aid
        return f"<Entity {self.kind} layer={self.s(8)!r}>"


class Block:
    __slots__ = ("name", "base", "layer", "entities")

    def __init__(self, name, base, layer):
        self.name = name
        self.base = base
        self.layer = layer
        self.entities = []

    def __repr__(self):  # pragma: no cover
        return f"<Block {self.name!r} {len(self.entities)} entities>"


class Drawing:
    __slots__ = ("version", "layers", "blocks", "entities")

    def __init__(self):
        self.version = ""
        self.layers = []
        self.blocks = {}       # upper-case name -> Block
        self.entities = []     # model space


def _pairs(text):
    """Split raw DXF text into (code, value) pairs."""
    lines = [line.rstrip("\r\n") for line in text.splitlines()]
    out = []
    index = 0
    while index + 1 < len(lines):
        code_text = lines[index].strip()
        if code_text == "":
            index += 1
            continue
        try:
            code = int(code_text)
        except ValueError:
            raise ValueError(f"DXF: line {index + 1}: {code_text!r} is not a group code")
        out.append((code, lines[index + 1]))
        index += 2
    return out


def _split_sections(pairs):
    sections = {}
    index = 0
    while index < len(pairs):
        code, value = pairs[index]
        if code == 0 and value == "SECTION":
            name = pairs[index + 1][1] if index + 1 < len(pairs) else ""
            body = []
            index += 2
            while index < len(pairs) and pairs[index] != (0, "ENDSEC"):
                body.append(pairs[index])
                index += 1
            sections.setdefault(name, []).extend(body)
            index += 1
        else:
            index += 1
    return sections


def _raw_groups(body):
    """Split a flat pair list into (kind, pairs) groups on every code 0."""
    groups = []
    current = None
    for code, value in body:
        if code == 0:
            current = Entity(value, [])
            groups.append(current)
            continue
        if current is not None:
            current.pairs.append((code, value))
    return groups


def _group_entities(body):
    """Turn a flat pair list into Entity objects, folding ATTRIB/VERTEX/SEQEND."""
    groups = _raw_groups(body)
    result = []
    owner = None          # Entity that children attach to
    for entity in groups:
        if entity.kind in ("ATTRIB",):
            if owner is not None and owner.kind == "INSERT":
                owner.attributes.append(entity)
            else:
                result.append(entity)
            continue
        if entity.kind in ("VERTEX",):
            if owner is not None and owner.kind == "POLYLINE":
                owner.vertices.append(entity)
            else:
                result.append(entity)
            continue
        if entity.kind == "SEQEND":
            owner = None
            continue
        result.append(entity)
        owner = entity if entity.kind in ("INSERT", "POLYLINE") else None
    return result


def load(path):
    with open(path, "r", encoding="utf-8", errors="replace") as handle:
        text = handle.read()
    pairs = _pairs(text)
    sections = _split_sections(pairs)
    drawing = Drawing()

    header = sections.get("HEADER", [])
    for index, (code, value) in enumerate(header):
        if code == 9 and value == "$ACADVER" and index + 1 < len(header):
            drawing.version = header[index + 1][1]

    tables = sections.get("TABLES", [])
    in_layer_table = False
    for index, (code, value) in enumerate(tables):
        if code == 0 and value == "TABLE":
            in_layer_table = index + 1 < len(tables) and tables[index + 1][1] == "LAYER"
        elif code == 0 and value == "ENDTAB":
            in_layer_table = False
        elif code == 0 and value == "LAYER" and in_layer_table:
            # the layer record's own code 2 follows
            for offset in range(index + 1, min(index + 12, len(tables))):
                if tables[offset][0] == 2:
                    drawing.layers.append(tables[offset][1])
                    break

    blocks_body = sections.get("BLOCKS", [])
    groups = _raw_groups(blocks_body)
    index = 0
    while index < len(groups):
        group = groups[index]
        if group.kind != "BLOCK":
            index += 1
            continue
        name = group.s(2) or group.s(3)
        block = Block(name, (group.f(10), group.f(20), group.f(30)), group.s(8, "0"))
        index += 1
        body = []
        while index < len(groups) and groups[index].kind != "ENDBLK":
            body.append(groups[index])
            index += 1
        # fold the body through the same child handling as model space
        flat = []
        for child in body:
            flat.extend([(0, child.kind)] + list(child.pairs))
        for child in _group_entities(flat):
            block.entities.append(child)
        drawing.blocks[block.name.upper()] = block
        index += 1  # skip ENDBLK

    drawing.entities = _group_entities(sections.get("ENTITIES", []))
    return drawing


# --------------------------------------------------------------------------- #
# small geometry helpers used by the symbol builder and the checks
# --------------------------------------------------------------------------- #


def arc_points(cx, cy, radius, start_deg, end_deg, steps=12):
    """Sample an arc, for tests that need its extent."""
    import math

    points = []
    for step in range(steps + 1):
        angle = math.radians(start_deg + (end_deg - start_deg) * step / steps)
        points.append((cx + radius * math.cos(angle), cy + radius * math.sin(angle)))
    return points


if __name__ == "__main__":  # tiny self-check: write one file and read it back
    import sys
    import tempfile
    import os

    doc = DxfDocument()
    doc.add_line(-5, 0, 5, 0)
    doc.add_circle(0, 0, 2)
    doc.define_block("DEMO", entities=[DxfDocument._line(-1, -1, 1, 1)])
    target = os.path.join(tempfile.gettempdir(), "dxfio_selfcheck.dxf")
    doc.save(target)
    parsed = load(target)
    assert parsed.version == "AC1009", parsed.version
    assert len(parsed.entities) == 2, len(parsed.entities)
    assert "DEMO" in parsed.blocks, parsed.blocks.keys()
    print("dxfio self-check ok:", target)
    if "--keep" not in sys.argv:
        os.remove(target)

"""Name every NECA 100 symbol from the published standard's own text.

The CAD zip that ships with NECA 100-2024 is geometry only: file names carry the
symbol number (``5-001.dxf``) but no wording.  The standard's PDF (NECA 100-2013,
the edition the numbering comes from) prints one table per group with the number,
the drawing and the description.  This tool extracts that table and joins it to
the converted drawings, so ``make_library.py`` can give every shape a real name.

Output: ``tools/neca_index.json`` keyed by drawing stem (``5-001``)::

    {"5-001": {"token": "5.001", "section": "5.0 Motors-Controls",
               "file_section": "5.0 Motors-Controls",
               "name": "Disconnect switch, unfused type ...", "alt": false}}

Run: ``py -3.13 tools/neca_index.py``

The PDF text is expected at ``target/tmp/neca100.txt`` (``pymupdf`` dump of
``target/tmp/neca100.pdf``).  Regenerate it with::

    py -3.13 -c "import pymupdf,io; d=pymupdf.open('target/tmp/neca100.pdf'); \
        io.open('target/tmp/neca100.txt','w',encoding='utf-8').write( \
        ''.join('\\n===== PAGE %d =====\\n'%(i+1)+p.get_text() for i,p in enumerate(d)))"
"""

import json
import os
import re
import sys

TOOLS = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(TOOLS)
TEXT = os.path.join(ROOT, "target", "tmp", "neca100.txt")
SHAPES = os.path.join(TOOLS, "neca_shapes.json")
OUT = os.path.join(TOOLS, "neca_index.json")

PAGE_RE = re.compile(r"^=+ PAGE \d+ =+$", re.MULTILINE)
FURNITURE = (
    PAGE_RE,
    re.compile(r"^\s*n\s+\d+\s*$"),
    re.compile(r"^\s*\d+\s+n\s*$"),
    re.compile(r"NECA 100\s+Symbols for Electrical Construction Drawings"),
    re.compile(r"^Symbols for Electrical Construction Drawings\s+NECA 100"),
    re.compile(r"^Number\b"),
    re.compile(r"^Preferred Symbol\b"),
    re.compile(r"^Description\b"),
)

# Normalised drawing stem -> the token the PDF prints for it.
MAIN_RE = re.compile(r"^(\d{1,2})-(\d{3})([a-z]?)$")
NFPA_RE = re.compile(r"^(\d+)-(\d+(?:\.\d+)*)$")
ALT = {}


def tokens_for(stem):
    """(token printed in the PDF or None, is_alternate) for one drawing stem."""
    match = MAIN_RE.match(stem)
    if match:
        token = "%s.%s" % (match.group(1), match.group(2))
        return token, bool(match.group(3))
    match = NFPA_RE.match(stem)
    if match:
        return stem, False
    return None, False


def clean(lines):
    """One-paragraph description from the lines printed under a number."""
    kept = []
    for line in lines:
        line = line.replace("\t", " ").strip()
        if not line:
            continue
        if any(pattern.search(line) for pattern in FURNITURE):
            continue
        kept.append(line)
    text = " ".join(kept)
    text = re.sub(r"\s+", " ", text).strip()
    return text


SYMBOL_CODE = re.compile(r"^[A-Z0-9][A-Z0-9./&-]{0,7}$")


def strip_symbol_letters(text):
    """Drop the drawing's own letter codes that follow a description.

    The table prints the description and then, in the symbol column, the letters
    the drawing carries (``NE``, ``EB``, ``UCD``).  Those letters are part of the
    drawing, not of the wording, so they must not end a name.
    """
    parts = text.split()
    while parts and SYMBOL_CODE.match(parts[-1]) and not parts[-1].endswith("."):
        parts.pop()
    return " ".join(parts).strip()


def list_page_groups(body, token_count):
    """Descriptions for a page that prints all numbers, then all wordings.

    Section 1.0 is printed as three columns: a run of numbers (1.001 ... 1.013),
    then one paragraph per number, then the symbol letters.  A description ends
    at a full stop; the paragraphs are therefore split on that and paired with
    the numbers in order.  The letter column marks the end of the wordings.
    """
    lines = []
    for line in body.splitlines():
        line = line.replace("\t", " ").strip()
        if not line or any(pattern.search(line) for pattern in FURNITURE):
            continue
        lines.append(line)
    groups = []
    current = []
    for line in lines:
        if len(groups) == token_count - 1 and SYMBOL_CODE.match(line):
            break  # the symbol column begins: stop taking wording
        current.append(line)
        if line.endswith(".") or line.endswith(":"):
            groups.append(clean(current))
            current = []
    if current and len(groups) < token_count:
        groups.append(clean(current))
    return groups


def token_lines(page_text):
    """(token, start, end) for every ``5.001``-style line on one page."""
    found = []
    for match in re.finditer(r"(?m)^[ \t]*(\d{1,2}\.\d{3})[ \t]*$", page_text):
        found.append((match.group(1), match.start(), match.end()))
    return found


def page_spans(text):
    """[(start, end)] of the body between page markers."""
    starts = []
    for match in PAGE_RE.finditer(text):
        starts.append(text.find("\n", match.end()) + 1)
    spans = []
    for index, start in enumerate(starts):
        end = starts[index + 1] if index + 1 < len(starts) else len(text)
        spans.append((start, end))
    return spans


def parse(text):
    """Return {stem: {token, section, name, alt}} for every drawing we have."""
    # The PDF dump leaves a bell character on some number lines ("1.101\x07\t")
    # and thin spaces inside wordings; neither is part of the Standard's text.
    text = text.replace("\x07", " ").replace("\u2008", " ")
    shapes = json.load(open(SHAPES, encoding="utf-8"))
    stems = sorted({key.split("/", 1)[1] for key in shapes})

    wanted = {}
    for stem in stems:
        token, alt = tokens_for(stem)
        wanted[stem] = (token, alt)

    # Where each token is printed, so a description is the text between it and
    # the next printed token.
    printed = {}
    for stem, (token, alt) in wanted.items():
        if token is None or alt:
            continue
        pattern = re.compile(r"(?m)^\s*" + re.escape(token) + r"\s*$")
        for match in pattern.finditer(text):
            printed.setdefault(token, match)

    ordered = sorted(printed.items(), key=lambda item: item[1].start())
    found = {}
    for index, (token, match) in enumerate(ordered):
        end = ordered[index + 1][1].start() if index + 1 < len(ordered) else len(text)
        found[token] = strip_symbol_letters(clean(text[match.end():end].splitlines()))

    # Pages that print the numbers as a block, then the wordings as a block
    # (section 1.0 is one) cannot be read positionally: pair them in order.
    for start, end in page_spans(text):
        body = text[start:end]
        rows = token_lines(body)
        if len(rows) < 3:
            continue
        gaps = [
            body[rows[i][2]:rows[i + 1][1]] for i in range(len(rows) - 1)
        ]
        if sum(1 for gap in gaps if len(gap.strip()) <= 2) < len(gaps) - 1:
            continue  # numbers are interspersed with wordings: already handled
        wordings = list_page_groups(body[rows[-1][2]:], len(rows))
        for (token, _, _), wording in zip(rows, wordings):
            if wording:
                found[token] = strip_symbol_letters(wording)

    # The group heading in force at each printed token, so an extra symbol can
    # still be filed under the right class.
    sections = []
    section_re = re.compile(r"(?m)^(\d{1,2}\.\d)\s+([^\n]+)$")
    for match in section_re.finditer(text):
        sections.append((match.start(), match.group(1), match.group(2).strip()))

    def section_at(position):
        best = ("", "")
        for start, number, title in sections:
            if start < position:
                best = (number, title)
            else:
                break
        return best

    # The zip's own folder names are the authority on the group.
    file_sections = {}
    for key in shapes:
        folder, _ = key.split("/", 1)
        number = folder.split(" ", 1)[0]
        file_sections[number] = folder

    result = {}
    for stem in stems:
        token, alt = wanted[stem]
        folder = None
        for key in shapes:
            if key.endswith("/" + stem):
                folder = key.split("/", 1)[0]
                break
        number = folder.split(" ", 1)[0] if folder else ""
        name = find_name(found, token) if token else ""
        if alt and not name:
            base = stem[:-1]
            base_token, _ = tokens_for(base)
            base_name = find_name(found, base_token) if base_token else ""
            name = (base_name + " (alternate drawing)") if base_name else "Alternate drawing"
        if not name:
            name = stem
        result[stem] = {
            "token": token or stem,
            "section": number,
            "file_section": folder or "",
            "name": name,
            "alt": alt,
        }
    return result


def find_name(found, token):
    """Description printed for one token, tolerating a number split by the PDF."""
    if not token:
        return ""
    if token in found:
        return found[token]
    # Some annex numbers print as e.g. "5-7.1.1.1" while the drawing stem is the
    # same, so the exact token is the only spelling we need.
    return ""


def main():
    if not os.path.exists(TEXT):
        print("missing %s - dump the PDF text first (see module docstring)" % TEXT)
        return 1
    with open(TEXT, encoding="utf-8") as handle:
        text = handle.read()
    index = parse(text)
    with open(OUT, "w", encoding="utf-8") as handle:
        json.dump(index, handle, indent=1, sort_keys=True)

    named = [entry for entry in index.values() if entry["name"] != entry["token"]]
    print("wrote %d entries to %s" % (len(index), OUT))
    print("named %d, unnamed %d" % (len(named), len(index) - len(named)))
    for section in sorted({entry["section"] for entry in index.values()}):
        rows = [entry for entry in index.values() if entry["section"] == section]
        blanks = [entry["token"] for entry in rows if entry["name"] == entry["token"]]
        print("  %-4s %3d entries, %3d unnamed %s"
              % (section, len(rows), len(blanks), ("(" + ", ".join(blanks[:6]) + ")") if blanks else ""))
    return 0


if __name__ == "__main__":
    sys.exit(main())

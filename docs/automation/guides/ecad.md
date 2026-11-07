# Electrical drawings (ECAD)

AryaCADElectrical is the same editor as Open CAD Studio with an electrical layer on top:
sheet templates with AutoCAD Electrical style title blocks, a project file, symbol libraries,
pin-level wire connectivity, cables, panel reports and a few hundred `ELEC*` commands. This
guide is the order to do things in and the checks that tell you a drawing is right.

## 1. Start from a drawing set, not a blank drawing

The GUI opens a chooser: **Drafting drawing** or **Electrical drawing set**. The electrical
path is a setup panel that builds a complete project:

| field | meaning |
| --- | --- |
| Standard | `IEC` (ISO 5457 frame, first-angle block) or `NEC` (ASME Y14.1 frame, third-angle block) |
| Sheet size | A4–A0 and B1 for IEC; ANSI A–E and Legal for NEC |
| Project / Title / Company | written into the title block and `project.eprj` |
| Drawing number | file stem: sheets are `<drawing>-01.dxf`, `<drawing>-02.dxf`, … |
| Sheets in the set | 1–200; every sheet is generated from the template |
| Folder | picked with **Browse…**; if Windows protects the folder the set falls back to the per-user AryaCAD folder, then temp, then the current directory and says where it landed |

`ELECSTART` reopens that chooser at any time, or `ELECNEW <template.dxf>` starts a single
electrical drawing from a template when a script already knows the path.

Bundled templates: `templates/iec/*.dxf` and `templates/nec/*.dxf` next to the executable
(the portable folder carries them too). The title block is real geometry plus TEXT
placeholders — `COMPANY NAME`, `DRAWING TITLE`, `AR-0001`, `1 / 1`, `PROJECT`, `DRAWN` — and
the drawing-set builder fills those in.

## 2. Set up the drawing

`ELECSETUP` reads the `WD_M` block from the template. It defines the tag format, wire number
format and layer names, and it must run before `ELECINSERT`/numbering commands so tags and
wire numbers follow the project convention. `ELECLIB` lists or loads the symbol library.

## 3. Draw

- `ELECINSERT <symbol>` inserts a library symbol, asks for the insertion point, fills its
  attributes and tags it (`-K1`, `-K2`, …). The tag defaults come from the library entry.
- Symbols may carry pin attributes (`X1TERM01` …), which is what makes the netlist name a
  device pin (`-K1:2`) instead of an anonymous device. Symbols without declared pins still
  anchor at their wire stubs, so both AutoCAD Electrical blocks and plain library symbols work.
- `ELECWIRE` / the drafting line command draws on the `WIRES` layer. Wires on layer `0`
  count too, but only if they are at least 0.5 units long — the flattened customer prints
  store wires that way, and symbol strokes shorter than that are ignored.
- `ELECJUNCTION` and `DOT`/`WDDOT` inserts join wires that cross: a T junction only joins
  when there is a dot on it, which is what the drawing convention says.
- `ELECCABLEDRAW <tag> [CORES=] [PART=] [MFG=] [SHIELD=yes]` draws a cable as real
  schematic objects: LINE segments on the `CABLES` layer, the tag as TEXT and a shield arc.

## 4. Read the drawing back

| command | result |
| --- | --- |
| `ELECWIRELIST` | every wire segment, its nets and pins |
| `ELECWIRENUMBER` | assigns wire numbers from the wire graph, skipping unconnected ends |
| `ELECLIST` | components on the sheet |
| `ELECTERMINALS` | terminal blocks and their connections |
| `ELECBOM [out.csv] [OPTION=<code>]` | bill of material; the option filter keeps only devices assigned to that variant option |
| `ELECCABLELIST [out.csv]` | cable schedule with measured route length, shield, ends resolved to device pins |
| `ELECREPORT` | multi-sheet report workbook: wires, BOM, terminals, cables |
| `ELECPANEL` | panel layout report |
| `ELECZONE` | the sheet's zone letter/number for cross references |

The connectivity all comes from one wire graph (`crates/elec`), so these commands agree.
Net names follow AutoCAD Electrical conventions: connected pins share a net, a device with
declared pins anchors at each pin, and a wire that reaches no device is reported as loose
rather than silently dropped.

## 5. Projects, variants and systems

- `ELECPROJ` opens or lists the project file `project.eprj` (one `sheet <file>` line per
  sheet, in drawing order).
- `ELECVARIANT LIST|NEW <name>|USE <name>` keeps a copy of the project beside the master
  (`variant-<name>.eprj`) so the same design can be issued with option differences.
- `ELECOPTION <tag> <option>` assigns a device tag to an option code; `ELECBOM
  OPTION=<code>` filters the BOM and reports `option X selects N of M` or
  `no components assigned`.
- `ELECSYSTEM LIST|INSERT <name> [x,y]|SAVE <name>` inserts a reusable sub-circuit: a saved
  starter (devices + wires) is placed at a point and its symbols are retagged in sequence.
  `ELECSYSTEM SAVE` captures the current drawing's devices and wires relative to the
  lower-left device: the usual way to grow the library from a working circuit.

## 6. Use it from an MCP client

Start the editor with `--automation <drawing>` so the bridge is on from the first frame
(the same bridge the MCP chip toggles), then run the client against
`OpenCADStudio.exe --mcp`. Read the `cad://guides/ecad` resource (or `ocs_read` with
`op: guide`, `parameters.name: ecad`). For the data:

- `ocs_read` with `op: electrical` → `wire_segments`, `wire_length`, `nets` (name, pins,
  wire_count), `pins` (label, device, pin, x, y), `loose_pins`, `components` (tag, class, part,
  manufacturer, description). Add `parameters.detail: summary` for counts only.
- `ocs_execute` with `op: run`, `parameters.cmd` runs any `ELEC*` command and returns its
  command-line output (`ELECWIRELIST`, `ELECBOM`, `ELECREPORT`, …).
- Node graph: category **Electrical** has `Electrical Netlist`, `Electrical Wire Length`,
  `Electrical Components` and `Net At Point`, reading the same wire graph so a graph can
  report a sheet without a command round trip.

The automation bridge, the node graph and the drawing commands share the wire graph: if
`ocs_read electrical` and `ELECWIRELIST` disagree about a net, that is a bug, not a
configuration difference.

## 7. Verify a sheet

1. `ELECSETUP` ran and reports the `WD_M` settings without errors.
2. `ELECWIRELIST` nets make sense: two pins per net for a contact, three for a junction; no
   `?` device labels; loose pins are the ones you intended to leave open.
3. `ELECBOM` row count matches the symbols on the sheet and the class/part columns are
   filled. With an option filter, the count is `N of M`.
4. `ELECCABLELIST` resolves both ends (`-K1:2 – -K2:1`) and the length matches the route.
5. `ELECREPORT` opens with all sheets present.
6. Save through `save_verified` to an absolute path and check the extension/format.

## Layout conventions this suite assumes

- Sheet frame: IEC uses a 20 mm binding edge on the left and 10 mm elsewhere; NEC uses a
  25.4 mm binding edge and 12.7 mm elsewhere. Zone numbers run along the top and bottom,
  letters up both sides.
- Title block: bottom-right, 180 mm nominal width; it scales down on narrow sheets (ANSI A
  portrait) instead of running off the frame.
- Electrical layers created by `ELECNEW`: `WIRES` (green, 3), `JUNCTIONS` (green, 3),
  `WIRENO` (yellow, 2), `XREF` (magenta, 6), `ELEC-SYMBOLS` (white, 7), `CABLES`.

## Screenshots in the repository

- `docs\screenshots\startup-chooser.png` — the first launch: drafting or electrical.
- `docs\screenshots\template-iec-a3.png`, `docs\screenshots\template-nec-b.png` — the
  finished templates rendered by the app.
- `docs\screenshots\node-graph-electrical.png` — the node graph's Electrical category with a
  live `Electrical Netlist` node evaluated on the sample sheet.

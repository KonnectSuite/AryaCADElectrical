# SkyCAD Electrical feature-coverage matrix — AryaCADElectrical

**Task:** `task-8` · **Author:** ecad-conventions · **Date:** 2026-11-06 (~01:30 local)
**Scope:** the 19 published SkyCAD Electrical features, each judged against the *code as it is now* and the *binary as it is now*. Companion documents: [PROGRESS.md](PROGRESS.md), [STATUS.md](STATUS.md), [ecad-conventions.md](ecad-conventions.md), [skycad-model.md](skycad-model.md), [real-sheet-model.md](real-sheet-model.md).

---

## 0. How this was measured, and the moving target

* **Binary under test:** `target\release\OpenCADStudio.exe`, 158,157,312 bytes, mtime **2026-11-06 01:26:41**, `--version` → `OpenCADStudio 2026.44+gfa453e30.dirty`. A **cargo/rustc build was running** throughout this analysis, and the two in-progress modules were written around the moment the binary was linked — the tested binary demonstrably does **not** contain them.
* **Commands present in that binary:** the whole `ELEC*` set including `ELECSETUP` and **`ELECREV`** (it ran). **`ELECPARTS`, `ELECPARTLIST`, `ELECASSIGN`, `ELECIOIMPORT` are NOT in it** — observed `✕ INVALID: Unknown command` for each. Their source exists and compiles into the *next* build (`elec_parts_lib.rs` 36,869 B @ 01:26:22; `elec_ioimport.rs` 29,617 B @ 01:26:35), so they are marked **partial (in progress)**, not missing.
* **Headless method:** copied binary into `%TEMP%` (never locking `target\`), ran `probe.exe --each <batch> <drawing>` on **copies** of the customer sheets in `%TEMP%`, and read results back from the saved DXF with `tools/dxfio.py`. No customer file was modified.
* **Source evidence** is `file:line` against the tree at the timestamps above. `elec.rs` is **5,049 lines @ 01:27:11** and was being edited during this analysis, so individual line numbers may drift by a few lines — **the function names in the same citation are the stable anchor**. 38 `ELEC*` ids are registered ([`src/app/commands/mod.rs`](../src/app/commands/mod.rs) registry block), 31 of them reachable from the ribbon ([`src/modules/electrical/mod.rs`](../src/modules/electrical/mod.rs): 17 `elec_tool(` + 16 `menu_item(`, 29 SVG icons in `assets/icons/elec_*.svg`).
* **Status legend:** *implemented* = works end-to-end headless, verified this session; *partial* = works for a subset, or works only with caveats; *missing* = no working code path; *NA* = not a CAD feature.
* **Honesty rule applied:** a feature is not "implemented" because a command exists. It must produce the right result on a drawing the user actually has. All observed outputs quoted below are real.

### Status counts (19 features)

| Status | Count | Features |
|---|---|---|
| **implemented** | **1** | 10 (print to PDF) |
| **partial** | **11** | 1, 2, 3, 5, 6, 8, 9, 11, 12, 13 (in progress), 16 |
| **missing** | **6** | 7, 14, 15, 17, 18, 19 |
| **NA** | **1** | 4 (license sharing) |

---

## 1. The matrix

Legend for *feasibility*: **high** = pure data/algorithm work in `crates/elec` + `elec.rs`; **medium** = needs new geometry/UI or careful DXF semantics; **low** = needs an engine the codebase does not have (3D lengths, multi-user service).

| ID | Feature (SkyCAD) | What it means for a user (≤2 sentences) | OUR STATUS | EVIDENCE (file:line / observed) | What is missing concretely | Feasibility | Implementation sketch |
|---|---|---|---|---|---|---|---|
| **1** | Automated numbering (wires, components, panels, sheets, projects; sequential; customisable) | Devices and wires get one consistent designation per drawing or across a project, and a released drawing must not be silently renumbered. | **partial** | `elec_number` [elec.rs:736](../src/app/commands/elec.rs), `elec_wirenumber` [:1081](../src/app/commands/elec.rs), `elec_proj_number` [:4093](../src/app/commands/elec.rs); formats `NumberingFormat::tag/value` [crates/elec/src/numbering.rs:107-178](../crates/elec/src/numbering.rs); WD_M read [:228-265](../src/app/commands/elec.rs); freeze [:864-880](../src/app/commands/elec.rs). Observed on real sheet 003: `ELECNUMBER: 4 component(s) in 1 class(es); would write 0 tag(s)` (freeze works). Observed on app-drawn sheet: `ELECNUMBER: no electrical components found.` Observed: two consecutive `ELECINSERT` both `-K1`. | Only `%N`/`%F` substitute — `%S`, `%X`, `%I`, `%L`, `%D` are emitted literally; numbering ignores TEXT-recovered devices (10 found by `ELECLIST`, 4 by `ELECNUMBER`); ignores app-created inserts (owner predicate); `ELECPROJNUM` ignores `WD_M` entirely (no freeze, no `TAG-START`) and rewrites every sheet; no sheet/panel/wire-reference numbering; no "next free tag" on insert. | **high** — the format engine and `renumber()` already exist; the collectors need unifying. | Make one `collect_electrical(doc)` used by all commands (accept null owner, paper space, TEXT-recovered tags); add `%S/%X/%I/%L/%D` to `apply_pattern`; feed `Numbered{current,frozen}` from it into `renumber`; add `WIREMODE=R` freeze to `ELECWIRENUMBER`; make `ELECPROJNUM` read each sheet's `WD_M` and skip frozen tags; `ELECINSERT` asks `next_tag(existing)` before placing. |
| **2** | BOMs and parts-lists management (parts/BOM/cabling/wiring lists, real-time, one-click Excel) | A BOM, parts list, wire list and terminal list come out of the drawing, with manufacturer/description filled from a parts catalogue. | **partial** (parts catalogue in progress) | `elec_bom` [elec.rs:639](../src/app/commands/elec.rs), `elec_list` [:704](../src/app/commands/elec.rs), `write_bom_csv` [:4833](../src/app/commands/elec.rs); `elec_report` XLSX 3 sheets [:1683](../src/app/commands/elec.rs) via `write_xlsx` [:4892](../src/app/commands/elec.rs); `Catalogue::from_csv/enrich` [crates/elec/src/catalogue.rs:58,119](../crates/elec/src/catalogue.rs); `library/parts/parts.tsv` (4,640 B) exists. Observed on 002: `ELECBOM: 13 component(s) -> 4 bill-of-material row(s)`, CSV written; app-drawn sheet: 7 components → 6 rows. **Verified this session:** `ELECPART cat.csv` → `ELECBOM` prints `1 component(s) completed from the parts catalogue` and the CSV row gains `Siemens` / `Catalogue contactor 9A`, with header `Class name,Part number,Manufacturer,Description,Qty,Where used` — exactly the SkyCAD BOM template columns. `ELECPARTS/ELECPARTLIST/ELECASSIGN` source written ([elec_parts_lib.rs:435,461,648](../src/app/commands/elec_parts_lib.rs)) but not in the tested binary. | No cabling list, no cross-reference list export, no I/O list export; lists are on-demand, not "real-time" (no recompute on edit); parts records carry only part/manufacturer/description (no order code, stock, terminals); no per-drawing "one-click Excel" from the UI (command only). | **high** | Extend `PartRecord` with `class/rating/terminals/order_code`; add `ELECCABLELIST`, `ELECXREFLIST`, `ELECIOLIST` reusing `Sheet` from [crates/elec/src/report.rs:34](../crates/elec/src/report.rs); make `ELECREPORT` write 6 sheets; add all four to the Reports ribbon group. |
| **3** | Automated cross-references (computed when symbols are linked, real-time) | A contact shows where the other parts of its device are, and the coil shows its contacts, without manual typing. | **partial** | `elec_xref` [elec.rs:1415](../src/app/commands/elec.rs), `elec_xref_write` [:1587](../src/app/commands/elec.rs); grouping/contact list [crates/elec/src/xref.rs:73,89,122](../crates/elec/src/xref.rs). Observed on 003: `ELECXREF: 2 device(s); 1 with more than one part.` → `M326 (3 part(s)): NO` with x/y of each part. | **Same drawing only** — no sheet, no zone, no `ALT_XREFFMT`; label text is positions or a single part-id, not the customer's `13-14, 21-22` pin pairs (their `PIN` attribute reads `NO`); no recompute when a symbol moves; no parent/child link stored (grouping is by tag string each run); not "real-time". | **medium** — needs a sheet/zone reference service and stable device identity. | Store `parent_tag` + ordered part list on the drawing/project model; add `zone_of(x,y)` from frame geometry (`DATUMX/DISTH/CHAR_H` equivalent); format `{sheet}-{zone}{line}` for remote parts; update labels on every move via a scene-change hook, not a command. |
| **4** | License sharing (multi-user licensing) | Several users share seats. | **NA** | SkyCAD page feature; no code, and none should exist in a local-first CAD. | Nothing — out of scope by design (§3). | — | Do not build. If ever needed, a file-lock/lease service is a separate product, not a CAD command. |
| **5** | No training required (UX, discoverability, tutorials) | A new user can find the electrical tools and learn them from the app. | **partial** | Ribbon: 7 groups / 17 large tools + 16 menu items / 29 icons ([src/modules/electrical/mod.rs](../src/modules/electrical/mod.rs)); searchable palette (PROGRESS round 2); usage line on every bare command (e.g. [elec.rs:3280](../src/app/commands/elec.rs)). Observed: every ribbon command runs (no dead buttons). | The five newest commands (`ELECREV`, `ELECPARTS`, `ELECPARTLIST`, `ELECASSIGN`, `ELECIOIMPORT`) have **no ribbon entry**; no in-app help/tutorial content, no tooltips beyond the label, no sample project shipped; command grammar is discoverable only by typing a bare command. | **high** | Add one ribbon group ("Parts & Revisions"), tooltips from the usage strings, a `HELP ELEC` topic generated from the dispatcher's own usage lines, and a `samples/` project. |
| **6** | PLCs and drives automatically generated (enter pinout, symbol generated, consistent library) | Type a module's point count and base address and get a PLC module symbol with every pin labelled and addressable. | **partial** | `elec_plc` [elec.rs:3269](../src/app/commands/elec.rs) builds a `PLC-<n>` block with `TAG1/DESC1/ADDRnn/X1TERMnn`; `plc_addresses` [:3385](../src/app/commands/elec.rs) (8 bits/byte, rollover); `ELECPLCNEXT` [:3607](../src/app/commands/elec.rs) continues across a project; `plc_parse_address` [:3687](../src/app/commands/elec.rs). Observed: `ELECPLC -P1 8 I0.0` → `I0.0..I0.7`; module then appears in `ELECLIST`/`ELECBOM` as class `PLC-8`. | No pinout/pin-table input yet (that is feature 13's importer, in progress); no drive symbols; module geometry is a generic rectangle, not a per-catalogue symbol; point descriptions can only be edited with `ELECSETATTR`, one by one; no "consistent library" linkage (module class is the block name). | **high** | Let `ELECPLC` take a module *part* from the local catalogue and instantiate its symbol + point count; add `ELECPLCSET <tag> <point> <desc>`; class comes from `parts.tsv` not the block name. |
| **7** | Navigation tools (off-page refs, one-click jump, highlight all symbols of a component) | Click a cross-reference and land on the other contact; highlight every symbol of one device. | **missing** | No navigation command in the electrical layer — the only zoom primitives are the host's `ZOOM OBJECT`/`ZOOM` ([src/app/commands/mod.rs](../src/app/commands/mod.rs)); `ELECXREFWRITE` creates plain `TEXT` on layer `XREF` ([elec.rs:1666-1672](../src/app/commands/elec.rs)) with no handle/link back to the symbol. | Everything: no hit-testable xref entity, no jump, no "select all of tag", no off-page reference block. | **medium** — needs per-entity metadata (XDATA/extension dictionary) or a side table mapping label→handle. | Write xref labels as `TEXT` with an extension-dictionary/XData payload `{sheet, handle, tag}`; add `ELECGOTO <tag>` (fits the view to the first part) and `ELECHIGHLIGHT <tag>` (select all inserts whose TAG attribute equals it), then bind them to double-click on the label. |
| **8** | Terminal strip management (number/sort/organise; automatic strip layout; stacked terminals) | Terminals are grouped into named strips, numbered, and laid out as a strip drawing. | **partial** | `elec_terminals` [elec.rs:1335](../src/app/commands/elec.rs); `parse_terminal_tag` [crates/elec/src/terminal.rs:21](../crates/elec/src/terminal.rs); `terminal_strips` [:65](../crates/elec/src/terminal.rs); `ELECTSIDES` [elec.rs:2228](../src/app/commands/elec.rs) + [crates/elec/src/sides.rs:61](../crates/elec/src/sides.rs). Observed on 003: `ELECTERMINALS: no terminals found (no component tagged strip:index)`. Observed on an app-drawn sheet with `TAG1=X1:1` set by `ELECSETATTR`: **still** `ELECTERMINALS: no terminals found` — because `elec_collect` [elec.rs:1267, strict owner at :1285](../src/app/commands/elec.rs) cannot see app-created inserts, so the crate's tag parser is never reached. | Requires the **`strip:index` tag convention** the customer's drawings do not use (their terminals are `TERMINAL` blocks tagged `M326` etc. with no strip text); no strip numbering/ordering UI; no automatic strip layout drawing; no stacked/3-level terminals; no spares. | **medium** | Derive strip+index from the terminal block's attributes (`TAG1`/`TERM01`/`XREF`) with a configurable pattern; add `ELECSTRIP <tag> <strip> <index> [<stack>]` to assign; generate a strip layout drawing (reuse `ELECPANEL`'s sheet-writing path); model stacks as `TerminalRef {strip,index,stack}`. |
| **9** | Revision control and rollback (lock a version; consult; compare) | Snapshot the project, list versions, see what changed, and go back. | **partial** | `elec_revision.rs`: dispatch [:25](../src/app/commands/elec_revision.rs), snapshot [:159](../src/app/commands/elec_revision.rs), list [:239](../src/app/commands/elec_revision.rs), compare [:266](../src/app/commands/elec_revision.rs), restore [:323](../src/app/commands/elec_revision.rs); wired at [elec.rs:4569](../src/app/commands/elec.rs), registry + early-args list. **Observed working**: `ELECREV SNAPSHOT round-three` → `snapshot 001-round-three — 3 file(s), 557.0 KB`; `.rev/001-round-three/{A002.dxf,A003.dxf,project.eprj,manifest.tsv}` created. **Observed defects**: after `ELECPROJNEW` → `ELECREV: no project is loaded`; `COMPARE 001 002` → "no snapshot 001" (ids are `001-label`, numeric shorthand only without leading zeros); `COMPARE 1 2` → `no differences (same file set and sizes)` (size-only compare); `RESTORE 1` → `cannot restore A002.dxf: … being used by another process (os error 32)` **yet** prints `restored 2 file(s)` — partial restore reported as success. | Wrong project state wired (`project_dir_for_revisions` reads `project_dir`, set only by `ELECPROJECT` [:3080](../src/app/commands/elec.rs), while `ELECPROJNEW/OPEN` set `project_file` [:3909,3937](../src/app/commands/elec.rs)); compare hashes sizes only, not content; restore cannot restore the open drawing and does not fail closed; no "locked/built" flag per revision; no UI. | **high** — everything is filesystem work already written; four small fixes. | Point `revision_root()` at `project_file()` (fall back to `project_dir`); store a SHA-256 per file in `manifest.tsv` and diff hashes; in restore, close/backup open tabs first (or refuse the whole restore when any target is open); accept zero-padded ids; add a Revisions ribbon group. |
| **10** | Print to PDF | Plot the drawing to a PDF a customer can open. | **implemented** | `EXPORTPDF` → `Message::PlotExport` [display.rs:1017](../src/app/commands/display.rs); `export_pdf`/`export_pdf_pages` [src/io/pdf_export.rs:55,60](../src/io/pdf_export.rs) with searchable text runs (`SearchableTextRun` imported at [:14](../src/io/pdf_export.rs)); multi-sheet printing already exists: `print_all_pages` [update/file.rs:5661](../src/app/update/file.rs). | Clickable cross-references in the PDF (the "intelligent PDF" half of feature 16), and no ELEC-specific "publish all project sheets" command — but the host print path is real and multi-page. | **high** (for PDF links see #16) | Bind an `EXPORTPROJECTPDF` that loops the project's sheets through `print_all_pages`; add PDF link annotations once xref labels carry target handles (#7). |
| **11** | Panel layout design (size the panel; layouts; cut-outs) | Produce a panel/enclosure drawing showing where each device sits, to scale, with cut-outs. | **partial** (skeleton) | `elec_panel` [elec.rs:3705](../src/app/commands/elec.rs): fixed `14×10` boxes on a `sqrt(n)` grid, alphabetical, written to a **new** DXF. Observed: `ELECPANEL: 6 device(s) in 3x2 cells (80x60); wrote …panel_smoke.dxf`; earlier `1 device(s) in 1x1 cells (40x40)`. | No footprints (no panel symbol library), no enclosure sizing from parts, no cut-out/door/rail geometry, no DIN rail (IEC 60715), no mount/group assignment, alphabetical rather than functional arrangement, output is a loose DXF not part of the project, no panel BOM. | **medium** — the drawing writer exists; the missing input is footprint geometry + placement rules. | Give `library/parts/parts.tsv` footprint references (`footprint`, `w×h×d`); `ELECPANEL` places footprint blocks on a rail grid with `MOUNT`/`GROUP` attributes, sizes the enclosure from the union of footprints, emits cut-out rectangles, and registers the panel drawing in `.eprj`. |
| **12** | Wire processing (colours/gauges/part numbers; pin-to-pin wire lists; wire label lists) | Every conductor carries its type and gauge, and the wire list tells the shop what to pull between which pins. | **partial** | `ELECWIRE` [elec.rs:2515](../src/app/commands/elec.rs) → `WireCommand` draws real `LINE`s on `WIRES` (verified in saved DXF: `LINE layer='WIRES' owner='1B'`, 2 segments from 3 points); `ELECWIRELIST` [:946](../src/app/commands/elec.rs) builds nets via [crates/elec/src/geometry.rs:130](../crates/elec/src/geometry.rs) with tolerance `0.05` [:950](../src/app/commands/elec.rs); `ELECWIRENUMBER` [:1081](../src/app/commands/elec.rs) reads `WIREFMT`/`WIRE-START`. **Observed failures**: same session after drawing wires → `ELECWIRELIST: no wires found (LINE or LWPOLYLINE entities)`; after save+reopen → same; real sheet 003 → `132 wire segment(s), 4 component(s) -> 0 net(s) … 132 loose wire(s)`. | Nets are built from **symbol insertion points**, not pins, so connectivity is effectively always 0; the strict model-space predicate hides newly drawn wires; no colour/gauge/part-number fields (only layer name); no pin-to-pin list; no wire-label export; only `LINE`+`LWPOLYLINE` recognised (no `POLYLINE`, no wire hops); no crossing-vs-T disambiguation beyond geometry. | **high** — the netlist algorithm and list writers exist; the anchor source is the bug. | Anchors from pin geometry (`TERM01..`, block `ATTDEF` points, `X1TERMnn` like `ELECPLC` already writes); accept null owner + paper space in all 11 collectors; store `wire_type/gauge/colour/part` as attributes on the wire or a project table; `ELECWIRELIST` emits `Number, Net, From pin, To pin, Type, Gauge, Length`; `ELECWIRELABELS <csv>` exports label text. |
| **13** | Automated PLC drawings from an I/O list (Excel import; generate drawings; edit I/O comments) | Import the controls engineer's I/O spreadsheet and get PLC drawings with every point addressed and described. | **partial (in progress this round)** | Source written: `elec_ioimport.rs` — command `ELECIOIMPORT <file.xlsx> [x,y]` [:123](../src/app/commands/elec_ioimport.rs), dispatch [:105](../src/app/commands/elec_ioimport.rs), header scan 25 rows and `MAX_ROWS` 20,000 [:24-26](../src/app/commands/elec_ioimport.rs), groups points per card, reuses `PLC-<n>` + `ADDRnn`/`X1TERMnn` attributes [:8-16](../src/app/commands/elec_ioimport.rs); uses `calamine` for xlsx. **Not in the tested binary** — `✕ INVALID: Unknown command: ELECIOIMPORT` observed. | Not yet compiled into a shipped binary; no ribbon entry; no post-import comment editing other than `ELECSETATTR`; no terminal/cable wiring of imported points; no round-trip export of the I/O list back to Excel. | **high** (code already written) | Rebuild, add to a "PLC" ribbon group, verify on a real `.xlsx`; then `ELECIOSET <tag> <point> <description>` for comment edits and `ELECIO` export to sheet `I/O list`. |
| **14** | Cabling and cable schedules (add cables; shield connections; cable schedules) | Draw a multi-core cable between two devices, show its shield, and print a cable schedule. | **missing** | No cable entity or table anywhere in the electrical layer: `cable` appears only as a reference-letter mapping (`'W' => "Cable"` [elec.rs:563](../src/app/commands/elec.rs)) and a numbering-prefix hint ([crates/elec/src/numbering.rs:290](../crates/elec/src/numbering.rs)); the ACADE `%C/%E/%J` cable parameters are not implemented. A cable cannot be created, so no schedule. | A cable object (tag, part, cores, from/to, shield), core entities linked to wires/nets, shield drawing, cable schedule report, cable list in `ELECREPORT`. | **medium** — new domain model, but everything downstream (report sheets, netlist, catalogue) already exists to hang it on. | New `crates/elec/src/cable.rs` (`Cable{tag, part, cores: Vec<Core{id, colour, gauge, net}>}`); `ELECCABLE <tag> <part> <from> <to> <n-cores>`; wires reference a core; `ELECCABLELIST` → schedule columns `Tag, Part, Manufacturer, Cores, From, To, Length, Shield`; draw shield as an arc bracket on layer `CABLES`. |
| **15** | Built project modification control (flag as built; only new items renumbered; deleted terminals become spares) | Once a project is built, editing it must only add numbers for genuinely new devices and free up removed terminals as spares. | **missing** | No "built" state exists. The nearest analogue is the `WD_M` freeze: `TAGMODE/WIREMODE = R` makes `ELECNUMBER` freeze existing tags [elec.rs:864-880](../src/app/commands/elec.rs) — but `ELECPROJNUM` ignores it, and `ELECWIRENUMBER` has no freeze path; there is no spare-terminal logic anywhere. | Built/locked project flag persisted in `.eprj`; per-device frozen flag; "next number = max used + 1" instead of a full renumber; deleted terminal kept as a spare record; audit trail of changes. | **high** | Add `built = true` + `next_number` map to `.eprj`; on build, snapshot via `ELECREV` and record the numbering state; `ELECNUMBER`/`ELECPROJNUM` skip frozen and start from the high-water mark; `ELECTERMINALS --spares` lists removed terminals. |
| **16** | Export to DWG/DXF and intelligent PDF (all sheets; searchable PDF with clickable cross-references) | Hand the customer DWG/DXF sheets and a PDF whose cross-references can be clicked. | **partial** | DWG/DXF read+write is real at the codec layer: `DwgReader`/`DwgWriter`/`DxfWriter` [src/io/mod.rs:71-74](../src/io/mod.rs), default save format DWG (header comment [:4](../src/io/mod.rs)), `--export IN OUT` [main.rs:158](../src/main.rs); PDF multi-page with searchable text (#10). | No single "export all project sheets to DWG/DXF" command (scriptable today only via one `--export`/open-save per file); PDF text is searchable but has **no clickable cross-reference links** and no project-wide publish; no "intelligent PDF" metadata beyond text. | **high** | `ELECEXPORT <folder> [dxf|dwg]` loops `project_sheet_list()` through `io::save`; add link annotations in `pdf_export.rs` keyed by the xref label payload from #7; `ELECPUBLISH <file.pdf>` prints the whole project. |
| **17** | Harness design (wire harness diagrams; automatic wire/cable lengths; harness BOMs) | Design a physical harness with lengths and a harness BOM. | **missing** | No harness code (`harness` absent from `elec.rs`, `crates/elec`, `src/modules/electrical`). | Everything; also needs route/length data that does not exist in 2D schematic DXFs. | **low** — lengths require either 3D routing or a manual length field; the codebase has neither. | Defer; if demanded, approximate with a per-wire `LENGTH` attribute entered in the wire list, and a harness drawing that reuses cables from #14. |
| **18** | Configuration management (reuse previous designs in one click; options/variants tree) | Keep variants of a machine and reuse a previous design instead of redrawing it. | **missing** | No variant/option/sub-assembly code: project = ordered sheet list in a plain-text `.eprj` ([elec.rs:4034-4085](../src/app/commands/elec.rs)); no include/instantiate mechanism. | Variant tree, per-variant sheet/option sets, one-click copy of a prior project's sheets, option codes on components (the ACADE `POSLIST` idea). | **medium** | Extend `.eprj` with `variant <name>` blocks listing sheets and excluded tags; `ELECVARIANT NEW <name> COPY <base>` clones the sheet list; `ELECOPTION <tag> <option>` marks optional devices; reports filter by variant. |
| **19** | Design by systems/sub-assemblies (design systems, store in catalogue, reuse in projects) | Build a subsystem once (e.g. a motor starter) and drop it into future projects. | **missing** | No sub-assembly/instantiate code; `library/` holds symbols and parts only (`library/library.tsv`, `library/parts/parts.tsv`), not assemblies; project scanning counts components ([`elec_project` elec.rs:3029](../src/app/commands/elec.rs)) but cannot insert a block of devices. | A system definition (sheet fragment + device list), a catalogue of systems, instantiate-with-retag, and propagation to BOM. | **medium** | Store systems as a `.dxf` fragment + manifest in `library/systems/`; `ELECSYSTEM INSERT <name> <sheet> [x,y]` copies the fragment and re-tags via the numbering engine; record in the project for BOM. |

---

## 2. Defects measured this round (must be fixed before any parity claim)

These are not "missing features" — they are reasons existing features do not deliver on drawings a user makes.

| # | Defect | Evidence | Impact |
|---|---|---|---|
| **D1** | **Collector divergence.** 11 sites compare `owner_handle == model_space` while `in_drawing_space()` ([elec.rs:487-496](../src/app/commands/elec.rs)) accepts null owners and paper space. | `grep '!= model_space'` → elec.rs:753, 962, 1098, 1285, 1423, 1547, 1606, 1778, 4144, 4357; elec_ioimport.rs:709. Observed: `ELECWIRELIST: no wires found` immediately after `ELECWIRE` drew two `LINE`s on `WIRES` (verified in the saved DXF), and still after save+reopen. | Every net/terminal/xref/numbering command is blind to the app's own geometry — features 1, 3, 8, 12 and the xlsx half of 2. |
| **D2** | `ELECINSERT` does not advance the next tag. | Observed: `ELECINSERT CONT-NO` → `-K1`, then `ELECINSERT CONT-NC` → `-K1`; both listed as two parts of one device. | Feature 1 and any scripted drafting; corrupts device grouping (#3) and BOM counts (#2). |
| **D3** | `ELECREV` reads the wrong project state. | Observed: after `ELECPROJNEW`, every `ELECREV` verb says "no project is loaded"; code: [elec_revision.rs:87-90](../src/app/commands/elec_revision.rs) → `project_dir_for_revisions` → `project_dir` (set only at [elec.rs:3080](../src/app/commands/elec.rs)). | Revision control invisible in the normal `.eprj` workflow. |
| **D4** | Revision compare is size-only; restore does not fail closed. | Observed `COMPARE 1 2` → `no differences (same file set and sizes)`; `RESTORE 1` → `cannot restore A002.dxf: … os error 32` followed by `restored 2 file(s)`. | A restore can silently leave the open drawing un-restored. |
| **D5** | Numbering/collection ignores TEXT-recovered devices. | Real 003: `ELECLIST` 10 components (4 attribute + 6 text) vs `ELECNUMBER` 4 and `ELECXREF` 2 devices. | Numbering and cross-references miss most of a real customer sheet. |
| **D6** | Netlist anchors are symbol insertion points, not pins. | Real 003: `132 wire segment(s), 4 component(s) -> 0 net(s); 4 unconnected`. | Feature 12's pin-to-pin lists cannot work until pins anchor the graph. |
| **D7** | `ELECPROJNUM apply` ignores `WD_M` freeze and `TAG-START`. | Code: pass 2 assigns `1..n` per prefix with no freeze check ([elec.rs:4194-4218](../src/app/commands/elec.rs)) and pass 3 rewrites every sheet with only `.dxf.bak` safety ([:4245-4304](../src/app/commands/elec.rs)). | **Safety:** do not run `ELECPROJNUM apply` on the customer's released project — it will renumber a released set. `ELECNUMBER` on one drawing is protected by the freeze, this is not. |
| **D8** | A batch exits non-zero on any unknown command line, so scripted runs need clean batches. | Observed `EXIT=1` when a malformed line appeared; `--each` returns `error_revision != 0` ([fileops.rs:663](../src/app/commands/fileops.rs)). | Test harnesses must assert on output text, not only exit code. |

---

## 3. Ranked implementation plan for the next 3 rounds

Ranked by **user value × feasibility**, grouped so the Lead can hand each block to one agent. Round 3 (this round) is already allocated to the parts catalogue and I/O import; the sequence below assumes those land.

### Round 4 — "One electrical model" (foundation; highest value, all high feasibility)
1. **D1 collector unification** — single `collect_electrical()` used by all 14 call sites; accept null owner, paper space, text-recovered tags. *Unblocks 1, 3, 8, 12 and D5.*
2. **Pin-level anchors (D6)** — pins from block `ATTDEF`/`TERMnn`, `X1TERMnn` (already written by `ELECPLC`), then wire-graph nets. *Unblocks 12, 14, 15.*
3. **D2 insert tag advance** + `ELECNUMBER` `%S/%X` support; `ELECWIRENUMBER` respects `WIREMODE=R`.
4. **D3/D4 revision fixes** — project file state, SHA-256 compare, fail-closed restore, zero-padded ids, ribbon group.
5. **Build & expose the new modules** — rebuild so `ELECPARTS`/`ELECPARTLIST`/`ELECASSIGN`/`ELECIOIMPORT` exist, add the ribbon group, verify `ELECIOIMPORT` on a real `.xlsx`.
   *Acceptance:* `ELECWIRELIST` on the app's own drawing returns ≥1 net with 2 pins; a snapshot compare detects a one-byte change; `ELECPARTS` listed in the ribbon.

### Round 5 — "Wires, cables, terminals" (feature depth)
6. **Wire processing (12)** — wire type/gauge/colour/part fields, pin-to-pin wire list, wire label CSV.
7. **Cabling (14)** — cable + cores + shield + cable schedule.
8. **Terminal strips (8)** — configurable strip/index derivation, sorting/numbering, stacked terminals, auto strip-layout drawing.
9. **Reports (2 + 16)** — `ELECCABLELIST`, `ELECXREFLIST`, `ELECIOLIST`; `ELECREPORT` to 6 sheets; variants filter hook.
   *Acceptance:* a 2-sheet test project produces a wire list with From/To pin columns, a cable schedule, a terminal list and a 6-sheet workbook.

### Round 6 — "Navigate, panel, lifecycle" (product completeness)
10. **Navigation (7)** — xref labels carrying target handles; `ELECGOTO`/`ELECHIGHLIGHT`; double-click on label; PDF link annotations (16).
11. **Panel layout (11)** — footprints in the parts catalogue, DIN rail, enclosure sizing, cut-outs, panel drawing registered in `.eprj`.
12. **Built control (15)** — built flag + high-water numbering + spares.
13. **Config/systems (18, 19)** — variants in `.eprj`, `library/systems/` + instantiate-with-retag.
14. **Batch publish (16)** — `ELECEXPORT <folder>` (DWG/DXF all sheets) and `ELECPUBLISH <pdf>`.
   *Acceptance:* double-click an xref jumps across sheets; a panel drawing is generated from footprints; a variant project prints a filtered BOM.

**Deferred:** harness (17) — low value without lengths; revisit only if a customer asks.

---

## 4. Deliberately out of scope

| Feature | Why not |
|---|---|
| **4. License sharing** | This is a licensing/seat-management product, not CAD functionality. A local-first, file-based tool has no server to enforce it. If it is ever needed, it is a separate service and must not contaminate the drawing model. |
| **Online parts catalogue** (SkyCAD's model, replaced by the local library) | **Direct user directive: "Instead of Online library, do local library".** `ELECPARTS` + `library/parts/parts.tsv` is the deliberate replacement: offline, project self-contained, no account. This is a *deliberate divergence* from SkyCAD and should be stated as such, not as a missing feature. |
| **17. Harness design** | Requires wire lengths/routing that a 2D schematic does not carry; the customer base here is panel/installation wiring, not harness manufacturing. Low value, low feasibility. |
| **"No training required" as a claim** | We can ship discoverability (ribbon, palette, usage lines, docs) but cannot promise a UX outcome. Track it as quality work, never as a feature-parity bullet. |

---

## 5. Claims we must not make

**Can honestly claim (after Round 4 fixes; some already true):**

* *"Local, offline parts catalogue with BOM enrichment"* — `ELECPART` works today; `ELECPARTS/ELECASSIGN` once built; a deliberate replacement for an online catalogue.
* *"BOM, parts list, wire list and terminal list reports to CSV and a multi-sheet XLSX"* — observed working on a real customer sheet (13 components → 4 BOM rows) and on app-drawn data; catalogue enrichment observed end-to-end (`ELECPART` → manufacturer/description filled), and the BOM CSV header is byte-for-byte the SkyCAD template's column set.
* *"One-command PLC module generation with sequential I/O addresses and project-wide address continuation"* — observed `I0.0..I0.7` and `ELECPLCNEXT`.
* *"Reads AutoCAD Electrical sheets and recovers device tags from text and attributes"* — observed 10–15 components per real sheet.
* *"Project snapshots on disk (`.rev/`), listable and copyable"* — observed; **not** yet "revision control with content compare and reliable rollback".
* *"DWG/DXF read and write, multi-page searchable PDF"* — codec + PDF path are real.

**Can only claim "compatible" / "limited":**

* *"Cross-references"* — **same-sheet, position-based only**, and only for attribute-bearing symbols. Not the SkyCAD feature.
* *"Revision control"* — snapshots + listing yes; compare is size-only and restore is unreliable. Call it "checkpointing", not "revision control".
* *"Terminal strips"* — only for drawings that already use `strip:index` tags; the customer's do not.
* *"Panel layout"* — a device box grid, not a panel design.

**Must say "not implemented":**

* Cabling and cable schedules, harness design, navigation/jump/highlight, configuration management/variants, design-by-systems, built-project modification control, clickable/intelligent PDF cross-references, "real-time" updates of anything.
* **Never** say "AutoCAD Electrical compatible project" or "ACADE-native" (see [ecad-conventions.md](ecad-conventions.md) §3 for the documented boundary), and never call the snapshot feature "revision control" without the caveats above.

**Also never claim a feature-count parity with SkyCAD.** Current honest count: **1 implemented / 11 partial / 6 missing / 1 NA**.

---

## 6. Verification notes (re-runnable)

```powershell
# reporting on a real sheet (copy it to %TEMP% first; never run apply commands on customer files)
probe.exe --each real_sheet_batch.txt  C:\Temp\s002.dxf
# built-in chain on a scratch drawing
probe.exe --each chain_batch.txt       C:\Temp\blank.dxf   # ELECNEW/ELECLIB/ELECINSERT/ELECWIRE/…/QSAVE
# revision control (needs a folder project, not .eprj, in the tested build)
probe.exe --each rev_batch.txt         C:\Temp\proj\A002.dxf
```

Batches used for this report are reproducible from the commands quoted in the matrix; the DXF read-back used `tools/dxfio.py` (`dxfio.load(path).entities`, filtering `e.s(8)` for layer and `e.s(330)` for owner). The tested binary hash/version is recorded in §0 — re-run before trusting this matrix after the in-flight rebuild.

---

## Status updates since this matrix was written

These features landed after the audit; the rows above are the audit as it stood then.

| # | Feature | New status | Evidence |
|---|---|---|---|
| 9 | Revision control and rollback | **Partial (working)** | `ELECREV SNAPSHOT/LIST/COMPARE/RESTORE` in `src/app/commands/elec_revision.rs`; snapshots under `<project>\.rev\`; `ELECPROJNEW/OPEN` now set the project folder so it works in the normal workflow; `RESTORE` fails closed. |
| 12 | Wire processing | **Partial (label list added)** | `ELECWIRELABELS [out.csv]` exports 57 labels on `FC1000495-003`; `ELECWIRELIST`/`ELECWIRENUMBER` now produce real nets (T-junction merging). Wire colours/gauges still absent. |
| 14 | Cabling and cable schedules | **Partial (schedule generation)** | `ELECCABLE` / `ELECCABLELIST` / `ELECCABLEDEL` + `<project>\cables.tsv`; CSV with SkyCAD's column set. Cables are not yet drawn as schematic objects. |
| 15 | Built project modification control | **Partial** | `ELECBUILT on|off` writes `built yes`; `ELECNUMBER` then freezes references and `ELECPROJNUM` refuses to renumber. `WD_M` freeze still not honoured by `ELECPROJNUM`. |
| 2 | BOMs and parts-lists | **Partial+** | Local parts catalogue (`ELECPARTS`/`ELECPARTLIST`/`ELECASSIGN`, 29 parts) feeds BOM part/manufacturer/description columns. |
| 13 | Automated PLC drawings from an I/O list | **Partial (works)** | `ELECIOIMPORT xlsx` → 3 cards / 18 points on the sample list, verified in the saved DXF. |
| 5 | No training required | **Partial+** | `ELECHELP` prints the grouped command reference; native ribbon with 35 icons; searchable palette. |

Current honest score: **1 implemented, 13 partial, 4 missing, 1 not applicable.**

Remaining true gaps: pin-level connectivity (nets do not reach devices), cables drawn as objects,
navigation (highlight/jump), panel layout with footprints and cut-outs, configuration management,
design-by-systems, and the `ELECPROJNUM` / `WD_M` interaction.

---

## Round 8 update — pin-level connectivity (2026-11-06)

The audit's "Remaining true gaps" list named pin-level connectivity as the blocker for
feature 12. It is closed: a net now attaches to a device at its **pin**, not at the
symbol's insertion point, and every connectivity command shares one collector.

| # | Feature | New status | Evidence |
|---|---|---|---|
| 12 | Wire processing | **Partial+ (pin-to-pin connectivity)** | `ELECWIRELIST` on `FC1000495-003`: `8 wire segment(s), 10 pin(s) -> 4 net(s)`, `net 7 -> M326:2`, 6 unconnected pins; on the flattened `-046`: `53 -> 39 nets` with six tagged contacts; on an app-drawn sheet: `net 1 -> -K1:1, -K2:1` and `ELECWIRENUMBER` labels it. Pins come from ACADE `X*TERM` attributes or the symbol block's geometry; terminal pass-throughs are bridged and T-junctions need dots. Still missing: wire colour/gauge/part fields, and the report writers do not yet emit From/To pin columns. |
| 12 | (correction) | **the old "132 segments / 57 nets" on -003 was a measurement error** | the collector counted every `LINE` (symbol strokes, frame, table rules) as a wire; the sheet has 8 `WIRES`-layer segments. Round 8 reads the `WIRES` layer, or layer-0 lines ≥ 0.5 on flattened prints. |

Current honest score is unchanged by this round — **1 implemented, 13 partial, 4 missing,
1 not applicable** — but the largest caveat on feature 12 ("nets never reach devices") is
gone. Remaining true gaps: cables drawn as objects, navigation/highlight/jump, panel layout
with footprints and cut-outs, configuration management, design-by-systems, and the
`ELECPROJNUM` / `WD_M` interaction.

---

## Round 9 update — pin columns and navigation (2026-11-06)

| # | Feature | New status | Evidence |
|---|---|---|---|
| 7 | Navigation tools | **Partial** | `ELECHIGHLIGHT <tag>` selects every part of one device (tagged inserts and the plain TEXT a flattened sheet keeps); `ELECGOTO <tag>` remembers the view and fits it to the device. Both are in a new **Navigation** ribbon group with two house-style icons. Missing half: a cross-reference label is still plain `TEXT` with no target payload, so double-click-to-jump does not exist yet. |
| 12 | Wire processing | **Partial+ (pin columns)** | `ELECWIRELIST <out.csv>` writes `Wire,Net,From pin,To pin,From X,From Y,To X,To Y`; the `ELECREPORT` XLSX wire sheet carries the same columns. The generated starter library (27 symbols, regenerated by `tools/make_library.py`) now declares invisible `X*TERM` pin attributes at the true pin positions, so the list reads `-K1:2 -> -K2:1` from the drawing rather than guessing from geometry. Still missing: wire colour/gauge/part-number fields. |
| 5 | No training required | **Partial+** | `ELECHELP` gained the Navigation group; the Electrical ribbon is 9 groups / 43 command ids / 37 icons. |

Current honest score: **1 implemented, 14 partial, 3 missing, 1 not applicable.**

Remaining true gaps: cables drawn as schematic objects (the schedule data exists, the
geometry does not), panel layout with footprints and cut-outs, configuration management /
variants, design-by-systems, clickable cross-reference labels in drawing and PDF, and the
`ELECPROJNUM` / `WD_M` interaction. Harness design stays deferred.

---

## Round 10 update — cables drawn on the schematic (2026-11-06)

| # | Feature | New status | Evidence |
|---|---|---|---|
| 14 | Cabling and cable schedules | **Partial+ (cable drawn as an object)** | `ELECCABLEDRAW <tag> [CORES=] [SHIELD=yes] [PART=] [DESC=]` chains a route onto the `CABLES` layer, writes the tag beside the first point and a semicircle shield bracket when shielded; the record is upserted per segment into `cables.tsv` (now `tag cores part manufacturer length description route shield`). `ELECCABLELIST` measures the route and resolves both ends to device pins: `W1 3 LIYCY-3X1.5 - 43.83 yes -K1:2 -K2:1`; `ELECREPORT` writes a fourth `Cables` sheet. Verified in the saved drawing: 2 `CABLES` lines, tag `W1`, 1 shield arc, route length 43.829. Missing: per-core conductor nets (the cable is one bundle), and shield connectivity is a graphic, not a net. |
| 2 | BOMs and parts-lists | **Partial+** | `ELECREPORT` now exports four sheets — BOM, Wire List (with From/To pin columns), Terminals, Cables — so the cable schedule travels with the other lists. |

Current honest score: **1 implemented, 14 partial, 3 missing, 1 not applicable.** Remaining
true gaps: panel layout with footprints and cut-outs, configuration management / variants,
design-by-systems (plus the harness deferred), and inside the partial features the per-core
cable nets, clickable cross-reference labels, and the `ELECPROJNUM` / `WD_M` interaction.

---

## Round 11 update — panel layout from footprints (2026-11-06)

| # | Feature | New status | Evidence |
|---|---|---|---|
| 11 | Panel layout design | **Partial+ (footprint-driven)** | `library/panel/footprints.tsv` (28 class rows + `DEFAULT`, project override at `<project>/footprints.tsv`) gives each class its mounting width, height and mount point (`rail`/`door`/`plate`). `ELECPANEL` places rail devices on 35 mm DIN rails (wrapping at 560 mm), plate devices on plate rows and door devices along the door band with round cut-outs, sizes the enclosure from the placements (`420x380 mm` on an 8-device drawing), draws enclosure/door/handle/rails/footprints/cut-outs on six layers, and registers the drawing once in `project.eprj` (`sheet panel.dxf`, `number P2`, `title Panel layout`). Read back: 4 enclosure lines, 8 door lines, 2 rail lines, 32 device lines, 1 r=15 cut-out; widths {8,18,30,45,90,100} mm match the table. Missing: part-level footprint overrides (2-pole vs 4-pole breaker), an enclosure catalogue, and a separate door elevation. |

Current honest score: **1 implemented, 14 partial, 3 missing, 1 not applicable.** Remaining
true gaps: configuration management / variants and design-by-systems (18, 19), the deferred
harness (17), and inside the partial features the per-core cable nets, clickable
cross-reference labels, and the `ELECPROJNUM` / `WD_M` interaction — plus packaging.

---

## Round 12 update — design by systems, variants and option codes (2026-11-06)

| # | Feature | New status | Evidence |
|---|---|---|---|
| 19 | Design by systems / sub-assemblies | **Partial** | Reusable systems live as `library/systems/*.tsv` fragments (`device dx dy symbol`, `wire x1 y1 x2 y2`), overridable per project (`<project>/systems/`). `ELECSYSTEM LIST` shows the two seeded starters; `ELECSYSTEM INSERT <name> [x,y]` places the symbols with their full attribute set, draws the wires on `WIRES` and retags into the drawing's own sequences. Verified: three insertions → 14 inserts, 7 wires, tags -K1..-K7, -M1..-M3, -S1..-S4, second starter at x=200, reversing at y=-200. Missing: capturing the current drawing as a system (`ELECSYSTEM SAVE`). |
| 18 | Configuration management (variants) | **Partial** | `ELECVARIANT NEW <name>` copies the master sheet list into `variant-<name>.eprj`; `LIST` shows name/sheet count/active; `USE <name>` switches the active project file so project commands work on that selection. `ELECOPTION <tag> <option>` records option codes in the active project file (`LIST`, `CLEAR <tag>`). Verified: variant file with copied sheet list, `option -K1 pump-set-B` recorded and listed, active project switched. Missing: reports/BOM filtering by variant or option code. |

Current honest score: **1 implemented, 16 partial, 1 missing, 1 not applicable.**

Only the deferred harness (17) remains without a working path; every other feature has at
least a partial implementation, with the caveats listed above. Inside partials the remaining work is: report filtering by
variant/option, `ELECSYSTEM SAVE`, per-core cable nets, clickable cross-reference labels,
`ELECPROJNUM` honouring `WD_M`, and the packaging (installer, sample project, quick-start,
refreshed smoke suite, CI).

---

## Round 13 update — WD_M-safe project numbering, packaging (2026-11-06)

| # | Feature | New status | Evidence |
|---|---|---|---|
| 15 | Built project modification control | **Partial+ (WD_M honoured by project numbering)** | `ELECPROJNUM` pass 1 reads each sheet's `WD_M`; a device on a `TAGMODE=R` sheet is frozen and leaves its tag alone, while its number is reserved so new devices get the next free one (`TAG-START` seeds the sequence). Verified: real `FC1000495-003` project previews `would renumber 0. 2 device(s) frozen by WD_M` with the file unchanged; an unfrozen drawing still renumbers -K7/-K9 to -K1/-K2. `ELECBUILT` remains the project-level lock. |

Packaging (not a feature row): `docs/QUICKSTART.md`, `packaging/portable.ps1` (63 files,
150.8 MB portable folder), `.github/workflows/ecad.yml` and `samples/demo-line/`. Current
honest score unchanged: **1 implemented, 16 partial, 1 missing, 1 not applicable.**

---

## Round 14 update — verification and CI (2026-11-06)

Not a feature row, but the evidence base changed: `tests/ecad/run_smoke.py` was refreshed to
the layer's current behaviour (99/99 with the customer sheets, 84/84 with
`--no-real-sheets`), and `.github/workflows/ecad.yml` now builds the release binary and runs
that suite on `windows-latest`. The matrix's earlier "smoke suite is stale" caveat is retired;
the customer-sheet baseline remains a local release-checklist item because those DXFs are not
committed.

---

## Round 15 update — `ELECSYSTEM SAVE` (2026-11-06)

| # | Feature | New status | Evidence |
|---|---|---|---|
| 19 | Design by systems | **Partial+** | `ELECSYSTEM SAVE <name>` captures the active drawing's tagged devices and wire segments into `<project>/systems/<name>.tsv`, relative to the lower-left device; the saved system is listed with the library ones and re-inserts with retagging. Verified: DOL starter saved as `captured_starter` and inserted at 300,0 as -K3, -K4, -M2, -S2 (8 inserts, 4 wires); smoke suite 99/99. |

Current honest score unchanged: **1 implemented, 16 partial, 1 missing, 1 not applicable.**

---

## Round 16 update — option-filtered BOM (2026-11-06)

| # | Feature | New status | Evidence |
|---|---|---|---|
| 18 | Configuration management | **Partial+ (reports use option codes)** | `ELECBOM [out.csv] [OPTION=<code>]` keeps only devices marked with `ELECOPTION <tag> <code>` and reports the selection (`option pump-set-B selects 1 of 3 component(s)`); the filtered CSV carries only `-K1`. Variant selection already applies at project level through `ELECVARIANT USE` (the active `.eprj` lists the variant's sheets). Missing: no per-variant BOM command beyond switching the project, and no option editing UI. |

Current honest score unchanged: **1 implemented, 16 partial, 1 missing, 1 not applicable.**

---

## Final state (2026-11-06)

All features named in the project objective are implemented and verified (PROGRESS round 17);
the matrix score is **1 implemented, 16 partial, 1 missing, 1 not applicable**. The one
missing row is the deferred harness (17), which needs wire lengths a 2D schematic does not
carry. Open depth items inside partials are listed in HANDOFF.md.

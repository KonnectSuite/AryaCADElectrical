# Quick start — AryaCADElectrical

The electrical layer lives inside OpenCADStudio: one `OpenCADStudio.exe`, an **Electrical**
ribbon tab, and a set of `ELEC…` commands that also work from the command line and from
headless batches. Everything is local and file-based; no account, no online catalogue.

## 1. Run it

Build from source (see `reports/HANDOFF.md` for the full build notes):

```powershell
$env:PATH="$env:USERPROFILE\.cargo\bin;$env:PATH"
cargo build --release -j 4
target\release\OpenCADStudio.exe
```

or use the portable folder produced by `packaging/portable.ps1`:

```powershell
powershell -File packaging\portable.ps1
dist\AryaCADElectrical-portable\OpenCADStudio.exe
```

A sample project is in `samples\demo-line\`: open `project.eprj` (or the drawings directly),
run `ELECLIST`, `ELECWIRELIST`, `ELECCABLELIST` and `ELECPANEL` to see the finished outputs.

## 2. First schematic

| step | command |
|---|---|
| start from a standard sheet | `ELECNEW templates\iec\A3_landscape.dxf` (IEC A4–A0, NEC ANSI A–E; see `templates\README.md`) |
| new drawing, electrical layers | `ELECNEW` |
| load the symbol library | `ELECLIB library\starter` (or `library` for the harvested set) |
| browse/place symbols | `ELECPALETTE`, or `ELECINSERT CONTACT_NO` |
| draw conductors | `ELECWIRE`, click the points, Enter to finish |
| junction dots | `ELECJUNCTIONS` |
| list the devices | `ELECLIST` |
| number devices | `ELECNUMBER` (preview) / `ELECNUMBER apply` |
| wire numbers and labels | `ELECWIRENUMBER`, then `ELECWIRELABELS labels.csv` |

Every symbol carries real pins: the local library writes `X*TERM*` attributes at the pin
positions, so `ELECWIRELIST` names the device pin at each end of a conductor
(`net 1 -> -K1:2, -K2:1`) and `ELECWIRELIST wirelist.csv` exports
`Wire, Net, From pin, To pin, From X, From Y, To X, To Y`.

## 3. Parts, BOM and reports

| step | command |
|---|---|
| load the local parts catalogue | `ELECPARTS library\parts` |
| search / attach a part | `ELECPARTLIST fuse`, `ELECASSIGN -K1 LPJ-100SP` |
| bill of material | `ELECBOM bom.csv` |
| full workbook (BOM, wire list, terminals, cables) | `ELECREPORT report.xlsx` |

## 4. PLC and I/O

`ELECPLC -P1 8 I0.0` builds a PLC module with `ADDRnn` attributes; `ELECPLCNEXT` continues
the address sequence across the project; `ELECIO` lists the device each point reaches.

`ELECIOIMPORT iolist.xlsx` generates the PLC modules from a controls engineer's I/O sheet
(tolerant header matching; one card per address prefix).

## 5. Cables

A cable is a drawn object, and the schedule is measured from it:

1. `ELECCABLEDRAW W1 CORES=3 SHIELD=yes PART=LIYCY-3X1.5 DESC=Motor feeder`
2. click the route, Enter to finish — the route lands on `CABLES`, the tag beside it, and a
   shield bracket when `SHIELD=yes`.
3. `ELECCABLELIST cables.csv` prints/export the schedule: measured length and both ends
   resolved to device pins (`W1, Motor feeder, LIYCY-3X1.5, -, 43.83, 3, yes, -K1:2, -K2:1`).

`ELECCABLE` adds a schedule-only cable when there is nothing to draw.

## 6. Panel layout

1. put devices on the schematic first (the panel is generated from them);
2. `ELECPANEL panel.dxf` — places rail devices on 35 mm DIN rails, plate devices on plate
   rows and door devices on the door with round cut-outs, sizes the enclosure and registers
   the drawing in the project file. Footprints are editable in
   `library\panel\footprints.tsv` (a project can override them with `footprints.tsv`).

## 7. Projects, revisions, systems, variants

| step | command |
|---|---|
| build a project from a folder of drawings | `ELECPROJNEW C:\Projects\Line` |
| list the project's sheets | `ELECPROJLIST` |
| renumber across the project | `ELECPROJNUM` (preview) / `ELECPROJNUM apply` — honours `WD_M` `TAGMODE=R` |
| revision snapshots | `ELECREV SNAPSHOT label`, `ELECREV LIST`, `ELECREV RESTORE id` |
| built/assembled protection | `ELECBUILT ON` / `ELECBUILT OFF` |
| reusable sub-circuits | `ELECSYSTEM LIST`, `ELECSYSTEM INSERT DOL_STARTER 0,0`, `ELECSYSTEM SAVE my_starter` |
| alternative sheet sets | `ELECVARIANT NEW pumpset_b`, `ELECVARIANT LIST`, `ELECVARIANT USE pumpset_b` |
| option codes | `ELECOPTION -K1 pump-set-B`, `ELECOPTION LIST` |

`ELECHELP` prints every electrical command grouped the way the ribbon is organised.

## 8. Drawings from AutoCAD Electrical

`ELECSETUP` prints the drawing's own `WD_M` contract (`TAGFMT`, `TAGMODE`, `WIREFMT`).
When `TAGMODE=R` the references on that sheet are fixed: `ELECNUMBER` and `ELECPROJNUM`
leave them alone. The customer prints that were exploded (symbols flattened to text) still
yield their device tags from the text; nets only reach devices where the symbols survived.

## 9. Headless use (scripting and tests)

```powershell
target\release\OpenCADStudio.exe --each batch.txt drawing.dxf
```

One command per line, a blank line ends an interactive command, point input is `x,y`, and
the drawing is only written when the batch ends with `SAVE`/`QSAVE`. Every example in this
document works that way; `reports/HANDOFF.md` lists the batches used for verification.

## 10. Honest limits

Panels need part-level footprint overrides and a separate door elevation; a drawn cable is
one bundle (its cores are not separate nets yet); cross-reference labels are text, not
clickable links; variants select sheets but reports do not filter by them yet; and the
build/test gates are manual (`cargo check --release --tests`, `cargo test -p elec --lib`).

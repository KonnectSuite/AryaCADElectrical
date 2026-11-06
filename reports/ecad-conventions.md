# ECAD conventions — implementation brief for AryaCADElectrical

**Status:** research complete · **Date:** 2026-11-05 · **Author:** ecad-conventions (research subagent)
**Deliverable:** this file only. **Companion:** [STATUS.md](STATUS.md) (existing `ELEC*` command layer, verified behaviours).

This is a *domain reference* for an open-source, licence-clean electrical CAD. It is not a standards reproduction: IEC full texts are paywalled, and every place where we could not verify primary text is marked **[unverified]**.

---

## 0. Method and evidence limits

* The session's `web_search` tool returned no results, so research was done with direct `web_fetch` of known/primary URLs plus DuckDuckGo's HTML endpoint while it was rate-limit-free. Where only a search snippet could be read, the claim is marked **[snippet]**.
* **IEC paywall:** the IEC 60617 symbol database (`https://std.iec.ch/iec60617`) returned **HTTP 403** to an automated fetch, and IEC standards are sold per-document. **No IEC clause text was read.** Standard-level claims below therefore rest on secondary summaries whose primary references are named, and on what Autodesk publishes.
* **Quotes are short** (a clause or a table cell), never whole paragraphs.
* Autodesk AutoCAD Electrical (ACADE) public help pages were readable and are the primary source for the ACADE sections. Autodesk-authored PDF class handouts are cited but were **not machine-readable here** (`application/pdf` unsupported) — marked **[snippet]**.

---

## 1. Symbol standards and an open-licensed geometry seed

### 1.1 What the canonical set is

| Standard | Role | Open? |
|---|---|---|
| **IEC 60617** ("Graphical symbols for diagrams", maintained as a database, implemented nationally as e.g. DIN EN 60617 / NF EN 60617) | The current international symbol standard for diagrams | **No.** Sold by IEC; the public DB rejects automated access ([IEC webstore entry](https://webstore.iec.ch/en/iec-search/result?q=60617), [status summary](https://en.wikipedia.org/wiki/Electronic_symbol)) |
| **IEC 61082-1** (preparation of documents used in electrotechnology — rules for diagrams) **[unverified title]** | Drawing rules: sheet layout, reference systems, cross-references | **No.** No public text found |
| **ANSI/IEEE 315** (Graphic Symbols for Electrical and Electronics Diagrams, incl. reference-designation letters; also ANSI Y32.2 / CSA Z99) | US symbol set + class letters | **No.** IEEE sells it; **inactivated without replacement on 2019-11-07** ([Electronic symbol](https://en.wikipedia.org/wiki/Electronic_symbol)) |
| **ASME Y14.44-2008** | Reference designations (replaced IEEE 200-1975) ([Reference designator](https://en.wikipedia.org/wiki/Reference_designator)) | **No** |

Consequence: **there is no free, authoritative IEC 60617 geometry set.** Any open project must either (a) ship a licence-compatible approximation and say so, or (b) require the user to license the IEC set. We do (a).

### 1.2 Open-licensed geometry that actually exists

| Source | Licence | Obligations / limits | Fit for us |
|---|---|---|---|
| **QElectroTech official element collection** (8000+ elements, industrial-electrical oriented) | **CC-BY 3.0** when the collection (or part) is redistributed *outside* an electrical diagram; **unrestricted** use/modification *inside* diagrams regardless of the diagram's licence. App itself GPL. Also: **"Permission is not granted to use this software or any of the associated files as sample data for the purposes of building machine learning models."** ([ELEMENTS.LICENSE](https://github.com/qelectrotech/qelectrotech-elements/blob/main/ELEMENTS.LICENSE), [project site](https://qelectrotech.org/)) | Attribution + licence text on redistribution; modifications stay CC-BY; **no ML-training use** | **Best seed.** IEC-style installation symbols (contactors, terminals, motors, fuses) |
| **KiCad symbol libraries** | **CC-BY-SA 4.0 + exception**: using library data in a *design* imposes nothing; **redistributing the libraries (or parts, modified) as a collection keeps CC-BY-SA** and must retain attribution/licence docs ([KiCad libraries licence](https://www.kicad.org/libraries/license/)) | Share-alike applies to the redistributed collection | Good for electronics symbols; do **not** fold into our core library — keep as an optional, separately-licensed pack |
| **LibrePCB official libraries** | **CC0-1.0** ([LibrePCB licence](https://librepcb.org/license/)) | None | Useful for part/footprint metadata; not an IEC-60617 installation set |
| IEC official set | Commercial | — | Not usable |

### 1.3 Recommendation and attribution duties

1. **Seed = QElectroTech official collection**, filtered to the industrial-electrical categories we need, imported through a converter, with **our own redraws** for the primitive symbols we already harvested (the 51-symbol library described in [STATUS.md](STATUS.md)).
2. **Do not** call the result "IEC 60617 compliant". Say **"IEC 60617-style"** and keep a mapping table `our_symbol → nearest IEC 60617 symbol number` where the user supplies the number.
3. **Licence hygiene:** `library.tsv` gains `source`, `licence`, `attribution`, `modified` columns; repo ships `NOTICE` + `LICENSES/CC-BY-3.0.txt` + QET's `ELEMENTS.LICENSE` verbatim; About box lists QElectroTech; record in the repo that QET files are **not** used as ML training data.
4. A future optional "KiCad pack" is a separate download with its own CC-BY-SA notice, so share-alike never touches our core library.
5. IEEE 315 symbols are **not** available openly and were inactivated in 2019 — we do not implement them; we implement the *class-letter idea* from IEC 81346-2 instead (§2).

---

## 2. Device tagging (IEC 81346 / reference designators / ACADE)

### 2.1 IEC 81346 aspects — the part the brief got slightly wrong

ISO/IEC 81346-1 defines three aspects and their prefix signs ([EN IEC 81346 summary, de.wikipedia](https://de.wikipedia.org/wiki/EN_IEC_81346)):

| Sign | Aspect | Meaning |
|---|---|---|
| `=` | **Function** | what the object does |
| `-` | **Product** | how the system is composed (the classic *Betriebsmittelkennzeichen*, e.g. `-K1`) |
| `+` | **Location** | where the object is |

So the product prefix is **`-`**, not `==` (the task brief's `==` is a mis-transcription — worth correcting in our docs).

IEC 81346-2:2019 then assigns class letters by **inherent function** (deliberately *not* by component kind), with three levels (A/B/C classes) and **616 classes** total; the 2019 letters are **not fully compatible** with the 2009 edition ([de.wikipedia](https://de.wikipedia.org/wiki/EN_IEC_81346), [en.wikipedia](https://en.wikipedia.org/wiki/IEC_81346)). Functional main classes (2019 sense): B sensing, C storing, E emitting, F protecting, G initiating/generating, K processing signals, M mechanical drive, P presenting information, Q controlled switching, R limiting/stabilising, S manual→signal, T converting energy, U holding, W conducting, X connecting ([de.wikipedia table](https://de.wikipedia.org/wiki/EN_IEC_81346)).

### 2.2 IEEE/ANSI reference designators (for the electronics side)

`C` capacitor, `R` resistor, `K` relay/contactor, `Q` transistor, `F` fuse, `M` motor, `T` transformer, `X` socket/connector, `P` plug; assemblies `A`, inseparable assemblies `U`; `1A12A2R3` = unit/assembly/sub-assembly/component path ([Reference designator](https://en.wikipedia.org/wiki/Reference_designator)) — the designator rules come from IEEE 315 ch. 22 and ASME Y14.44.

**Conflict to manage:** `K`/`Q`/`T`/`F` mean different things in IEEE 315 and 81346-2 (relay vs signal-processing; transistor vs controlled switching; transformer vs energy conversion). The user's drawings are IEC-land, so **81346-2 wins by default**, but the class table must be user-editable.

### 2.3 How ACADE composes a tag (documented)

* Tag format is stored on the invisible `WD_M` block attribute **`TAGFMT`**, default **`%F%N`** ([About the WD_M block](https://help.autodesk.com/cloudhelp/2022/ENU/AutoCAD-Electrical/files/GUID-75AEBC13-2F09-4B9E-B349-C7B202D52119.htm), [About Replaceable Parameters](https://help.autodesk.com/cloudhelp/2022/ENU/AutoCAD-Electrical/files/GUID-8F803D63-4390-4617-BC71-F68AE7545D97.htm)).
* Replaceable parameters: `%F` family code string ("PB", "SS", "CR", "FLT", "MTR"), `%S` sheet, `%D` drawing number, `%N` sequential-or-reference number, `%X` suffix position, `%P`/`%I`/`%L` IEC project/installation/location, `%A`/`%B` project drawing-list SEC/SUB-SEC ([Replaceable Parameters](https://help.autodesk.com/cloudhelp/2022/ENU/AutoCAD-Electrical/files/GUID-8F803D63-4390-4617-BC71-F68AE7545D97.htm)).
* Tagging strategies on `WD_M`: **`TAGMODE` = `S` sequential / `R` reference-based**, `TAG-START`, `TAG-RSUF` (suffix list "A,B,C") ([WD_M](https://help.autodesk.com/cloudhelp/2022/ENU/AutoCAD-Electrical/files/GUID-75AEBC13-2F09-4B9E-B349-C7B202D52119.htm)).
* Worked examples from Autodesk: relay 50 on sheet 3 → `CR350` (`%F%S%N`), `CR50` (`%F%N`), `CR-3-50`; three push buttons on line 101 with reference-based tagging → `PB101`, `PB101A`, `PB101B` ([Replaceable Parameters](https://help.autodesk.com/cloudhelp/2022/ENU/AutoCAD-Electrical/files/GUID-8F803D63-4390-4617-BC71-F68AE7545D97.htm)).

### 2.4 Convention we adopt

* **Canonical stored tag**: `TAG1` holds the **product designation without prefix** (`K1`), and rendering adds `-` → `-K1`. Rationale: ACADE's default `%F%N` has no prefix, so a bare tag round-trips; the IEC prefix becomes a display/print option.
* **Default profile (IEC)**: `-<class><number>` with 81346-2 letters — `-K1` (signal processing), `-Q1` (switching), `-F1` (protection), `-M1` (drive), `-T1` (energy conversion), `-X1` (connecting/terminal strip), `-P1` (indication), `-B1` (sensing), `-W1` (cable), `-S1` (manual actuation).
* **Family is not the class letter**: keep a `family` field (ACADE's `%F`, e.g. `CR`, `MTR`) separate from the 81346 class, exactly as ACADE does.
* **Location/installation/function** are separate fields (`LOC`, `INST`, `FUNC`) and are *composed* into tags/xrefs only by format strings (`%I`, `%L`), never concatenated ad hoc.
* **Never derive a tag from the symbol's display name**; a tag is allocated by the numbering engine (§4).

---

## 3. AutoCAD Electrical on-disk conventions — and the interop boundary

### 3.1 What Autodesk documents (public help)

* **`WD_M` is mandatory.** "A special invisible block must be present on the drawing"; the alert text Autodesk documents is *"A drawing needs an invisible block, WD_M.dwg, on the drawing to be compatible with AutoCAD Electrical toolset."* ([About the WD_M block](https://help.autodesk.com/cloudhelp/2022/ENU/AutoCAD-Electrical/files/GUID-75AEBC13-2F09-4B9E-B349-C7B202D52119.htm)).
* `WD_M` is placed at 0,0 and carries **drawing-level settings as attributes**, grouped by Autodesk into: drawing layout (`SHEET`, `SHEETDWGNAME`, `IEC_PROJ`, `IEC_INST`, `IEC_LOC`, `UNIT_SCL`, `FEATURE_SCL`), ladder defaults (`RUNGHORV`, `REFNUMS`, `RUNGDIST`, `DLADW`, `RUNGINC`, `DRWRUNG`, `PH3SPACE`), component tagging (§2.3), wire numbering (§4), layer names (`TAG_LAY`, `TAGFIXED_LAY`, `DESC_LAY`, `CDESC_LAY`, `TERM_LAY`, `XREF_LAY`, `CXREF_LAY`, `LOC_LAY`, `POS_LAY`, `COMP_LAY`, `LINK_LAY`, `LOCBOX_LAY`, `WIRELAYS`, `WIRENO_LAY`, `WIRECOPY_LAY`, `WIREFIXED_LAY`, `WIREREF_LAY`), fan-in/out, cross-reference (§5), referencing grid (`DATUMX`, `DATUMY`, `DISTH`, `DISTV`, `CHAR_H`, `CHAR_V`, `HORIZ_FIRST`, `XY_DELIM`), styles (`PLC_STYLE`, `ARROW_STYLE`) and misc flags (`WNUM_OFFSET`, `WNUM_FLAGS`, `MISC_FLAGS` with bit 4 = "use plc wire numbers") ([WD_M](https://help.autodesk.com/cloudhelp/2022/ENU/AutoCAD-Electrical/files/GUID-75AEBC13-2F09-4B9E-B349-C7B202D52119.htm)).
* **Panel counterpart** `WD_PNLM` (invisible settings block for layout drawings) — Autodesk class handout **[snippet]**: <https://static.au-uw2-prd.autodesk.com/Class_Presentation_IM322678_Randy_Brunette.pdf>.
* **Project database** is a *Microsoft Access* file, **temporary and derived**: "This file is for temporary use only. If the project database file is missing or corrupted, it is automatically generated from the drawing set." Editable/importable tables: **COMP, TERMS, WNUM, PINLIST, COMPPINS, PLCIO, POSLIST, SIG, FILETIME, PNLCOMP, PNLTERM, PNLWDLEV, CONDUIT, USER** ([About the Project Database](https://help.autodesk.com/cloudhelp/2025/ENU/AutoCAD-Electrical/files/GUID-8DF798D6-F71A-413E-AD1B-ABBE8A6ACB81.htm)); edits are re-imported to drawings from a copy ([To Update Drawings From the Project Database](https://help.autodesk.com/cloudhelp/2025/ENU/AutoCAD-Electrical/files/GUID-EF6CD1F8-DF8D-43CA-ADCB-90D9B949EB3A.htm)).
* **Catalog database**: a project-specific catalog database is always created as **`.mdb`** even when the source catalog is SQL Server **[snippet]** ([To Create a Project-Specific Catalog Database](https://help.autodesk.com/cloudhelp/2023/ENU/AutoCAD-Electrical/files/GUID-031AA9B4-9578-4166-A5A4-79E3B0B990AD.htm)). Since ACADE 2026 the Access **Runtime** is a prerequisite (Access Database Engine 2016 support ended 2025-10-14) ([Symetri](https://www.symetri.co.uk/insights/blog/transitioning-to-microsoft-access-runtime-for-autocad-electrical-2026/)).
* Environment settings live in `wd.env` and are referenced by `WD_M` values (e.g. `GBL_wd_importdbUppercase`, `GBL_wd_wnum_offset`) ([Update From Project Database](https://help.autodesk.com/cloudhelp/2025/ENU/AutoCAD-Electrical/files/GUID-EF6CD1F8-DF8D-43CA-ADCB-90D9B949EB3A.htm), [WD_M](https://help.autodesk.com/cloudhelp/2022/ENU/AutoCAD-Electrical/files/GUID-75AEBC13-2F09-4B9E-B349-C7B202D52119.htm)).

### 3.2 What is *not* publicly documented (do not pretend otherwise)

* **The complete attribute schema of stock symbols.** The names we use — `TAG1`, `TAG2`, `RATING1..12`, `MFG`, `CAT`, `DESC1..3`, `INST`, `LOC`, `TERM01..`, `FAMILY`, `PINLIST` — are a **de-facto schema** observed in ACADE-authored DXFs (including the user's 34 production sheets, see [STATUS.md](STATUS.md) §3 item 6). We did **not** find a public Autodesk page enumerating them. Treat them as *compatibility profile*, not specification.
* **XDATA / extension-dictionary usage.** No public Autodesk page found stating which ACADE data lives in XDATA, extension dictionaries, or custom objects. We must not claim knowledge of it.
* **`.dwg` internals and custom objects.** DWG is proprietary; DXF is the documented interchange, but DXF carries only *entities and attributes*, not ACADE's proprietary project semantics.
* **Project (`.wdp`) format details and the PDS (Project Database Service) internals** — only the existence and behaviour are documented (§3.1).
* **Which `WD_*` block names exist.** Documented: `WD_M` (§3.1) and `WD_PNLM` **[snippet]**. Names such as `WD_WNH` are **not** documented in any public list we could reach; treat any `WD_*` block as "setup/control — inspect attributes, do not draw".

### 3.3 Interop boundary (honesty box)

| Claim | Verdict |
|---|---|
| "We read and write DXF with ACADE-style blocks, layers and attributes" | **Yes** — fully in our control, testable headless |
| "An ACADE user can open our DXF and run ACADE commands on the symbols" | **Plausible but unproven.** Requires `WD_M` + stock symbol blocks + expected layers/attributes; **must be validated on a licensed seat**; we have none |
| "We write ACADE projects (`.wdp` + `.mdb`) that ACADE will open" | **No.** Only partially documented; PDS/format not public |
| "We preserve ACADE drawings perfectly through a round trip" | **No.** Unknown XDATA/custom-object content would be lost |
| "ACADE-native" as a marketing phrase | **Forbidden** until validated. Use **"ACADE-compatible DXF profile"** |

**Design decision:** we ship an explicit **"ACADE-compatible DXF profile v1"** — `WD_M` block at 0,0 with a documented subset of the attributes in §3.1, standard layer names, `TAG1`/`TAG2`/`RATING1`/`MFG`/`CAT`/`DESC1..3`/`INST`/`LOC`/`TERM01..`, and a `PROVENANCE` marker in a project extension dictionary saying "written by AryaCADElectrical, profile v1, unvalidated against ACADE". If a licensed seat ever validates it, bump the profile version and record the ACADE build.

---

## 4. Wire numbering, net naming, cables, terminals

### 4.1 Documented ACADE mechanism

* `WIREMODE` = `S` sequential / `R` reference-based; `WIREFMT` default **`%N`**; `WIRE-START`; `WIRE-RSUF`; `WINC` increment; `SORTMODE` sort order for retag/number; `WNUM_OFFSET` placement; `WNUM_FLAGS` (inline gap adjust + above/below wire) ([WD_M](https://help.autodesk.com/cloudhelp/2022/ENU/AutoCAD-Electrical/files/GUID-75AEBC13-2F09-4B9E-B349-C7B202D52119.htm)).
* Formats use the same `%` language: `%N` = 50, `%S/%N` = 3/50, `W-%S%N` = W-350 ([Replaceable Parameters](https://help.autodesk.com/cloudhelp/2022/ENU/AutoCAD-Electrical/files/GUID-8F803D63-4390-4617-BC71-F68AE7545D97.htm)).
* **Wire type / gauge / colour is carried by the wire *layer***: `%G` is documented both as "wire layer name" and as "wire color/gauge (or wire layer name)"; `WIRELAYS` lists valid wire layers ([Replaceable Parameters](https://help.autodesk.com/cloudhelp/2022/ENU/AutoCAD-Electrical/files/GUID-8F803D63-4390-4617-BC71-F68AE7545D97.htm), [WD_M](https://help.autodesk.com/cloudhelp/2022/ENU/AutoCAD-Electrical/files/GUID-75AEBC13-2F09-4B9E-B349-C7B202D52119.htm)). This is the single most important ACADE idea for us: **encode conductor class in the layer name, not only in attributes.**
* **Cable/core**: `%E` cable tag, `%J` conductor/core colour, `%C` tag+colour ("tag-color"), `%V` cable tag substituted for wire number when non-blank, `%H` cable colour substituted when non-blank; cable data sits in the project DB (COMP/TERMS/WNUM + `SIG` for signal codes) ([Replaceable Parameters](https://help.autodesk.com/cloudhelp/2022/ENU/AutoCAD-Electrical/files/GUID-8F803D63-4390-4617-BC71-F68AE7545D97.htm), [Project Database](https://help.autodesk.com/cloudhelp/2025/ENU/AutoCAD-Electrical/files/GUID-8DF798D6-F71A-413E-AD1B-ABBE8A6ACB81.htm)).
* **Terminals**: `%T` terminal-strip pin assignment, `%K` terminal `TERMDESC` (multi-stack), and `%1`–`%9` destination-component combinations (`%2` = `%1:%P`, `%3` = `%1:%P:%D`, `%4` = `%L%1`, …) ([Replaceable Parameters](https://help.autodesk.com/cloudhelp/2022/ENU/AutoCAD-Electrical/files/GUID-8F803D63-4390-4617-BC71-F68AE7545D97.htm)).
* Colour/terminal **standards** we can name but not read: **IEC 60445** ("Identification of equipment terminals, conductor terminations and conductors") and **IEC 60757** ("Code for designation of colours") ([List of IEC standards](https://en.wikipedia.org/wiki/List_of_IEC_standards)). The brief's "IEC 61605?" for terminal numbering **could not be verified** — we should not cite it. DIN-rail dimensions are **IEC 60715** ([same list](https://en.wikipedia.org/wiki/List_of_IEC_standards)).

### 4.2 Scheme we implement

```
net            = logical equipotential (graph of wire segments + pin connections)
wire_number    = strategy(net) : sequential | sheet_sequential | reference_based | manual
                 format template with {N} {S} {Z} {prefix} {suffix}
wire_layer     = conductor class: "<class>-<gauge>-<colour>" e.g. "PWR-1.5-BK"  (ACADE %G)
cable          = tag + { conductor: core_id, colour, gauge, net }
terminal       = strip tag + pin number (+ TERMDESC) ; terminals are components, not wires
```
Rules: number per **net** not per segment (one number per equipotential); reference-based = `{sheet}-{zone}{line}`; never renumber a manual number unless `--force`; numbering must be **idempotent** and preview must be **non-mutating** (already proven for `ELECNUMBERPREVIEW`, [STATUS.md](STATUS.md) §2).

---

## 5. Cross-referencing

### 5.1 Documented ACADE mechanism

* `XREFFMT` default **`%N`**; **`ALT_XREFFMT`** is the *inter-drawing* format, documented example **`%S-%N`**; `XREF_STYLE` 0 text / 1 graphical / 2 table; `XREF_FLAGS` (include unused contacts, include parent coil in tables); `XREF_UNUSEDSTYLE`; `XREF_GRAPHICSTYLE` 0 JIC / 1 IEC; `XREF_CONTACTMAP` lists contact mappings; table title/style/index attributes ([WD_M](https://help.autodesk.com/cloudhelp/2022/ENU/AutoCAD-Electrical/files/GUID-75AEBC13-2F09-4B9E-B349-C7B202D52119.htm)).
* `REFNUMS` chooses the reference mode: **1** line-reference numbers, **2** numbers with ruling, **3** user-defined block, **4** X-Y grid, **5** X-zone; grid geometry comes from `DATUMX/DATUMY/DISTH/DISTV/CHAR_H/CHAR_V/HORIZ_FIRST/XY_DELIM` ([WD_M](https://help.autodesk.com/cloudhelp/2022/ENU/AutoCAD-Electrical/files/GUID-75AEBC13-2F09-4B9E-B349-C7B202D52119.htm)).
* **Location nuance worth copying:** `%I`/`%L` in a cross-reference use the **drawing-default** installation/location of the *other* drawing, not the component's own INST/LOC ([Replaceable Parameters](https://help.autodesk.com/cloudhelp/2022/ENU/AutoCAD-Electrical/files/GUID-8F803D63-4390-4617-BC71-F68AE7545D97.htm)).
* Cross-reference text lives on `XREF_LAY` (parent) / `CXREF_LAY` (child) ([WD_M](https://help.autodesk.com/cloudhelp/2022/ENU/AutoCAD-Electrical/files/GUID-75AEBC13-2F09-4B9E-B349-C7B202D52119.htm)).

### 5.2 Rules we implement

* Parent/child model: a multi-contact device has one **parent** (coil) and N **children** (contacts). Each child stores `parent_tag`; each parent stores the list of `(child_sheet, child_zone, pins)`.
* Reference string = **`{sheet}-{zone}{line}`** for cross-sheet, **`{zone}{line}`** for same-sheet, matching ACADE's `%S-%N` / `%N` split; contact pairs rendered as `13-14, 21-22` (our `ELECXREF` already emits this form, [STATUS.md](STATUS.md) §2).
* Zone computation from the sheet frame geometry (`DATUM*`, `DIST*`, `CHAR_*` equivalents), cached per sheet; recompute on `ELECSHEET`.
* Never store a cross-reference as free text alone — store `(sheet, zone, line, pins)` on the child and *derive* the label, so sheet reordering cannot silently corrupt refs.

---

## 6. PLC I/O, terminal strips, panel layout

* **PLC.** ACADE keeps PLC I/O in the project DB table **`PLCIO`** ("schematic PLC I/O address and descriptions") and a **`PLC_STYLE`** drawing setting; `MISC_FLAGS` bit 4 = "use plc wire numbers" ([Project Database](https://help.autodesk.com/cloudhelp/2025/ENU/AutoCAD-Electrical/files/GUID-8DF798D6-F71A-413E-AD1B-ABBE8A6ACB81.htm), [WD_M](https://help.autodesk.com/cloudhelp/2022/ENU/AutoCAD-Electrical/files/GUID-75AEBC13-2F09-4B9E-B349-C7B202D52119.htm)). Address **syntax is vendor-specific**; IEC 61131-3 standardises the controller's language and I/O *mapping* (variables can be "I/O Mapping – Input, Output, I/O") but not a diagram address string ([IEC 61131-3](https://en.wikipedia.org/wiki/IEC_61131-3)). So: **make the address a template**, don't hard-code it.
  Templates to ship: `I{r}.{c}` / `Q{r}.{c}` (Siemens-style `I0.0`, used by our current `ELECPLC`), `%I{r}.{c}` (IEC 61131-3-style direct variable), `I:{slot}/{bit}` (A-B-style), plus a custom template. Fields: rack, slot, channel, bit, direction, description, wire number.
* **Terminals.** Terminal = component with `strip`, `pin`, optional `TERMDESC`; strips are ordered and DIN-rail mounted (rail dimensions **IEC 60715**, [list](https://en.wikipedia.org/wiki/List_of_IEC_standards)); ACADE mirrors terminals into `TERMS` and panel terminals into `PNLTERM` ([Project Database](https://help.autodesk.com/cloudhelp/2025/ENU/AutoCAD-Electrical/files/GUID-8DF798D6-F71A-413E-AD1B-ABBE8A6ACB81.htm)).
* **Panel.** ACADE layout drawings carry footprints with **mount (`%M`)** and **group (`%U`)** assignments, stored in `PNLCOMP`/`PNLWDLEV`, with drawing scale factors `UNIT_SCL`/`FEATURE_SCL` ([WD_M](https://help.autodesk.com/cloudhelp/2022/ENU/AutoCAD-Electrical/files/GUID-75AEBC13-2F09-4B9E-B349-C7B202D52119.htm), [Project Database](https://help.autodesk.com/cloudhelp/2025/ENU/AutoCAD-Electrical/files/GUID-8DF798D6-F71A-413E-AD1B-ABBE8A6ACB81.htm)). Our `ELECPANEL` is currently a fixed 14×10 box skeleton ([STATUS.md](STATUS.md) §3 item 5) — the target is: footprint block per part, 1:1 model geometry, DIN rail as a reusable block, mount/group fields, and a generated layout drawing that *references* the schematic tags.

---

## 7. Reports a functional ECAD must produce

Column sets below are the ones a commercial ECAD in this segment actually ships — taken from SkyCAD Electrical's report templates inventoried in [STATUS.md](STATUS.md) §5 — plus the ACADE project-DB tables that feed them ([Project Database](https://help.autodesk.com/cloudhelp/2025/ENU/AutoCAD-Electrical/files/GUID-8DF798D6-F71A-413E-AD1B-ABBE8A6ACB81.htm)). These are *conventional*, not standardised columns.

| Report | Conventional columns | Our command | Data source |
|---|---|---|---|
| **BOM / parts list** | Class name · Part number · Manufacturer · Description · Qty · Where used | `ELECBOM` (+CSV) | COMP + catalog |
| **Component/parts list (per ref)** | Reference · Located in · Class name · Part number · Manufacturer · Description | `ELECLIST` / `ELECBOM --per-ref` | COMP, FILETIME |
| **Wire list** | Number · Net · Length · Wire size · Source · Destination | `ELECWIRELIST` | WNUM, COMPPINS |
| **Connection-net list** | Class name · Number · Connected elements | (new) `ELECNETLIST` | net graph |
| **Cable list** | Description · Part number · Manufacturer · Length · Conductor | (new) `ELECCABLELIST` | cable model |
| **Terminal list** | Strip · Terminal · Description · Wire (both sides) · Accessory | `ELECTERMINALS` | TERMS/PNLTERM |
| **I/O list** | Description · Connected element · Through terminal strip · Owner (address) | `ELECIO` | PLCIO |
| **Cross-reference list** | Reference · Type · Sheet/zone refs · Pins | `ELECXREF` | parent/child model |
| **Title-block / sheet list** | Sheet, drawing no., title, SEC/SUB-SEC, revision | `ELECSHEET` | FILETIME |

Implementation notes: every report must be (a) deterministic and sorted by a documented key, (b) previewable without mutation, (c) exportable as CSV, and (d) driven from the **same** in-memory domain model as the drawing — not re-scraped from text. Excel export in `ELECREPORT` should use these column sets as named templates.

---

## 8. Prior art worth studying (behaviour only, no code copying)

| Project | Licence | What it does that we should study |
|---|---|---|
| **QElectroTech** | App **GPL**; element collection **CC-BY 3.0** outside diagrams ([site](https://qelectrotech.org/), [ELEMENTS.LICENSE](https://github.com/qelectrotech/qelectrotech-elements/blob/main/ELEMENTS.LICENSE)) | 8000+ IEC-style industrial elements; element **folio/reference** model (element = ordered sequence number + text, "folio" report); built-in DXF export; terminal/ conductor representation. Best model for *industrial installation* drawings |
| **LibrePCB** | App **GPLv3**; libraries **CC0-1.0** ([licence page](https://librepcb.org/license/)) | **Semantic library** (symbol ≠ part ≠ package ≠ assembly), human-readable canonical file format for clean diffs, and a headless CLI for CI checks ([homepage](https://librepcb.org/)). Best model for part/catalog hierarchy |
| **KiCad Eeschema** | App/source **GPL**; symbol libs **CC-BY-SA 4.0 + exception** ([libraries licence](https://www.kicad.org/libraries/license/)) | Annotation flow (assign refs to unannotated symbols), netlist as the authoritative connectivity output, multi-unit symbols and alternate body styles, hierarchical sheet references. Best model for *connectivity/netlist* and ref-des assignment |
| **Horizon EDA** | **GPLv3** ([LICENSE](https://github.com/horizon-eda/horizon/blob/master/LICENSE)) | A **pool of parts** shared across projects, units/gates model, block-symbol design, and JSON file format ([site](https://horizon-eda.org/)). Best model for *pool/part reuse* and gate/unit decomposition |

Explicitly **not** copied: no source, no element files, no library data. Only behaviours and data-model ideas; QET geometry is used only under its CC-BY terms and never as ML training data.

---

## 9. What we will not claim

1. We will not call any output "IEC 60617 compliant" — only "IEC 60617-style", with a user-supplied symbol-number mapping.
2. We will not ship the IEC or IEEE 315 symbol sets (not openly licensed; IEEE 315 is inactive).
3. We will not claim ACADE project (`.wdp`/`.mdb`) compatibility or any proprietary `.dwg` custom-object fidelity.
4. We will not claim a DXF "works in ACADE" until someone validates it on a licensed seat.
5. We will not cite "IEC 61605" for terminal numbering — unverified, and possibly a mistyped standard number.
6. We will not present 81346-2:2019 letters as interchangeable with the 2009 or IEEE 315 letters.

---

## 10. Implement next — 10 items, mapped to existing `ELEC*` commands

Verification pattern for every item: run a batch through `OpenCADStudio.exe --each <batch.txt> <drawing.dxf>`, then **read the saved DXF back** (parse `BLOCK_RECORD`/`INSERT`/`ATTRIB`/`TEXT` and layer names) and assert on the parsed structure — never trust stdout alone (this is the standard used throughout [STATUS.md](STATUS.md) §2). Previews must leave the DXF byte-identical.

| # | Item | Data it needs | Where it lands | Headless verification |
|---|---|---|---|---|
| 1 | **`WD_M`-style setup block** at 0,0, invisible, with the documented subset of attributes (`SHEET`, `SHEETDWGNAME`, `IEC_*`, `UNIT_SCL`, `TAGMODE`, `TAGFMT`, `WIREMODE`, `WIREFMT`, `XREFFMT`, `ALT_XREFFMT`, `REFNUMS`, `*_LAY`). Add `ELECSETUP` and emit the block from `ELECSHEET`. | drawing properties struct; profile defaults | `ELECSHEET`, new `ELECSETUP` | batch `ELECSETUP` + `ELECSHEET A3`; reparse DXF: exactly **one** block named `WD_M` at (0,0) with ≥15 `ATTRIB`s; catch the current double-execute bug early ([STATUS.md](STATUS.md) §3 item 1) |
| 2 | **Fix `WD_*` handling**: stop treating every `WD_` block as setup; keep `WD_M`, ignore only known control blocks. Unblocks item 1 and the symbol loader. | block-name allow/deny table | `ELECLIB`, `ELECLIST` | batch `ELECLIB lib.tsv` then `ELECLIST`; assert the `WD_M` block is present and symbol count is unchanged |
| 3 | **Component attribute profile** `TAG1/TAG2/RATING1/MFG/CAT/DESC1..3/INST/LOC/FAMILY` written on insert; `ELECSETATTR` becomes a real editor (it is a stub today, [STATUS.md](STATUS.md) §4). | per-symbol attribute map in `library.tsv` | `ELECPLACE`, `ELECSETATTR` | `ELECPLACE`+`ELECSETATTR`; reparse the `INSERT`'s `ATTRIB`s for the 9 names and exact values |
| 4 | **Pin/terminal connectivity**: `TERM01..TERMnn` attributes on symbols, wire endpoints snapped to pin points, net = connected component. | pin geometry per symbol; wire graph | `ELECWIRE`, `ELECJUNCTIONS`, `ELECXREF` | build a 2-symbol 1-wire net; assert `ELECWIRELIST` yields 1 net with 2 endpoints and the junction rule still fires only on T-joints |
| 5 | **Numbering engine**: strategies `sequential`, `sheet_sequential`, `reference_based`; format templates for tag and wire (`%F %N %S %X` equivalents), increment, per-drawing start/suffix list; apply to tags **and** wire numbers. | numbering config block + net/tag graph | `ELECNUMBERPREVIEW` then `ELECNUMBER` | assert preview leaves DXF hash unchanged, apply is idempotent (second apply = no diff), and reference-based output matches `{sheet}-{zone}{line}` |
| 6 | **Zone/sheet reference service**: compute `(sheet, zone, line)` from the frame geometry; `REFNUMS`-equivalent modes (line / X-Y / X-zone). | frame geometry, datum/spacing/chars, per-sheet cache | `ELECSHEET`, `ELECXREF` | place symbols at known coordinates; assert derived zone chars/lines match hand-computed values, and survive `ELECSHEET` re-fit |
| 7 | **Cross-sheet cross-references**: parent/child child-list stored as structured data, labels derived; same-sheet vs cross-sheet formats; `XREF`/`CXREF` layers. | parent tag on children; child list on parent | `ELECXREF`, `ELECXREFWRITE` | two drawings; assert parent label contains `SHEET-ZONEn` for the remote contact and bare `ZONEn` for the local one; assert second run changes nothing |
| 8 | **Cable & core model + wire-list columns** (`Number · Net · Length · Wire size · Source · Destination`), conductor colour/gauge, `%G`-style conductor class encoded in the wire layer name. | cable entities, core table, layer-per-class | `ELECWIRE` (layer naming), `ELECWIRELIST` | create 1 cable × 3 cores; assert 3 rows with distinct core ids, correct layer names, and source/dest tags |
| 9 | **PLC I/O with pluggable address templates** (`I{r}.{c}`, `%I{r}.{c}`, `I:{slot}/{bit}`, custom), rack/slot/channel fields, `PLCIO`-style record, I/O list columns. | PLC module definition, address template, I/O records | `ELECPLC`, `ELECPLCNEXT`, `ELECIO` | generate 16 points across 2 modules; assert sequential rollover (`I0.7 → I1.0`), template round-trip parse→format→parse, and `ELECIO` columns |
| 10 | **Project database mirror** (SQLite, not Access): tables modelled on `COMP/TERMS/WNUM/PINLIST/PLCIO/PNLCOMP`, **derived from the DXFs and rebuildable at will** — the ACADE lesson is that the database is a cache, never the source of truth. Fix `ELECPROJNEW/OPEN/NUM` double-execution and add `ELECPROJREBUILD`. | per-drawing extracted records | `ELECPROJNEW/OPEN/NUM`, new `ELECPROJREBUILD` | rebuild twice from scratch; assert byte-identical DB (excluding timestamps) and that deleting the DB then rebuilding restores all report rows |

**Deferred, explicitly:** panel-layout footprints + DIN rail (`ELECPANEL`, needs §6 footprint data), catalog lookup + BOM where-used, and any `.dwg`/ACADE project output.

---

## References

Primary and secondary sources actually fetched for this brief (all links verified reachable on 2026-11-05 unless marked):

1. Autodesk, *About the WD_M block* — <https://help.autodesk.com/cloudhelp/2022/ENU/AutoCAD-Electrical/files/GUID-75AEBC13-2F09-4B9E-B349-C7B202D52119.htm>
2. Autodesk, *About Replaceable Parameters* — <https://help.autodesk.com/cloudhelp/2022/ENU/AutoCAD-Electrical/files/GUID-8F803D63-4390-4617-BC71-F68AE7545D97.htm>
3. Autodesk, *About the Project Database* — <https://help.autodesk.com/cloudhelp/2025/ENU/AutoCAD-Electrical/files/GUID-8DF798D6-F71A-413E-AD1B-ABBE8A6ACB81.htm>
4. Autodesk, *To Update Drawings From the Project Database* — <https://help.autodesk.com/cloudhelp/2025/ENU/AutoCAD-Electrical/files/GUID-EF6CD1F8-DF8D-43CA-ADCB-90D9B949EB3A.htm>
5. Autodesk, *To Create a Project-Specific Catalog Database* **[snippet]** — <https://help.autodesk.com/cloudhelp/2023/ENU/AutoCAD-Electrical/files/GUID-031AA9B4-9578-4166-A5A4-79E3B0B990AD.htm>
6. Autodesk class handout, *Configuring AutoCAD Electrical* (WD_M/WD_PNLM) **[snippet, PDF]** — <https://static.au-uw2-prd.autodesk.com/Class_Presentation_IM322678_Randy_Brunette.pdf>
7. Symetri, *Transitioning to Microsoft Access Runtime for AutoCAD Electrical 2026* — <https://www.symetri.co.uk/insights/blog/transitioning-to-microsoft-access-runtime-for-autocad-electrical-2026/>
8. Wikipedia (de), *EN IEC 81346* (aspect signs, class tables) — <https://de.wikipedia.org/wiki/EN_IEC_81346>
9. Wikipedia (en), *IEC 81346* — <https://en.wikipedia.org/wiki/IEC_81346>
10. Wikipedia (en), *Reference designator* (IEEE 315 / ASME Y14.44) — <https://en.wikipedia.org/wiki/Reference_designator>
11. Wikipedia (en), *Electronic symbol* (IEC 60617 DB; IEEE 315 inactivated 2019) — <https://en.wikipedia.org/wiki/Electronic_symbol>
12. Wikipedia (en), *List of IEC standards* (IEC 60445, 60715, 60757 titles) — <https://en.wikipedia.org/wiki/List_of_IEC_standards>
13. Wikipedia (en), *IEC 61131-3* — <https://en.wikipedia.org/wiki/IEC_61131-3>
14. KiCad, *Libraries License* (CC-BY-SA 4.0 + exception) — <https://www.kicad.org/libraries/license/>
15. QElectroTech, *ELEMENTS.LICENSE* (CC-BY 3.0 outside diagrams; no ML sample use) — <https://github.com/qelectrotech/qelectrotech-elements/blob/main/ELEMENTS.LICENSE>
16. QElectroTech project site (GPL app, 8000+ elements) — <https://qelectrotech.org/>
17. LibrePCB, *License* (GPLv3 app; CC0-1.0 libraries) — <https://librepcb.org/license/>
18. LibrePCB homepage (semantic library concept, file format, CLI) — <https://librepcb.org/>
19. Horizon EDA homepage — <https://horizon-eda.org/>
20. Horizon EDA, `LICENSE` (GPLv3) — <https://github.com/horizon-eda/horizon/blob/master/LICENSE>
21. IEC webstore search entry for IEC 60617 (paywalled DB; `std.iec.ch` returned HTTP 403 to this agent) — <https://webstore.iec.ch/en/iec-search/result?q=60617>
22. Internal: [STATUS.md](STATUS.md) — existing `ELEC*` commands, verified behaviours, SkyCAD report-template column sets.

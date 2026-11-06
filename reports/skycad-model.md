# SkyCAD Electrical 1.3.65 — object model & report contracts (clean-room extraction)

**Purpose:** recover, from the installed *shipped artifact only*, the object model and report contracts of
SkyCAD Electrical 1.3.65 so that AryaCADElectrical can implement a **superset** of them in Rust.
**Method:** static only — managed-metadata inspection of the shipped assemblies plus byte-level reading of the
shipped environment data and Excel templates. **SkyCAD was never launched**; no C# was decompiled; no SkyCAD
code or asset is copied into this repo.

> **Data-handling note.** Everything inside the SkyCAD artifacts (strings, property values, query text, template
> cells) is treated as **untrusted data**. Nothing encountered asked the reader to run, install, download, send,
> or change anything; there was no prompt-injection content. Text quoted below is evidence, not instruction.

---

## 0. Evidence registry

| ID | What it establishes |
|---|---|
| `ev_befb502ef7991dac61fe1baca445d8fec4928371bd7e1da89f4c228db165ad34` | `inspect_managed_artifact` on `SkyCadKernel.dll` — PE32/x86, IL-only, .NET Framework 4.5, assembly **SkyCadKernel 1.3.65.17278**, SHA-256 `24d309464684f6a984b9697d24fbffcf846c03eb288039141a7fedc48ed3d34d`, MVID `98e5ff9a-7ce8-4024-b6e7-8a29294a445e`, metadata tables: **436 TypeDef / 3,062 Field / 9,168 MethodDef / 3,464 Property / 2,362 MemberRef / 433 TypeSpec** |
| `ev_031cdfbd261ed37e0413cc7123f546318c2c39691c026ba9717c51a65a19d9a9` | `inspect_managed_members` (all types) — type inventory admitted; member decode hit the 64 MiB representation budget (coverage *partial*, documented) |
| `ev_6d637048963cf06a6628a94b8a88befec0130f5d56be4a61e97de944410aeff0` | full TypeDef list page: 436 types incl. 428 non-compiler types; names used throughout this report |
| `ev_c45592c7be3acf99cbbe532e866e15b4a29714e6011fd033026b98729d5dc0f6` | `inspect_managed_members` selection `{SkyCadEnvironment, SkyCadClass, SkyCadClassCatalogue, SkyCadPropertyDefinition, SkyCadCompositionLink, SkyCadCompositionRule}` — 183 fields / 549 methods / 2471 `ldstr` user strings |
| `ev_956ab1011000bc5bbc70e77a45a80ec65001f8efd23f6d076bdadd330e4bbc77` | field rows for that selection (names, field types, constants) |
| `ev_4f8d2680f7d1e39a6faa90eedcc2d472a154895ca65d749e76df7b8c9b29f073` | same-selection summary (counts/limitations) |

Shipped data paths (read-only):

* `C:\Program Files\SkyCAD Electrical\SkyCadKernel.dll` (5,624,832 B) — object model.
* `C:\Program Files\SkyCAD Electrical\SkyCadControls.dll` (2.81 MB), `SkyCadViewer.dll`, `SkyCadPdfGenerator.dll`, `WW.Cad.dll`/`WW.dll`/`WW.License.dll` (commercial .NET DWG library) — not analysed beyond confirming managed PE.
* `C:\SkyCAD Environments\Standard Environment\` — 2,241 files / 286 MB.
  * `EnvironmentStructure.SkyCadFile` — **9,387,009 B**, the whole class/property/query/export environment graph.
  * `Catalogue\` — **1,646 `*.SkyCadFile`** (one extension, upper/lower case), folder tree = class tree.
  * `Template files\` — **10 report `.xlsx` + 1 I/O import `.xlsx`** = 11.
  * `Icons\` 45 files, `Images\` 517, `TimeMachine\` 2, `Collaborative\` 1.

---

## 1. The class model

### 1.1 What a "class" is

A class **is an object**, with a name, a parent class, a catalogue, property definitions, composition rules,
queries and (optionally) graphical blocks. The environment file contains exactly **240 `SkyCadClass` objects and
240 `SkyCadClassCatalogue` objects** (evidence: type histogram of `EnvironmentStructure.SkyCadFile`).

Class record anatomy (observed token stream at offsets ~294,132 / ~320,324 / ~322,712):

```
SkyCadClass%<classGuid>/
   <Setting> "Name" = "Root Class"          ← display name, also the catalogue folder name
   <Setting> "Name" = "SkyCad specific Classes"
   <Setting> "Name" = "Work field classes"
```
plus composition links to: parent class, sub-classes, `SkyCadClassCatalogue`, `SkyCadPropertyDefinition`s,
`SkyCadCompositionRule`s, `SkyCadExplorerQuery`s, icon/library data.

Consequences of "a class is an object":
* the catalogue **folder path is the class path** (`…\Catalogue\root class\SkyCad specific Classes\Net\Cable\`),
* sub-classing is stored as class→class composition links (21,311 composition links exist in the file),
* every catalogue part file belongs to the class of its folder.

### 1.2 Property definitions (`SkyCadPropertyDefinition`) — 644 in the shipped environment

Fields recovered by parsing every property-definition record in `EnvironmentStructure.SkyCadFile`
(example records at offsets ~299,356 / ~306,345 / ~313,340 / ~326,765):

| Field | Meaning | Observed values |
|---|---|---|
| `Name` | property label | e.g. `Part number`, `Tag`, `Address`, `Coil Voltage`, `Wire size` |
| `Type` | value type | counts: **Text 294, Boolean 142, Measurement 82, Integer 57, Color 18, Link 15, SkyCadObject 15, Angle 6, Image 5, Date 4, Decimal 4, LibraryObject 2** |
| `Description` | help text | usually empty |
| `Owner` | evaluator for "owner" display | `<r:GetParent.GetParent.DisplayedLabel>` |
| `ListType` | picker behaviour | `None`, `Forced` |
| `List` | allowed values | name of a `SkyCadListOfValues`, or expression `<r:GetEnvironment.GetConnectionPortGroupTypes>` |
| `Visible` | shown in grid/UI | `TRUE`/`FALSE` |
| `Read Only` | editable? | `True`/`False` |
| `Priority` | which UI context | `Context`, `Catalogue` |
| `Default Value` | default | e.g. `Open`, `<r:Class.Name>`, `256` |
| `Initial value` | first value expression | `<r:Name><#>` style |
| `Force initial value as default value` | bool | `FALSE` |
| `Configurable` | overloadable via options | `TRUE` |
| `Mapping operation` | copy/merge mapping | `None` |
| `Root query` | query returning candidate objects (for object-valued props) | `GetEnvironment`, `GetProjectElements(Class:"Function")` |
| `Explorer query name` | named query for the picker | `Numbering Formats`, `Sheets` |

The `Priority`/`Visible`/`Read Only` triple is the UI contract; `Root query` + `Explorer query name` is the
**value-provider contract** for object-valued properties (an ECAD property is often a reference into the
environment or the project).

### 1.3 Object identity, composition links, and the serialization model

`SkyCadCompositionLink` (metadata fields `_Child`, `_Parent`, `_ParentDictionaryindex`, `_ChildDictionaryIndex`,
`_ChildID`, `_ParentID`) is the **only** parent→child ownership edge. Instance data confirms 21,311 of them in
the environment file.

Objects are identified by **GUID strings embedded in the token stream**, not by .NET object references.
Stream shape (byte-level): a `.NET BinaryFormatter` stream whose payload is a *flat sequence* of
length-prefixed strings —

* `SkyCadKernel.<Type>` — an object header (its GUID follows in the next token `%<guid>/`),
* `%<guid>/` — a reference token,
* `name`,`value` pairs — settings (e.g. `'Name'`, `'Start Cell'`).

Verified framing: at byte offset 1468201 the bytes immediately before
`GetQuantity(ReferenceObjectQuery:GetLastUsedPartsQueriedObject)` are `00 00 00 3f` → `0x3F` = 63 =
the UTF‑8 length; object headers use a 1-byte 7‑bit length (`0x22` = 34 = `strlen("SkyCadKernel.SkyCadCompositionLink")`).
**Therefore the leading characters seen in string dumps (`?`, `&`, `%`, `!`, `#`, `,` …) are BinaryFormatter
length prefixes, not semantics** — quoted query text below is given with that prefix removed.

Parsed counts for `EnvironmentStructure.SkyCadFile` (my parser; object headers are unique GUIDs):

| Type | Count | Role |
|---|---|---|
| `SkyCadSetting` | 18,990 | one named value on one object |
| `SkyCadCompositionLink` | 21,311 | ownership edge |
| `SkyCadPropertyDefinition` | 644 | schema |
| `SkyCadClass` / `SkyCadClassCatalogue` | 240 / 240 | class tree |
| `SkyCadExplorerQuery` | 278 | saved queries |
| `SkyCadExplorerQueryCell` | 214 | report/grid column definitions |
| `SkyCadExcelExport` / `SkyCadExcelCellValue` | 18 / 27 | report export definitions |
| `SkyCadCompositionRule` | 106 | "what may be created under this class" |
| `SkyCadCardinalityRule` | 17 | relation kinds |
| `SkyCadRelationalLink` | 51 | typed relations (environment level) |
| `SkyCadNumberingFormat` | 26 | number generation templates |
| `SkyCadTextTemplate` / `SkyCadPropertyQuerytextTemplate` | 27 / 3 | text/title-block formats |
| `SkyCadCustomQuery` | 11 | user queries |
| `SkyCadSystemQueryFolder` | 19 | query tree folders |
| `SkyCadListOfValues` | 7 | pick-lists |
| `SkyCadQueryDocumentation` | 117 | query catalogue/help entries |
| `SkyCadCustomRibbonItem` | 12 | ribbon customisation |
| `SkyCadSequence` | 131 | ordering |
| `SkyCadEnvironment` | **1** | singleton root, GUID `f10078ea-3d25-4f09-9de5-9a28012601a3` |

Every object graph node therefore has: `{type_name, guid, owner_guid, ordered settings, ordered children}`.
That is exactly the minimum a reimplementation needs.

### 1.4 Relational links and the 17 relationship kinds

Besides composition, objects relate through **`SkyCadRelationalLink`** (51 in the environment, 2,348 in the
example project) typed by a `SkyCadCardinalityRule`. The shipped cardinality rule *names* are the relationship
vocabulary (all 17, offsets in the parse of the environment file):

```
Represents · ProcessRenumberingWith · GroupedIn · ImplementedBy · Locatedin · ConnectedTo ·
CorrespondsTo · LaysOn · LayoutInstanceOf · LayoutReprentationOf · SplitWith · WiredWith ·
RoutedThrough · LugConnection · IsIncludedIn · IsReferenceOf · FromSystem
```

These names are used as **filter keywords in query expressions**, e.g.
`GetLinkedObjects(Direction:To;Cardinality:GroupedIn)`. This is how "a component's pins", "a wire's endpoints",
"a terminal's strip", "a layout's component" are all expressed without special-casing classes.

### 1.5 Queries (`SkyCadExplorerQuery`) — 278

A query is a *named, reusable selector* rendered as a tree or grid. Fields (all 278 decoded):

| Field | Example |
|---|---|
| `Name` | `BOM`, `Wire list`, `IOs`, `Cable list`, `Parts list`, `Connection nets`, `Sheet sizes` |
| `Query` | selector expression, e.g. `GetUsedParts(SortBy:CatalogueID)`, `GetAllWires`, `GetEquipotentials`, `GetAllCables(SortBy:Name)`, `GetIOs`, `GetClasses`, `GetPropertyDefinitions(Where:Displayed=True)` |
| `Description` | e.g. `Displays bill of materials.` |
| `Icon` | `BOM`, `WireList`, `NetList`, `CablesList`, `io`, `ClassTreeLarge`, … (file in `Icons\`) |
| `Ribbon Tab`, `Ribbon Tab Section` | `Show` / `Lists`, `Show` / `General`, `Configuration Management` / `Queries` |
| `Show As Grid` | `TRUE` for reports; `FALSE` for trees |
| `Column order and display` | full ordered column spec (see 1.6) |
| `Sorting column` | `Number:Ascending`, `Part number:Descending` |
| `Row moving function` | `MovePropertyDefinition`, `MoveOption`, `MoveCableWire`, `MoveSheetClass` — drag-reorder hook |
| `Show index as row value`, `Expand tree`, `ShowSubColumnHeaders`, `Use extended label`, `Hidden`, `Secondary query name`, `Skip Selected Row Sorting` | rendering options |

`SkyCadCustomQuery` (11 instances) is the user-authored variant: same name/expression/render options, so the
query system is an **end-user programmable report engine**, not hard-coded reports.

### 1.6 Query-cell = column, and the column display specification

Two complementary mechanisms define a table's columns:

**(a) `SkyCadExplorerQueryCell`** (214) is a *computed column*:
`Name` (column label) + `Query` (expression evaluated per row) + `Column Header Text` (override) + `Allow edit`.
Examples actually shipped (Query string with BinaryFormatter prefix stripped):

| Query owner | Column `Name` | Column `Query` expression |
|---|---|---|
| Wire list | `Source` | `GetSourceConnectedLabel` |
| Wire list | `Destination` | `GetDestinationConnectedLabel` |
| Wire list | `Net` | `GetAssociatedConnections(Item:0).GetNet.DisplayedLabel` |
| Wire list | `Source location` | `GetSourceConnectedPort.LocatedIn` |
| Wire list | `Destination Part No.` | `GetDestination.TopMostComponent.PartNumber` |
| Wires (cable conductors) | `Wire number` | `GetAssociatedConnections(Item:0).GetDisplayedNumber` |
| Wires | `Sub-cable` | `GetCable.GetExtendedSubCableName` |
| BOM / Harness BOM | `Qty` | `GetQuantity` / `GetQuantity(ReferenceObjectQuery:GetLastUsedPartsQueriedObject)` |
| BOM / Harness BOM | `Where used` | `WhereUsed(SEPARATOR:/)` / `WhereUsed(ReferenceObjectQuery:GetLastUsedPartsQueriedObject;Separator:/)` |
| Parts list | `Reference` | `DisplayedLabel` (or `WorkID`) |
| Parts list | `Cross Reference` | `GetNavigationObjects(Where:GetClass.IsSubClassOf(Class:"Work block");Item:0).SheetPosition` |
| IOs | `Owner` | `Parent.DisplayedLabel(ExtendedLabel:True)` |
| IOs | `Connected element` | `GetConnectedElement.DisplayedLabel` |
| IOs | `Through terminal strip` | `GetThroughTerminalStrip.DisplayedLabel` |
| IOs | `Terminal Label` | `GetGroupedPorts(Separator:/)` (header override `Terminal Label(s)`) |
| Elements to layout | `Part number` / `Manufacturer` / `Part description` | `Parent.PartNumber` / `Parent.Manufacturer` / `Parent.Description` |
| Cable list | `Cross Reference` | `GetNavigationObjects(Item:0).GetSheetPosition` |
| DIs/AIs/AOs | `Card` | `ParentCard.WorkID` |
| Table of content | `Folder` / `System` | `GetFolder.Name` / `GetParentSystem.WorkID` |

**(b) `Column order and display`** — a serialized ordered list of *every* column with its physical index:

```
Name:<column>;Index:<0-based>;Hidden:<True|False>;Inclusive:<TRUE|FALSE>;BridgeColumn:<False|True>
```

rows are grouped per owner query inside one setting value delimited by
`<Start setting for:<QueryName> … <End setting for:<QueryName>` (observed in 432 marker pairs, 6,065 column
specs in the environment file; offsets e.g. `Name:Where used;Index:16;…` @1,308,741 and @4,857,827).
`Inclusive:TRUE` marks a column that also acts as a group/bridge header (stacked terminal views).

**The query language** is a dotted path/evaluation language. Recovered vocabulary:
object navigation (`GetParent`, `GetProject`, `GetEnvironment`, `GetClass`, `GetClass.GetRoot`,
`GetLinkedObjects`, `GetWorkObjects(Class:…)`, `GetProjectElements(Class:…)`, `GetNavigationObjects`,
`GetChildren`, `GetWorkblocks`), typed traversal (`GetConnectedPorts`, `GetPorts`, `GetGroupedPorts`,
`GetConnectedWires`, `GetAssociatedConnections`, `GetAllWires`, `GetAllCables`, `GetNet`,
`GetSource`, `GetDestination`, `GetSourceConnectedPort`, `GetCable`, `GetSheetPosition`,
`GetDisplayedNumber`, `GetExtendedSubCableName`), aggregates (`GetQuantity`, `WhereUsed`, `GetUsedParts`,
`PartsList`), predicates/parameters `Where:`, `Item:`, `SortBy:`, `Separator:`, `EXTENDEDLABEL:`,
`Direction:`, `Cardinality:`, `Class:`, `Scope:`, `ForSystems:`, and `GetClass.IsSubClassOf(Class:"Work block")`.
The same `<r:…>` template form is used in property defaults and numbering formats (`<r:GetClass.GetRoot><#>`,
`<r:GetParent.GetParent.DisplayedLabel>`, `<r:IF(…)`), i.e. **one expression engine serves queries, property
values and number formats**.

### 1.7 Composition rules — the "create menu" contract

106 `SkyCadCompositionRule` objects; fields: `Class Name`, `Description` (`Creates a new <r:Parent.ClassName>.`),
`Editor query` (which query populates the picker), `Icon`, `Allow user creation`, `Ribbon Tab`,
`Ribbon Tab Section`, `Disallow as child`, `Hidden`.
Examples: `Wire → Editor query "Wires"`, `Cable → "Sub-cables"`, `Accessory → "Accessories"`,
`Sheet → "Sheets"`, `Option → "Sub-Options"`. This is how the GUI decides **what can be inserted under what**,
which query lists candidates, and which icon/ribbon slot to use — a data-driven UX contract.

### 1.8 The class tree (240 classes)

Complete shipped tree (indent = subclass). This table is the single most reusable artifact: it is the
modelling skeleton of a commercial ECAD.

```
Root Class
├ SkyCad specific Classes
│ ├ Stored configuration
│ ├ Revision → Revision branch, Internal revision
│ ├ Option
│ ├ Library
│ ├ Connection Port → Bridge Connection Port
│ ├ Connection port group → Terminal junction (→ Ground terminal junction), I-O, Connector pin
│ ├ Net → Power, Control, Control - line numbering, Cable (→ Twisted group, Molded Cable),
│ │        Cable shield, Harness, IO Connection, Single line, Ground Wire
│ ├ Connection point group → Coil, PWR - NO, NO - Contactor, NC - Contactor, Junction (→ Ground junction),
│ │        NO, NC, PLC IO (→ Digital input, Digital output, Analog input, Analog output, Relay Output),
│ │        Feed common, Return common, Generic terminal (Left), Generic terminal (Right),
│ │        Communication port (Left), Communication port (Right), Pin, Splice junction
│ ├ Graphical Element → Construction Point (→ Connection Point, Sizing handle),
│ │        Graphical Object → Drawing Object (Line Segment, Arc, Closed Drawing Object →
│ │            Rectangle object (→ Image object), Circle (diameter), Circle (radius), Polyline (→ Revision cloud),
│ │            Spline, Ellipse), Dimension), Text (→ Cross Reference, Text template (→ Property query text template),
│ │            Title block coordinates settings), Sheet (→ 14 sheet-size classes: ANSI A-E/Letter/Legal,
│ │            ISO A0-A4/Portrait/B1), Graphical grouping, Graphical block (→ Title block, Work block,
│ │            List item (→ Terminal list item), Connection, OPR (→ 4 orientation classes + Multicore cable
│ │            junction), Association block, Cross-reference block, Connection point block),
│ │        Layout instance (→ Stretchable Layout instance), Layout representation
│ ├ Oriented Connection Point
│ ├ Folder, Layout layer, Library class folder
│ ├ Value listing classes → Title block attributes referenceable classes →
│ │        Project attributes, Folder attributes, Sheet attributes, Revision attributes, Title block system attributes
│ ├ Net branch → Harness branch, Single wire
│ └ Block mapping rule
└ Work field classes
  ├ Project
  ├ Location
  ├ Function
  ├ Component → Button (→ EStop), Circuit Breaker, Fuse holder, Selector switch, Overload, Indicator,
  │   Motor, Transformer, Contactor, Disconnect, Drive, Sensor (→ Level sensor, Limit switch, Photocell,
  │   Temp. switch, Pressure switch, Pressure transmitter, Proxy Switch (→ Ultra-sonic proxy.),
  │   Thermocouple & RTD, Float Switch, Encoder, Temp. Transmitter), HMI, Networking, Power supply, Relay,
  │   Safety Relay, Connector, Enclosure Light, PLC (→ PLC Coupler), PLC Module (→ DI/AI/AO/DO Module),
  │   Soft Starter, Fan, Solenoid valve, Alarm, Ammeter, Buzzer, Potentiometer, Line Reactor, Voltmeter,
  │   Surge Arrester, Current Transformer, Power Distribution Block, Ground Bar, Instrument Ground Bar,
  │   Foot switch, Capacitor, Fuse Switch Disconnect, Timer, Harness element (→ Harness boot, Sleeving,
  │   Grommet, Cable tie, Harness Label), Generator, Flying Lead
  ├ Terminal Strip
  ├ Accessory → Contactor/Pilot device/Enclosure/Disconnect/DIN rail/Fuse/Relay/Terminal block/
  │   Wire management/Overload/Harness/Circuit Breaker accessory
  ├ Terminal Strip Element → Terminal, Separator, End stop, Group marker
  ├ Connectivity → Wire (→ Shield), Termination lug (→ Unterminated wire, Harness terminal connection,
  │   Harness connector pin), Splice, Ground indicator
  ├ Rail, Wire duct, Ground, System, Parameter, Network Interface Setting
```

(Work-block sub-classes — the graphical side of the same domain — are
`Graphical terminal junction, List header (→ TS Header), Graphical PLC Module, Graphical I-O, Layout block,
Stretchable layout block, Graphical connector pin, Cable indicator, List block, Graphical lug, Harness block,
Harness boot block`.)

### 1.9 On-disk layout (what is data vs code)

```
Standard Environment\
  EnvironmentStructure.SkyCadFile   class tree + 644 property defs + 278 queries + 214 column defs
                                    + 106 composition rules + 18 report exports + 26 numbering formats
                                    + templates + 18,990 settings + 21,311 composition links
  Catalogue\<class\subclass\…>\<Part>.SkyCadFile   1,646 catalogue instances
        root class\SkyCad specific Classes\Library\ 3 symbol libraries (AUS, IEC, generic; 1.2–1.5 MB each)
        root class\work field classes\Project\      6 example projects (IEC, IEEE, Aus_NZ, Harness,
                                                    Options, Systems; 1.3–9.3 MB each)
  Template files\*.xlsx                           10 report layouts + 1 I/O import layout
  Icons\ (45)  Images\ (517, incl. *_Layout.png panel footprints)  TimeMachine\ (2)
```

Note for the record: the six example projects **are present** on this machine
(`…\Project\IEC Example Project.SkyCadFile` 8,776,036 B etc.), correcting `STATUS.md` §5 which says they were
absent after install.

---

## 2. Domain model for electrical

### 2.1 Component

**Class:** `Component` (under *Work field classes*), 8 own properties + subclasses.

Properties: `Tag`(Text), `Part number`(Text), `Manufacturer`(Text), `Located in`(Text), `Web`(Link),
`Assembly`(Boolean), `Quantity`(Integer), `Tag frozen`(Boolean).
Subclasses add their engineering data, e.g. `Motor`: `Type, Power, Voltage, Speed, FLA, Frequency, Label`;
`Contactor`: `Label, Current, Coil Voltage`; `Circuit Breaker`: 3 props; `Fuse`: 5 props.

**What a component instance actually is** (catalogue file composition, parsed token histogram):
`…\Component\Contactor\SC-E02-24VAC.SKYCADFILE` (100,574 B) =
`SkyCadWorkObject 1` + **`SkyCadProperty` 95** (the part's property values) + **`SkyCadConnectionPort` 18** +
**`SkyCadConnectionPortGroup` 9** + `SkyCadLayoutBlock 2` + `SkyCadLayoutBlockLayer 6` +
`SkyCadImageObject 2` + `SkyCadRelationalLink 21` + `SkyCadRevision`/`SkyCadOption`/`SkyCadConfigurationTools`.
So: **a component = one work object + a property bag + ports grouped into port groups + layout blocks (one per
view/panel representation) + image + revision/option wrappers**. The schematic symbol itself is a `SkyCadWorkBlock`
attached by links; the panel footprint is a `SkyCadLayoutBlock`.

**Connection points vs connection ports** (terminology that matters):
* `Connection Point` (subclass of `Construction Point`) — a *geometric* point on a symbol that can connect.
* `Connection Port` / `Connection port group` — the *logical* port objects (`Connection Port` has
  `Name, Type, Connected wire numbering format, Label`; `Connection port group` has
  `Flow Through, Type, Name, Function, Port Type`). Kinds: coil, NO, NC, NO-contactor, NC-contactor, junction,
  PLC I/O (digital/analog/relay), feed/return common, generic terminal L/R, communication L/R, pin, splice.
* `Connection point block` — a graphical block class (105 shipped catalogue blocks = terminals/lugs/etc.).

### 2.2 Net / wire / equipotential

* `Net` — 4 props: `Number`, `Tag frozen`, `Required Gauge`, `Color`; subclasses Power, Control,
  Control - line numbering, Cable, Cable shield, Harness, IO Connection, Single line, Ground Wire.
* `Net branch` — 2 props `Number`, `Color`; subclasses `Harness branch`, `Single wire` (10 props).
* `Wire` (under `Connectivity`) — 8 props: `Number`, `Wire size`, `Structure`, `Material`, `Length`,
  `Part number`, `Manufacturer`, `Tag frozen`; subclass `Shield`.
* Runtime connectivity is held by `SkyCadEquipotential` (the logical net node; 133 in the IEC example project)
  plus `SkyCadSingleWire`/`SkyCadConnection` graphical objects and `SkyCadRelationalLink`s
  (`ConnectedTo`, `WiredWith`, `RoutedThrough`). The graph is physical-graphical ↔ logical-net via ports.

### 2.3 Terminal and terminal strip

* `Terminal Strip Element` (5 props `Part number, Manufacturer, Assembly, Web, Quantity`) →
  `Terminal` (+1), `Separator` (4), `End stop` (4), `Group marker`.
* `Terminal Strip` (14 props): `Name, Side A label, Side B label, Invert sides, Located in, Color,
  Filter by locations, Show unresolved connectivity info, Show '(N, Allow terminal numbering, Quantity,
  Level labels, 2Prefix terminal location path on connectivity info`.
* `Terminal junction` (6 props): `Type, ConnectedSideA, ConnectedSideB, Tag frozen, Side A Connected Wires,
  Side B Connected Wires`; subclass `Ground terminal junction`; graphical twin `Graphical terminal junction`.
* `Bridge Connection Port` (terminal jumper/bridge), `Termination lug`, `Splice`, `Ground indicator`.

### 2.4 Cable and conductors

* `Cable` is a **Net subclass** with 17 props: `Name, Part number, Manufacturer, Number, Image, Virtual part,
  Web, Quantity, Wire numbering type, Flip, Length, Conductor size, List of values for wire numbering,
  Displayed in catalogue, Twisted group, Show Work ID value on sub-parts, Function`.
  Subclasses: `Twisted group`, `Molded Cable`; also `Cable shield` (as Net) and `Cable indicator` (graphical).
* **Conductors are `Wire` children of the Cable**: `…\Net\Cable\CF30-40-04-1.SkyCadFile` (14,763 B) contains
  `SkyCadCable 1` + **`SkyCadWire 4`** + `SkyCadProperty 15` — a 4-core cable with per-core wire records.
* `Multicore cable junction` (OPR subclass) is the graphical cable-break symbol; `SkyCadMultiCoreCableJunction`
  exists in the kernel and in projects.

### 2.5 Cross-reference

* `Cross Reference` is a **Text** subclass with `Name`, `Value`, `Type`(SkyCadObject) — i.e. the cross-reference
  is a text object bound to a type; `Cross-reference block` (graphical block, 1 prop) is its symbol.
  Shipped blocks: `NO Cross-ref block`, `NC Cross-ref block`, `Pwr NO Cross-Ref block`, and IEC variants (5 files).
* The report column expression is `GetNavigationObjects(Where:GetClass.IsSubClassOf(Class:"Work block");Item:0)
  .SheetPosition` — reference = *sheet position of the navigation object*, confirming that cross-references are
  derived from sheet/row/column position, not stored prose.
* Title block supplies the format: `Cross reference format` (Title block property).

### 2.6 Title block and sheet

* `Title block` (32 props) holds: `Name, Show coordinates, Coordinates width/inside line/separators/font/font size/
  bold/italic/strikeout/underlined, Cross reference format, Row separation method, Row fixed height/number,
  Row numbering type, Row display order, Column separation method, Column fixed width/number,
  Column numbering type, Column display order, Use line numbers, Row top/bottom offset, Title block position,
  Overlap coordinates, Scale, Row normalising character, Row number start value, Column number start value,
  (Isolate numbering by rung number section`.
* `Title block coordinates settings` (20 props) is the template-field object: `Value`, `Name`, `Description`,
  `Angle`, `Alignment`, `Scale`, `Displayed`, `Word wrap`, `Max width`, `Background object`, `Sub-selectable`,
  `Draw contour`, `Contour color/width/line style`, `Fill`, `Fill color`, `Opaque background`,
  `Row top/bottom offset`.
* Shipped title-block blocks: `A4TitleBlock.SkyCadFile` (428,957 B: 484 `SkyCadProperty`, 47 `Text`, 16 line
  segments, 87 construction points, 1 `TitleBlockCoordinatesSettings`), `LetterTitleBlock`.
* `Sheet` (12 props): `Number, Title, Description, Width, Height, Displayed, Angle, Scale, Background object,
  Sub-selectable, Tag frozen, Cover Page`; 14 sheet-size subclasses for ISO/ANSI sizes.

### 2.7 PLC module and I/O

* `PLC` → `PLC Coupler`; `PLC Module` (2 props `Prefix, Slot`) → `DI/AI/AO/DO Module` (1 each).
  Graphical twin `Graphical PLC Module` (14 props: `Orientation, AutoDraw connection, Feed/Return offset,
  Feed, First/Second terminal position offset, Symbol position offset, AutoCreation Channel spacing, contour…`).
* `I-O` (16 props): `Address`, `Requires terminals`, `Connected to`(LibraryObject), `Type`, `Name`,
  `Module block`(LibraryObject), `Required item tag`, `Required wire numbers`, `Required Location`,
  `Required Owner PLC`, `Required Module`, `Required Junction Box`, `Required Terminals`, `Description`,
  `Required location for connected item`, `Flow Through`.
  This is the schema that the *IO import* template feeds (`I/O Type`, `Description`, `Requires Terminals`,
  `Address` — see §3.11) and the `IOs` report exports.
* Runtime objects: `SkyCadPLCIO` (kernel), `SkyCadIO`, `SkyCadGraphicalIO`, `SkyCadIOImporter`,
  `SkyCadStoredIOImporterColumnIndex`.

### 2.8 Panel / layout

* `Layout block` (22 props): `Width, Height, Insertion point X/Y offset, Active layer, Left/Right/Top/Bottom
  clearance, contour props, Expand query, Direction, Show dimensions, Scale, Opaque background, Angle,
  Use background color, Background color, Pin-out block`.
* `Layout instance` (`Scale`, `Enforced Parent`), `Layout representation` (`Represented layout`, `Scale`,
  `Show drillhole pattern`, `Show dimensions`, paddings, `Partial layout view`),
  `Stretchable layout block/instance`, `Layout layer`.
* Panel-level work objects: `Location` (10 props: `Name, Part number, Manufacturer, Requires junctions,
  Assembly, Located in, Web, Quantity, Tag frozen, Try merge on System use`), `Rail` (5 props),
  `Wire duct` (4 props), `System` (8), `Accessory` (7).
* Shipped panel footprints: `Images\*_Layout.png` (517 images), `Layout block` catalogue entries under
  `graphical block\work block`.

### 2.9 Project / folder / function / revision

`Project` (12 props: `Name, Number, Date, Approved by, Default sheet size, Modify built system, Dimension,
Virtual part, Designed by, Displayed in catalogue, Quantity, Allow for sheet punch-hole margin`),
`Location` (10), `Function` (2: `Name, Virtual part`), `Revision` (4: `Number, Status, Date, Approved by`),
`Revision branch` (3), `Option` (4: `Name, Description, Option family, Highlight color`),
`Folder`. Project runtime objects in the IEC example: `SkyCadSheet 19`, `SkyCadTitleBlock 19`,
`SkyCadListBlock 1` + `SkyCadListBlockColumn 82`, `SkyCadWorkBlock 48`, `SkyCadSingleWire 229`,
`SkyCadEquipotential 133`, `SkyCadTerminal 55`(`SkyCadTerminal 3` distinct records parsed),
`SkyCadTerminalStrip 4`, `SkyCadCable 2`, `SkyCadRail 4`, `SkyCadWireDuct 6`, `SkyCadGround 3`,
`SkyCadOPR 4`, `SkyCadLayoutInstance 64`, `SkyCadGraphicalPLCModule 2`, `SkyCadPLCModule 2`.
*(Instance counts differ by parser depending on whether duplicate serialized records are folded — treat them as
order-of-magnitude, not exact.)*

### 2.10 In-drawing tables (`List block`)

`List block` (9 props: `Source object query`, `Query`, `Sorting query`, `Sort ascending`,
`AutoCreation Channel spacing`, `Font color/name/size`, `Representation block`) + `List block column`
(`Header text`, `Query`, `Width`) is the mechanism that draws report tables **inside a sheet** (BOM/wire-list
tables on drawings). 82 columns exist in the IEC example project. This is a second consumer of the same
query/column model as the Excel exporter — strong evidence that "report" and "table on drawing" share one
contract.

---

## 3. Report contracts (11 templates)

### 3.0 The export mechanism (recovered)

Each report is a `SkyCadExcelExport` object (18 shipped) with:

| Setting | Meaning | Example |
|---|---|---|
| `Template file` | path to the `.xlsx` layout | `\Template files\BOM.xlsx` |
| `Start Cell` | **top-left cell of the exported table** | `A7` |
| child `SkyCadExcelCellValue` ×N | fixed header cells: `Name`, `Cell address`, `Cell value` (expression) | `Project Name` → `E2` → `GetRootObject.DisplayedLabel` |

The variable table starts at `Start Cell`; its columns are the owner query's columns in `Index` order
(the `Column order and display` specification). **`Start Cell` coincides with the header row of every
project-report template**, including `CableSchedule.xlsx`, which contains *no* header row at all (its header is
generated by the export) — evidence that the export writes header + rows beginning at `Start Cell`.
`Hidden:True` columns are skipped when writing (proved by `Location Parts List`, where the visible columns in
`Index` order are exactly the template's A–F headers; and by BOM). `Column Header Text` overrides a column label.

Fixed header cells recovered verbatim (18 definitions walked in `EnvironmentStructure.SkyCadFile`):

| Export | Template | Start | Fixed cells |
|---|---|---|---|
| BOM | `BOM.xlsx` | A7 | `E2` Project Name = `GetRootObject.DisplayedLabel`; `E3` Export date = `Today` |
| Partslist | `Partslist.xlsx` | A7 | `F2` Project Name; `F3` Export date |
| CableList | `CableList.xlsx` | B7 | `E2` Project Name; `E3` Export date |
| CableSchedule | `CableSchedule.xlsx` | A7 | `E2` Project Name; `E3` Export date; **`A5` Cable Name = `DisplayedLabel`** |
| ConnecttionList (sic) | `ConnecttionList.xlsx` | **B8** | `E2` Project Name; `E3` Export date |
| IO List | `IO List.xlsx` | A7 | `D2` Project Name; `D3` Export date |
| Component IO List (a) | `Component IO List.xlsx` | A7 | `D2` Project Name; `D3` Export Date; `D4` Component Name = `DisplayedLabel` |
| Component IO List (b) | `Component IO List.xlsx` | A7 | same (second class context) |
| Location BOM | `Location BOM.xlsx` | A8 | `E2` Project, `E3` Date, `E4` Location = `DisplayedLabel(ExtendedLabel:True)` |
| Location Parts List | `Location Parts List.xlsx` | A7 | `F2` Project, `F3` Date, `F4` Location |
| WireList (a) | `WireList.xlsx` | B7 | `E2` Project Name; `E3` Export date |
| WireList (b) | `WireList.xlsx` | B7 | (second class context, no fixed cells recovered) |

### 3.1 BOM — `Template files\BOM.xlsx`

* Sheet `Feuil1`, used range `A1:J34`; merged `A1:C4`, `E2:F2`, `E3:F3`, `E4:F4`, `A5:F5`.
* Title `A5` = `Bill of Material`; header row **7**: `A7 Class name · B7 Part number · C7 Manufacturer ·
  D7 Description · E7 Qty · F7 Where used`.
* Export anchor `A7`; `E2` project, `E3` date.
* Data columns (owner query `BOM`, `GetUsedParts(SortBy:CatalogueID)`, `Show index as row value TRUE`,
  column 0 `Class name` visible; hidden columns 4 `Image`, 5 `Web`):
  `0 Class name · 1 Part number · 2 Manufacturer · 3 Description · 6 Qty · 7 Where used`
  → visible order = **Class name, Part number, Manufacturer, Description, Qty, Where used**.
* Column semantics: `Qty` = `GetQuantity` (count of the part across the project);
  `Where used` = `WhereUsed(SEPARATOR:/)` — a `/`-separated list of the locations/references using the part
  (the "where used"/grouping logic, derived — not a stored field).
* Row pattern: one row per **used catalogue part** (grouping key = catalogue part = class + part identity),
  sorted by `CatalogueID`; quantity aggregated; where-used aggregated; `Project`/`Date` fixed once in row 2–3.

### 3.2 Parts list — `Template files\Partslist.xlsx`

* Sheet `Feuil1`, `A1:J33`; merged `A1:D4` (logo), `A5:F6` (title); title `A5` = `Parts List`.
* Header row **7**: `Reference · Located in · Class name · Part Number · Manufacturer · Description`.
* Export anchor `A7`; `F2` project, `F3` date. Freeze panes `A8` (data from row 8).
* Owner query `Parts list` = `PartsList(SortBy:DisplayedLabelV2)` (project variant) /
  `PartsList(SortBy:WorkID)` / `GetPartsList(SortBy:ExtendedDisplayedLabel)` (harness variant);
  `Column order and display` for the project variant begins `Name:Class name;Index:0;Hidden:True`.
* Recovered column specs for `Parts list` variants:
  `0 Class name · 1 Reference · 2 Located in · 3 Part number · 4 Manufacturer · 5 Description`
  (one variant) and `0 Class name · 1 Reference · 2 Description · 3 Part number · 4 Manufacturer ·
  5 Located in` (the `PartsList(SortBy:WorkID)` variant), remaining columns `Cross Reference, Image, Tag, Web,
  Quantity, Tag frozen, Function, Voltage, Current range, Fault level, Current, Type, Label, Coil Voltage,
  FLA, Frequency, Power, Speed, Name, …` all `Hidden:True`.
* Column semantics: `Reference` = `DisplayedLabel` / `WorkID`; `Cross Reference` =
  `GetNavigationObjects(Where:GetClass.IsSubClassOf(Class:"Work block");Item:0).SheetPosition`.
* Row pattern: one row per **placed component instance** (not per part) — this is the per-device list.
  *Open point:* the template's first three headers are ordered `Reference, Located in, Class name` while the
  project query's column order starts `Class name, Reference, Located in`; the Location Parts List variant
  matches its template exactly, so the writer mode "visible columns → template columns" holds; the Parts List
  header mismatch is recorded as an unresolved detail (see §6).

### 3.3 Wire List — `Template files\WireList.xlsx`

* Sheet `Feuil1`, `A2:K33`; merged `A5:F5`; title `A5` = `Wire List`.
* Header row **7**: `B7 Number · C7 Net · D7 Length · E7 Wire Size · F7 Source · G7 Destination`
  (column A deliberately empty; freeze `A8`).
* Export anchor **`B7`**; `E2` project, `E3` date.
* Owner query `Wire list` = `GetAllWires` (grid). Recovered visible columns for `Wire list` variants:
  `Number · Net · Length · Gauge/Wire size · Source · Destination` (one variant), and
  `Reference · Net · Cable · Length · Source · Source location · Destination · Destination location ·
  Wire size · Part number · Manufacturer` (richer variant); `Class name`, `Description`, `Cable`,
  `Virtual part`, `Image`, `Displayed in catalogue`, `Tag frozen`, `Source Part No.`,
  `Destination Part No.`, `Number`, `Color`, `Web` hidden.
* Column semantics (query cells): `Number` = `GetAssociatedConnections(Item:0).GetDisplayedNumber`;
  `Net` = `GetAssociatedConnections(Item:0).GetNet.DisplayedLabel`; `Source` = `GetSourceConnectedLabel`;
  `Destination` = `GetDestinationConnectedLabel`; `Source/Destination location` =
  `GetSourceConnectedPort.LocatedIn` / `GetDestinationConnectedPort.LocatedIn`;
  `Wire Size` = Wire `Gauge`/`Wire size` property; `Length` = Wire/Cable `Length` (Measurement).
* Row pattern: one row per wire (pin-to-pin), with net and both endpoints expanded from connectivity
  (`ConnectedTo` relations), not stored columns.

### 3.4 Cable List — `Template files\CableList.xlsx`

* Sheet `Feuil1`, `A2:J33`; merged `A5:F5`; title `A5` = `Cable List`; freeze `A8`.
* Header row **7**: `B7 Description · C7 Part Number · D7 Manufacturer · E7 Length · F7 Conductor`
  (A empty).
* Export anchor `B7`; `E2` project, `E3` date.
* Owner query `Cable list` = `GetAllCables(SortBy:Name)`, grid.
* Recovered visible columns (two variants):
  `Description · Name · Part number · Manufacturer · Length · Conductor size`
  and `Name · Description · Part number · Manufacturer · Length · Conductor size`; hidden:
  `Class name, Image, Virtual part, Tag frozen, Web, Quantity, Wire numbering type, Flip,
  List of values for wire numbering, Displayed in catalogue, Twisted group, Required Gauge, Color,
  Cross Reference`.
* `Cross Reference` expression = `GetNavigationObjects(Item:0).GetSheetPosition` (hidden in this report).
* Row pattern: one row per cable; `Conductor` = conductor size/`Conductor size` property;
  `Length` = measured/entered cable length.

### 3.5 Cable Schedule — `Template files\CableSchedule.xlsx`

* Sheet `Feuil1`, `A2:H47`; **only** `D2 Project:`, `D3 Date:` (title area `A5:D5` merged but **no title text,
  no header row**).
* Export anchor `A7`; `E2` project, `E3` date; **`A5` = Cable Name = `DisplayedLabel`**.
* Because the template has no header row, the export generates the header at `A7` from the query columns.
  This is the strongest single proof that headers are data, not layout.
* Row pattern: a *per-cable* schedule block: one block per selected cable, header row at A7, body rows from the
  `Cable structure`/`Wires` query (`GetWires` — "Displays cable's conductors"); `A5` names the cable.
  Widths: A 17.0, B 28.6, C 17.4, D 45.1, E 24.6, F 18.7, G 22.2, H 17.0.

### 3.6 Connection nets (file name typo: `ConnecttionList.xlsx`) — `Template files\ConnecttionList.xlsx`

* Sheet `Feuil1`, `A2:K33`; merged `A5:F5`; title `A5` = `Connection nets`; freeze `A8`.
* Header row **7**: `B7 Class name · C7 Number · D7 Connected elements` (A empty; E/F widths set but unlabelled).
* Export anchor **`B8`**; `E2` project, `E3` date.
* Owner query `Connection nets` = `GetEquipotentials` (grid, `Sorting column=Number:Ascending`,
  column 0 `Class name` visible). Recovered columns:
  `0 Class name · 1 Number · 2 Connected elements · 3 Required gauge (hidden) · 4 Part number ·
   5 Manufacturer · 6 Description`.
* `Connected elements` is the derived member list (query cell `C…GetConnectedWires(Separator:/)` /
  `GetConnectedWires` family), i.e. the net's participants, not stored.
* Row pattern: one row per equipotential (net), sorted by net number, listing all elements on it.

### 3.7 IO List — `Template files\IO List.xlsx`

* Sheet `Feuil1`, `A2:H713` (formatted to 713 rows, i.e. a big PLC I/O list); merged `A5:D5`;
  title `A5` = `IO List`.
* Header row **7**: `A7 Description · B7 Connected Element · C7 Through Terminal Strip · D7 Owner`.
* Export anchor `A7`; `D2` project, `D3` date.
* Owner query `IOs` = `GetIOs` (grid). Recovered columns:
  `0 Class name (hidden) · 1 Description · 2 Type · 3 Connected element · 4 Connected to (hidden) ·
   5 Requires terminals (hidden) · 6 Through terminal strip · 7 Owner · 8 Terminal Label · 9 Name (hidden) ·
   10 Module block … 18 Address (hidden) …`
  → the template shows the 4 business columns `Description, Connected element, Through terminal strip, Owner`
  (a deliberate subset of the query's visible columns).
* Column semantics (query cells): `Owner` = `Parent.DisplayedLabel(ExtendedLabel:True)`;
  `Connected element` = `GetConnectedElement.DisplayedLabel`;
  `Through terminal strip` = `GetThroughTerminalStrip.DisplayedLabel`;
  `Terminal Label` = `GetGroupedPorts(Separator:/)` (header `Terminal Label(s)`).
* `Card` column in DI/AI/AO queries = `ParentCard.WorkID`; `Address` = I-O `Address` property
  (e.g. `%I000.0`, `%Q000.0` — see the import template).
* Row pattern: one row per PLC I/O point; connected element/terminal strip/owner resolved by relation
  traversal; grouping is implicit in the owning PLC module.

### 3.8 Component IO List — `Template files\Component IO List.xlsx`

* Sheet `Feuil1`, `A2:H435`; merged `A5:D5`; title `A5` = `IO List`.
* Header row **7**: `A7 Description · B7 Connected Element · C7 Through Terminal Strip · D7 Owner`.
* Export anchor `A7`; `D2` project, `D3` date, **`D4` Component Name = `DisplayedLabel`**.
* Two export definitions ship (one per class context) with identical layout — i.e. the same report bound to
  two different selected objects (e.g. project-level vs component-level I/O list).
* Row pattern: identical to IO List but **filtered to the I/O owned by the named component** (`D4` names it).

### 3.9 Location BOM — `Template files\Location BOM.xlsx`

* Sheet `Feuil1`, `A1:J35`; merged `A1:C4` (logo), `E2:F2`, `E3:F3`, `E4:F4`, `A6:F6` (title).
* Title `A6` = `Location Bill of Material`; header row **8**: `A8 Class name · B8 Part number ·
  C8 Manufacturer · D8 Description · E8 Qty · F8 Where used`.
* Export anchor **`A8`**; `E2` project, `E3` date, `E4` location (= `DisplayedLabel(ExtendedLabel:True)`).
* Owner query `Location BOM` (grid). Recovered columns start identically to BOM:
  `0 Class name · 1 Part number · 2 Manufacturer · 3 Description · 4 Qty · 5 …` with hidden extras
  (`Image, Web, Displayed in catalogue, …`).
* Row pattern: BOM restricted to one location; same grouping as §3.1.

### 3.10 Location Parts List — `Template files\Location Parts List.xlsx`

* Sheet `Feuil1`, `A1:J33`; merged `A1:D4` (logo), `A5:F6` (title); title `A5` = `Location Parts List`.
* Header row **7**: `A7 Reference · B7 Located in · C7 Class name · D7 Part Number · E7 Manufacturer ·
  F7 Description`.
* Export anchor `A7`; `F2` project, `F3` date, `F4` location.
* Owner query `Location parts-list` (grid). Recovered columns:
  `0 Reference · 1 Located in · 2 Class name · 3 Tag (hidden) · 4 Part number · 5 Manufacturer ·
   6 Description · 7 Image (hidden) · 8 Web (hidden) · 9 Quantity …`
  → **visible order exactly matches the template's A–F**, which validates the "visible query columns in Index
  order are written from Start Cell" rule and proves `Hidden` columns are skipped.
* Row pattern: one row per component/tag in the location; cross-reference column exists but is hidden here.

### 3.11 I/O import template — `Template files\IO import templates\I-O list template.xlsx`

Not an output report: the **input** contract for importing PLC I/O. Sheet `Sheet 1`, `A1:D6`; no merges.

| Column | Sample values |
|---|---|
| `I/O Type` | `Digital output`, `Digital input`, `Analog output`, `Analog input`, `Relay output` |
| `Description` | `Motor start`, `Motor running`, `Motor speed`, `Temperature`, `Motor fault` |
| `Requires Terminals` | `True`/`False` (blank allowed) |
| `Address` | `%Q000.0`, `%I000.0`, `%QW000`, `%IW000` |

These header labels map 1:1 to `I-O` property definitions (`Type`, `Description`, `Requires terminals`,
`Address`) — the import is a query-cell-driven mapping (kernel types `SkyCadIOImporter`,
`SkyCadStoredIOImporterColumnIndex`, and `SkyCadIOImportThreadingProcess`).

### 3.12 Cross-template observations

1. **No formulas in any template.** Zero `defined_names`, zero `data_validations`, zero formulas in all 11
   workbooks. All computation is in the export engine (cell expressions + column query expressions).
2. Header row position is data (`Start Cell`), not layout: templates that carry a header (BOM/Partslist/
   CableList/WireList/ConnecttionList/IO List/Component IO List/Location BOM/Location Parts List) match their
   export anchor; the one template without a header (CableSchedule) still exports one.
3. Fixed "cover" cells are always three or four: Project, Date, and optionally Location or Component.
4. `Where used` and `Connected elements` are computed aggregates (separator `/`), never stored properties.
5. All reports are *views over the same object graph*: BOM/parts list group by catalogue part; wire list / cable
   list / connection nets group by wire/cable/equipotential; I/O lists group by PLC I/O; location reports add a
   location filter. The report engine therefore needs one query evaluator + one generic grid→worksheet writer.

---

## 4. Superset spec for AryaCADElectrical (Rust)

Design rule: **model the environment exactly as SkyCAD does conceptually (classes are objects, properties are
data, queries are data), but do not copy its implementation, names where avoidable, or binary format.**

### 4.1 Storage split

| Tier | Contents | Our format |
|---|---|---|
| **Library / environment data** (shipped by us, editable by users) | class tree, property definitions, composition rules, query definitions, column specs, numbering formats, text/title-block templates, report definitions, symbol & footprint libraries, catalogue parts | human-readable files: `classes.toml`, `queries.json`, `numbering.toml`, `reports/*.toml`, plus one file per symbol/part (SVG/DXF + `meta.toml`). Optional SQLite index rebuilt from files. |
| **Runtime project state** | object instances, composition edges, registered relations, connectivity, numbering assignments, derived cross-references, sheet layout | one project file (JSON or SQLite) + DXF drawings; XDATA/extension dictionary for round-trip with other CAD. |
| **GUI** | explorer tree/grid rendered from queries, property editor from property defs, ribbon built from composition rules, catalogue browser, symbol editor, panel/layout editor, report designer (map cell → expression) | not persisted; purely derived from library+project. |

### 4.2 Core structs (fields only)

```rust
// ---------- environment / metamodel ----------
struct Environment {
    id: Uuid,                       // SkyCAD uses one singleton root object (GUID f10078ea-…)
    name: String,                   // "Standard Environment"
    classes: Map<ClassId, ClassDef>,
    root_class: ClassId,
    property_defs: Map<PropDefId, PropertyDef>,
    queries: Map<QueryId, QueryDef>,
    reports: Map<ReportId, ReportDef>,
    numbering: Map<NumberingId, NumberingFormat>,
    lists_of_values: Map<LovId, Vec<String>>,
    icons_dir: PathBuf, images_dir: PathBuf,
}

struct ClassDef {
    id: ClassId, name: String, displayed_name: String,       // catalogue folder name
    parent: Option<ClassId>, children: Vec<ClassId>,
    abstract_: bool,
    icon: Option<IconRef>,
    properties: Vec<PropDefId>,         // OWN properties; inherited = walk parent chain
    composition_rules: Vec<CompositionRuleId>,
    default_queries: Vec<QueryId>,
    work_field: bool,                   // SkyCAD: "Work field classes" subtree = things users create
}

enum PropertyType {                     // SkyCAD's 12 shipped types
    Text, Boolean, Integer, Decimal, Measurement { unit: Option<String> },
    Angle, Color, Image, Date, Image2, File, Directory,
    List { values: ListSource },        // ListSource::Static(LovId) | Query(QueryId) | Expression(Expr)
    Link,                               // URL
    Object { root_query: Option<QueryId>, picker_query: Option<QueryId> }, // SkyCadObject
    LibraryObject { root_query: Option<QueryId> },
}

struct PropertyDef {
    id: PropDefId, name: String, description: String,
    ty: PropertyType,
    owner_class: ClassId,               // declaring class (inheritance by class tree)
    visible: bool, read_only: bool,
    priority: PropPriority,             // Context | Catalogue
    default_value: Option<Expr>, initial_value: Option<Expr>,
    force_initial_as_default: bool, configurable: bool,
    list_type: ListType,                // None | Forced
    mapping_operation: MappingOp,
    owner_display_expr: Option<Expr>,   // e.g. "<r:GetParent.GetParent.DisplayedLabel>"
}

struct CompositionRule {
    id: CompositionRuleId, class: ClassId,
    description: Expr,                  // "Creates a new <r:Parent.ClassName>."
    editor_query: Option<QueryId>,      // which query lists candidates
    icon: IconRef, allow_user_creation: bool,
    ribbon_tab: String, ribbon_section: String,
    disallow_as_child: bool, hidden: bool,
}

struct QueryDef {
    id: QueryId, name: String, description: String,
    select: Expr,                       // "GetUsedParts(SortBy:CatalogueID)"
    class_scope: Option<ClassId>,
    grid: bool, expand_tree: bool, show_index: bool,
    use_extended_label: bool, hidden: bool, secondary: Option<QueryId>,
    icon: IconRef, ribbon_tab: String, ribbon_section: String,
    sort: Option<(ColumnName, SortDir)>,
    row_move_fn: Option<Expr>,
    columns: Vec<Column>,               // from Column order and display (+ query cells)
    cells: Vec<QueryCell>,              // computed columns
}

struct Column { index: u32, name: String, hidden: bool, inclusive: bool, bridge: bool }
struct QueryCell {
    name: String, expr: Expr,
    header_override: Option<String>, allow_edit: bool,
}

struct ReportDef {
    id: ReportId, name: String,
    template: PathBuf,                  // our own .xlsx layout
    anchor: CellRef,                    // "A7"
    fixed_cells: Vec<(CellRef, Expr)>,  // E2=Project, E3=Today, E4=Location, D4=Component …
    query: QueryId,                     // row source
    column_policy: ColumnPolicy,        // AllVisible | TemplateHeaders | Explicit
}

struct NumberingFormat {
    id: NumberingId, name: String,
    template: Expr,                     // "<r:GetClass.GetRoot><#>"
    scope: NumberScope,                 // Global | Local
    kind: NumberKind,                   // Numerical | Text
    start: Option<String>, format: Option<String>,
    prefix: Option<String>, increment: Option<u32>,
    empty_first_value: bool, follow_through: bool, number_connection_webs: bool,
    manages: Expr,
}

// ---------- instance graph (mirrors the shipped object graph) ----------
struct Object {
    id: Uuid,
    class: ClassId,
    owner: Option<Uuid>,                // composition parent
    order: u32,                         // dictionary index inside parent
    props: BTreeMap<PropDefId, Value>,
    children: Vec<Uuid>,
}

enum Value { Text(String), Int(i64), Dec(f64), Bool(bool), Color(u32),
             Date(NaiveDate), Ref(Uuid), List(Vec<Value>), Quantity(f64) }

struct Relation { kind: RelationKind, source: Uuid, target: Uuid }
enum RelationKind {                      // the 17 shipped cardinality kinds (renamed where sensible)
    Represents, RenumberWith, GroupedIn, ImplementedBy, LocatedIn, ConnectedTo,
    CorrespondsTo, LaysOn, LayoutInstanceOf, LayoutRepresentationOf, SplitWith,
    WiredWith, RoutedThrough, LugConnection, IncludedIn, ReferenceOf, FromSystem,
}

// ---------- electrical domain (specialised views over Object) ----------
struct Component   { object: Uuid, tag: String, part_number: String, manufacturer: String,
                     description: String, located_in: Option<Uuid>, quantity: u32,
                     ports: Vec<Uuid>, layouts: Vec<Uuid> }
struct ConnectionPort { object: Uuid, name: String, port_type: PortType, label: String,
                        numbering: Option<NumberingId>, flow_through: bool,
                        group: Option<Uuid>, point: Option<Uuid> }
struct Wire        { object: Uuid, number: String, wire_size: String, structure: String,
                     material: String, length: Option<f64>, part_number: String,
                     manufacturer: String, endpoints: (PortEnd, PortEnd) }
struct Net         { object: Uuid, number: String, color: String, required_gauge: String,
                     members: Vec<Uuid> /* computed */ }
struct Equipotential { object: Uuid, net: Option<Uuid>, elements: Vec<Uuid> }
struct Terminal    { object: Uuid, part_number: String, manufacturer: String,
                     junction: Option<Uuid>, strip: Option<Uuid>, side_a: String, side_b: String }
struct TerminalStrip { object: Uuid, name: String, side_a_label: String, side_b_label: String,
                     located_in: Option<Uuid>, levels: u32, invert_sides: bool }
struct Cable       { object: Uuid, name: String, number: String, part_number: String,
                     manufacturer: String, description: String, length: Option<f64>,
                     conductor_size: String, conductors: Vec<Uuid> /* Wire children */ }
struct CrossReference { object: Uuid, kind: XrefKind, target: Uuid, sheet: u32,
                     row: Option<u32>, column: Option<u32> }
struct Sheet       { object: Uuid, number: String, title: String, description: String,
                     width: f64, height: f64, size_class: ClassId, title_block: Option<Uuid> }
struct TitleBlock  { object: Uuid, block: Uuid, format: String, coords: CoordinateGrid }
struct PlcModule   { object: Uuid, tag: String, prefix: String, slot: String,
                     ios: Vec<Uuid>, layout_block: Option<Uuid>, graphical: Option<Uuid> }
struct PlcIo       { object: Uuid, address: String, io_type: String, description: String,
                     requires_terminals: bool, connected: Option<Uuid>,
                     terminal_strip: Option<Uuid>, owner: Option<Uuid> }
struct LayoutBlock { object: Uuid, width: f64, height: f64, insert_x: f64, insert_y: f64,
                     clearances: (f64,f64,f64,f64), representation: Option<Uuid> }
struct PanelLocation { object: Uuid, name: String, part_number: String, manufacturer: String,
                     requires_junctions: bool }
```

### 4.3 Expression engine (one evaluator, three consumers)

Implement one AST/evaluator used by (a) query selectors, (b) property default/initial values,
(c) numbering formats and report cell expressions. Minimum vocabulary:

```
nav:      Parent, GetParent, GetEnvironment, GetProject, GetClass, GetClass.GetRoot,
          GetLinkedObjects(Direction:,Cardinality:), GetWorkObjects(Class:),
          GetProjectElements(Class:), GetChildren, GetNavigationObjects(Where:,Item:)
ports:    GetPorts, GetConnectedPorts(Where:), GetGroupedPorts(Separator:)
wires:    GetWires, GetAllWires, GetConnectedWires(Separator:)
nets:     GetNet, GetEquipotentials
cable:    GetCable, GetAllCables(SortBy:), GetCableStructure, GetExtendedSubCableName
display:  DisplayedLabel(ExtendedLabel:), WorkID, Tag, GetDisplayedNumber, GetSheetPosition
property: <ClassName>.<PropertyName> (case-tolerant), CurrentValue
aggregate: GetQuantity(ReferenceObjectQuery:), WhereUsed(SEPARATOR:), GetUsedParts, PartsList
predicate: IsSubClassOf(Class:), Where:<expr>, parameters Where/Item/SortBy/Separator/Scope/ForSystems
template: <r:...>, <#>, <r:IF(...)>
```

### 4.4 Report data requirements (minimum per report)

| Report | Needs | Derived (computed) |
|---|---|---|
| BOM | catalogue part identity (class, part no., manufacturer, description), usages | Qty (count of usages), Where used (list of usage parents, `/`-joined) |
| Parts List | per-instance reference (tag/WorkID), located-in, class name, part no., manufacturer, description | cross-reference (sheet position) |
| Wire List | wires: number, net, length, size; endpoint ports → parent components/locations | net label, source/destination labels |
| Cable List | cables: description, part no., manufacturer, length, conductor size | cross-reference |
| Cable Schedule | one cable + its core list | header "Cable Name" |
| Connection Nets | equipotentials: class name, number | connected elements list |
| IO List | PLC I/O points: description, address, owner; connected element; through terminal strip | owner label, connected element, terminal labels |
| Component IO List | the above + owning component identity | same |
| Location BOM / Location Parts List | BOM/Parts-list rows restricted to a `Location` | location label; cross-reference hidden |
| I/O import | spreadsheet rows → I-O objects | map columns by header to `I-O` property defs |

### 4.5 What is library data vs runtime vs GUI (explicit)

* **Library data (we ship, users can extend):** class tree, property schema, composition rules, queries,
  column specs, numbering formats, text/title-block templates, report definitions (template + anchor + cells),
  symbol libraries (schematic blocks), panel footprints, catalogue parts (part number/manufacturer/engineering
  properties). This is where SkyCAD's *knowledge* lives and it is the part worth reproducing as data.
* **Runtime state (per project):** object instances and their property values, composition/relations,
  connectivity (ports ↔ wires ↔ equipotentials), numbering results, sheet/title-block instances, xref caches,
  revisions/options, report export runs. Never shipped as library.
* **GUI:** explorer (tree/grid from queries), property panel (from property defs), ribbon (from composition
  rules + query ribbon metadata), catalogue browser, layout/panel editor, cross-reference display,
  report designer (cell → expression mapping). All derived from library + runtime; store only user layout prefs.

### 4.6 Recommended migration path from the current codebase

1. Keep `crates/elec` as the geometry/DXF layer; add a `crates/model` crate with the structs above (no DXF types).
2. Seed `crates/model` data files from the *contracts in §2–§3* (names/types/columns), not from SkyCAD files.
3. Implement the expression evaluator before any report: every report, numbering format and xref needs it.
4. Build reports as "query → grid → xlsx writer" with a `ReportDef { template, anchor, fixed_cells, query }`;
   this reproduces all 10 templates with ~200 lines of engine plus data.
5. Add an **import-only** reader for `.SkyCadFile`/SkyCAD `.xlsx` for interoperability (see §5), never as our
   save format.

---

## 5. Licence boundaries

### 5.1 What we did here (and may defend)

* **Static observation of a legitimately installed, licensed copy**; the app was not run, no code was
  decompiled, no network access, no circumvention of protection.
* Extraction of **facts and interfaces**: class names, property names/types, relationship kinds, report column
  names/order, template cell anchors, numbering-format semantics, UX conventions, on-disk layout.
  Facts and interface names are the classic interoperability argument (cf. EU Software Directive Art. 6,
  US *Sega v. Accolade* / *Google v. Oracle* fair-use reasoning for declaring code/interfaces — still
  jurisdiction-dependent; get counsel).
* Reading shipped **data** to understand the *format* (`.SkyCadFile`, `.xlsx`) so our tool can *import* it.

### 5.2 What we must NOT do

| Must not | Why |
|---|---|
| Copy SkyCAD source, IL, decompiled C#, or `SkyCadKernel.dll`/`SkyCadControls.dll` in any form | its code |
| Re-ship its **assets**: `Icons\`, `Images\`, `Template files\*.xlsx`, symbol libraries (`AUS/IEC/SYMBOL LIBRARY.SKYCADFILE`), title-block blocks, example projects, catalogue `*.SkyCadFile` | its copyrighted/compiled assets; also contain third-party manufacturer data |
| Use `.SkyCadFile` (its BinaryFormatter object serialization) as **our** save/shipping format, or re-implement that serializer | proprietary file format + potential protection; also fragile |
| Copy its catalogue parts as our library (part numbers, descriptions, manufacturer property sets) | third-party data + SkyCAD compilation; use public manufacturer data or user imports |
| Copy the UI verbatim (icon set, ribbon artwork, dialog layouts) | copyrighted expression; conventions are fine, pixels are not |
| Reverse-engineer to clone internals 1:1, or market as a SkyCAD-compatible replacement using its marks | EULA risk; trademark risk |

### 5.3 Interoperability we may build (safe target)

* An **importer** for community/user-owned SkyCAD environments and `.SkyCadFile` objects *at the user's
  request*, translating the object graph (type name, GUID, settings, links) into our model — implemented from
  observed format facts, not from SkyCAD code.
* An **importer** for the `.xlsx` report outputs and the I/O import template (the I/O template is a simple
  4-column contract; headers map to our `I-O` properties).
* An **exporter** to DXF/DWG (via the existing `acadrust` path) and to plain xlsx/csv with our own templates.
* Naming: use functional names (Component, Terminal, Wire, Net, Cable, CrossReference, Sheet) — these are
  generic engineering terms; avoid SkyCAD-specific coined names (`SkyCadCompositionLink`,
  `Control - line numbering`, `Try merge on System use`, `Proxy. switch`, `ConnecttionList` typo) in our public
  API where a generic equivalent exists. If a name is needed for import mapping, keep it in a private
  compatibility table, not in the user-facing model.

### 5.4 EULA status

The SkyCAD EULA was accepted on this machine to complete installation (`STATUS.md` §5) and was not
re-read here. It very likely prohibits reverse engineering for a competing product. This document is a
**functional/interface specification derived from observable behaviour and shipped data**, with **no code or
asset copying**; whether that is permissible is a legal question for the project owner's counsel. Practical
risk controls: keep this analysis in `reports/` (documentation), never vendor SkyCAD binaries/assets into the
repository, and keep any interoperability code cleanly separated and reverse-engineered-from-format-only.

---

## 6. Residual unknowns / limitations (honest gaps)

1. **Static only.** SkyCAD never executed; no exported report was generated, so end-to-end writing behaviour is
   inferred from data + template geometry. Rule "visible query columns in `Index` order are written from
   `Start Cell`" is proved for `Location Parts List` (exact A–F match) and consistent with BOM, but
   `ConnecttionList` (3 template headers vs 6 visible columns) and the `Partslist` header order mismatch
   (Report §3.2/§3.6) remain unresolved. Next probe: run one export in SkyCAD on an example project and read
   back the produced `.xlsx` (requires a licensed GUI session — deliberately not done here).
2. **Member decoding was partial** for `SkyCadKernel.dll` (64 MiB representation budget,
   `ev_031cdfbd…`): field/method coverage is complete only for the selected six types; other classes' fields
   were recovered from the *data* (property definitions) rather than from metadata.
3. **Binary framing partly mapped**: object headers, `%<guid>/` references, and length-prefixed strings are
   evidenced; primitive record bytes between strings are not fully decoded (no need for our purposes).
4. **Property-definition count 644 vs 645 lines** in my parse (one record split differently) — safe to state
   "≈644 shipped property definitions".
5. **Instance counts per project** vary with duplicate-record folding (see §2.9 caveat).
6. Some recovered strings carry a leading character that is the BinaryFormatter string-length prefix; quoted
   values in this report were normalised by stripping it. Original bytes remain in the artifacts.
7. **Class-name vs folder-name drift**: e.g. folder `Graphical PLC Module` ↔ type `SkyCadGraphicalPLCModule`;
   folder `Terminal Strip Element` ↔ class `Terminal Strip Element`; a few class names contain typos/oddities
   (`Ultra-sonic proxy.`, `Control - line numbering`, filename `ConnecttionList.xlsx`). Recorded verbatim as evidence,
   not endorsed.
8. **EULA text not reviewed** in this session (see §5.4).

---

## Appendix A — the 18 shipped `SkyCadExcelExport` definitions

| # | Template | Start | Notes |
|---|---|---|---|
| 1 | — (query export, inline `Overloadable settings` query) | — | not a report |
| 2 | `CableSchedule.xlsx` | A7 | + A5 Cable Name |
| 3 | `Partslist.xlsx` | A7 | Project/Date @F2/F3 |
| 4 | `BOM.xlsx` | A7 | Project/Date @E2/E3 |
| 5 | `ConnecttionList.xlsx` | B8 | Project/Date @E2/E3 |
| 6 | `IO List.xlsx` | A7 | Project/Date @D2/D3 |
| 7 | `WireList.xlsx` | B7 | Project/Date @E2/E3 |
| 8 | `CableList.xlsx` | B7 | Project/Date @E2/E3 |
| 9 | — | — | not a report |
| 10 | `Location Parts List.xlsx` | A7 | Location @F4 |
| 11 | `WireList.xlsx` | B7 | second class context |
| 12 | `Location BOM.xlsx` | A8 | Location @E4 |
| 13 | — | — | not a report |
| 14 | `Component IO List.xlsx` | A7 | Component @D4 |
| 15 | `Component IO List.xlsx` | A7 | second class context |
| 16–18 | — | — | not report definitions (UI/export helper contexts) |

## Appendix B — the 26 numbering formats (names + templates)

`Standard basic numbering format` = `<r:GetClass.GetRoot><#>` · `Sheet numbering format` = `<#>`;
`Empty numbering format`; `Wire by rung numbering format` = `<r:GetFirstConnection.Row><#>`;
`Wire numbering by connected component` = `<r:GetParent.DisplayedLabel>-<r:DisplayedLabel>`;
`Revision numbering format`; `Systems numbering format` = `<r:Name><#>`;
`Connection port numbering format` = `<#>`;
`Text numbering format`; `Terminal strip numbering format`; `Component by rung numbering format` =
`k<r:GetClass.GetRoot><r:GetNavigationObjects(Where:GetClass.IsSubClassOf(Class:"Work block");Item:0).Row><#>`;
`PLC Module numbering format`; `Cable wire numbering format` = `<#>`; `Cable shield numbering format`;
`Connector pin numbering format` = `<#>`; `Connector numbering format`; `Location numbering format`;
`Numbering format for IO Connection w/out slot` =
`w<r:GetConnectedPorts(Where:GroupedIn.Class=I-O).Parent.WorkID>-<r:GetConnectedPorts(Where:GroupedIn.Class=I-O).Name><#>`;
`Numbering format for IO Connection with slot` = same + `.Parent.Slot`; `Option numbering format`;
`Harness Branch numbering format`; `Not connected element numbering format` = `<r:GetClass.GetRoot>`;
`Unterminated wire numbering format`; `Function basic numbering format`;
`Internal revision numbering format`; `Network Interface Setting numbering format` = `192.000.000.<#>`
(order format `000`).
Format options: `Uniqueness scope` (Global/Local), `Type` (Numerical/…), `Start value`, `Order number format`,
`Order number prefix`, `Order number increment`, `Empty first value`, `Follow through (numbering by symbol)`,
`Number connection webs (numbering by symbol)`, `Manages`.

## Appendix C — where the evidence lives

```
C:\Program Files\SkyCAD Electrical\SkyCadKernel.dll                       (managed metadata; ev_…)
C:\SkyCAD Environments\Standard Environment\EnvironmentStructure.SkyCadFile
    classes @~294,132 / ~320,324 / ~322,712;  property defs @~299,356 / ~306,345 / ~313,340 / ~326,765;
    queries @~4,100 / ~11,759 / ~18,623 / ~25,497;  ExcelExport @~68,870 / ~1,067,243;
    ExcelCellValue @~1,068,487 / ~1,070,085;  column spec strings @~1,308,741 / ~4,857,827;
    query strings @~1,468,201 / ~1,470,232
C:\SkyCAD Environments\Standard Environment\Catalogue\…\*.SkyCadFile      (instances, composition histograms)
C:\SkyCAD Environments\Standard Environment\Template files\*.xlsx          (10 reports + 1 import layout)
Working extracts kept outside the repo (scratch): %TEMP%\skycad_env_parse2.txt,
    skycad_env_parse3.txt, skycad_env_parse4.txt, skycad_proj_parse.txt, skycad_columns.txt,
    skycad_blocks.txt  (regenerable; not committed)
```

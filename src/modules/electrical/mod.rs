//! Electrical (ECAD) ribbon tab.
//!
//! The tab is the entry point for the electrical layer: symbols, conductors, the devices
//! on them, and the numbering, cross-references and reports that make an electrical
//! drawing worth drawing. Every button dispatches the same command the command line uses,
//! so each feature stays scriptable and a tool's id is its command name.
//!
//! The layout follows the module idiom the other tabs use — an icon per command, a
//! [`ToolDef`] that carries it, and ribbon groups shaped by the drafting workflow rather
//! than by the order the commands happened to be written: place symbols, wire them, number
//! the devices, then describe them with terminals, cross-references and reports.

pub mod place;
pub mod wire;

use crate::modules::{CadModule, IconKind, ModuleEvent, RibbonGroup, RibbonItem, ToolDef};
use ocs_plugin_api::ribbon::FOLLOW_CHOSEN;

/// An electrical tool button.
///
/// The id *is* the command name: the ribbon sends [`ModuleEvent::Command`] back with it,
/// so a button runs exactly the verb typing it would run and no mapping table can drift
/// from the dispatcher. A command that needs arguments starts and prompts for them, the
/// way every other CAD command does.
fn elec_tool(id: &'static str, label: &'static str, icon: IconKind) -> ToolDef {
    ToolDef {
        id,
        label,
        icon,
        event: ModuleEvent::Command(id.to_string()),
    }
}

/// One entry of a dropdown: (command, menu label, icon).
fn menu_item(
    id: &'static str,
    label: &'static str,
    icon: IconKind,
) -> (&'static str, &'static str, IconKind) {
    (id, label, icon)
}

/// The tab's icons, one per tool, embedded the way every other module embeds its own.
///
/// They are drawn on the same 24x24 grid with the application's stroke weight and its
/// grey body plus blue accent, so the tab reads as Draw and Annotate do rather than as a
/// row of glyphs.
mod icons {
    use crate::modules::IconKind;

    pub const ATTEDIT: IconKind =
        IconKind::Svg(include_bytes!("../../../assets/icons/attedit.svg"));
    pub const ATTRIBUTES: IconKind =
        IconKind::Svg(include_bytes!("../../../assets/icons/elec_attributes.svg"));
    pub const BOM: IconKind = IconKind::Svg(include_bytes!("../../../assets/icons/elec_bom.svg"));
    pub const DEVICE_LIST: IconKind =
        IconKind::Svg(include_bytes!("../../../assets/icons/elec_device_list.svg"));
    pub const INSERT: IconKind =
        IconKind::Svg(include_bytes!("../../../assets/icons/elec_insert.svg"));
    pub const IO: IconKind = IconKind::Svg(include_bytes!("../../../assets/icons/elec_io.svg"));
    pub const JUNCTION: IconKind =
        IconKind::Svg(include_bytes!("../../../assets/icons/elec_junction.svg"));
    pub const LIBRARY: IconKind =
        IconKind::Svg(include_bytes!("../../../assets/icons/elec_library.svg"));
    pub const LIBRARY_FOLDER: IconKind =
        IconKind::Svg(include_bytes!("../../../assets/icons/elec_library_folder.svg"));
    pub const MAKE_SYMBOL: IconKind =
        IconKind::Svg(include_bytes!("../../../assets/icons/elec_make_symbol.svg"));
    pub const NEW_DRAWING: IconKind =
        IconKind::Svg(include_bytes!("../../../assets/icons/elec_new_drawing.svg"));
    pub const NUMBER_DEVICE: IconKind =
        IconKind::Svg(include_bytes!("../../../assets/icons/elec_number_device.svg"));
    pub const NUMBER_PREVIEW: IconKind =
        IconKind::Svg(include_bytes!("../../../assets/icons/elec_number_preview.svg"));
    pub const PANEL: IconKind =
        IconKind::Svg(include_bytes!("../../../assets/icons/elec_panel.svg"));
    pub const PART: IconKind = IconKind::Svg(include_bytes!("../../../assets/icons/elec_part.svg"));
    pub const PLC: IconKind = IconKind::Svg(include_bytes!("../../../assets/icons/elec_plc.svg"));
    pub const PLC_NEXT: IconKind =
        IconKind::Svg(include_bytes!("../../../assets/icons/elec_plc_next.svg"));
    pub const PROJECT: IconKind =
        IconKind::Svg(include_bytes!("../../../assets/icons/elec_project.svg"));
    pub const PROJECT_NEW: IconKind =
        IconKind::Svg(include_bytes!("../../../assets/icons/elec_project_new.svg"));
    pub const PROJECT_OPEN: IconKind =
        IconKind::Svg(include_bytes!("../../../assets/icons/elec_project_open.svg"));
    pub const PROJECT_RENUMBER: IconKind =
        IconKind::Svg(include_bytes!("../../../assets/icons/elec_project_renumber.svg"));
    pub const SETUP: IconKind =
        IconKind::Svg(include_bytes!("../../../assets/icons/elec_setup.svg"));
    pub const SHEET: IconKind =
        IconKind::Svg(include_bytes!("../../../assets/icons/elec_sheet.svg"));
    pub const TERMINAL_SIDES: IconKind =
        IconKind::Svg(include_bytes!("../../../assets/icons/elec_terminal_sides.svg"));
    pub const TERMINAL_STRIP: IconKind =
        IconKind::Svg(include_bytes!("../../../assets/icons/elec_terminal_strip.svg"));
    pub const WIRE: IconKind = IconKind::Svg(include_bytes!("../../../assets/icons/elec_wire.svg"));
    pub const WIRE_LIST: IconKind =
        IconKind::Svg(include_bytes!("../../../assets/icons/elec_wire_list.svg"));
    pub const WIRE_NUMBER: IconKind =
        IconKind::Svg(include_bytes!("../../../assets/icons/elec_wire_number.svg"));
    pub const XREF: IconKind = IconKind::Svg(include_bytes!("../../../assets/icons/elec_xref.svg"));
    pub const XREF_WRITE: IconKind =
        IconKind::Svg(include_bytes!("../../../assets/icons/elec_xref_write.svg"));
    pub const PARTS: IconKind = IconKind::Svg(include_bytes!("../../../assets/icons/elec_parts.svg"));
    pub const PART_ASSIGN: IconKind =
        IconKind::Svg(include_bytes!("../../../assets/icons/elec_part_assign.svg"));
    pub const REVISION: IconKind =
        IconKind::Svg(include_bytes!("../../../assets/icons/elec_revision.svg"));
    pub const BUILT: IconKind = IconKind::Svg(include_bytes!("../../../assets/icons/elec_built.svg"));
    pub const IO_IMPORT: IconKind =
        IconKind::Svg(include_bytes!("../../../assets/icons/elec_io_import.svg"));
    pub const CABLE: IconKind = IconKind::Svg(include_bytes!("../../../assets/icons/elec_cable.svg"));
}

pub struct ElectricalModule;

impl CadModule for ElectricalModule {
    fn id(&self) -> &'static str {
        "electrical"
    }

    fn title(&self) -> &'static str {
        "Electrical"
    }

    fn ribbon_groups(&self) -> &[RibbonGroup] {
        static GROUPS: std::sync::OnceLock<Vec<RibbonGroup>> = std::sync::OnceLock::new();
        GROUPS.get_or_init(|| {
            use icons::*;
            vec![
                // ── Symbols ───────────────────────────────────────────────────
                // Insert is the action a drawing starts with; the library face
                // runs the palette and its menu also reaches loading a folder.
                RibbonGroup {
                    title: "Symbols",
                    tools: vec![
                        RibbonItem::LargeTool(elec_tool("ELECINSERT", "Insert\nSymbol", INSERT)),
                        RibbonItem::LargeDropdown {
                            id: "ELEC_SYMBOL_LIBRARY",
                            label: "Symbol\nLibrary",
                            icon: LIBRARY,
                            items: vec![
                                menu_item("ELECPALETTE", "Browse Symbols", LIBRARY),
                                menu_item("ELECLIB", "Load Library Folder", LIBRARY_FOLDER),
                            ],
                            default: "ELECPALETTE",
                        },
                        RibbonItem::LabeledTool(elec_tool(
                            "ELECMAKESYMBOL",
                            "Make Symbol",
                            MAKE_SYMBOL,
                        )),
                        RibbonItem::LabeledDropdown {
                            id: "ELEC_ATTRIBUTES",
                            label: "Edit Attributes",
                            icon: ATTRIBUTES,
                            items: vec![
                                menu_item("ATTEDIT", "Attribute Editor", ATTEDIT),
                                menu_item("ELECSETATTR", "Set Electrical Attributes", ATTRIBUTES),
                            ],
                            default: "ATTEDIT",
                        },
                    ],
                },
                // ── Wiring ───────────────────────────────────────────────────
                // Conductors first: a wire is drawn differently from a line (it
                // chains from the last point and lands on the wire layer), and
                // junctions and wire numbers read the geometry this draws.
                RibbonGroup {
                    title: "Wiring",
                    tools: vec![
                        RibbonItem::LargeTool(elec_tool("ELECWIRE", "Wire", WIRE)),
                        RibbonItem::LabeledTool(elec_tool(
                            "ELECJUNCTIONS",
                            "Junctions",
                            JUNCTION,
                        )),
                        RibbonItem::LabeledTool(elec_tool(
                            "ELECWIRENUMBER",
                            "Number Wires",
                            WIRE_NUMBER,
                        )),
                    ],
                },
                // ── Devices ──────────────────────────────────────────────────
                // The numbering face shows and runs the numbering last chosen, the
                // Annotate dropdown idiom; I/O and the PLC placement share a menu
                // because they are one workflow — place the module, list its points.
                RibbonGroup {
                    title: "Devices",
                    tools: vec![
                        RibbonItem::LargeTool(elec_tool("ELECLIST", "Component\nList", DEVICE_LIST)),
                        RibbonItem::LargeDropdown {
                            id: "ELEC_NUMBER_DEVICES",
                            label: FOLLOW_CHOSEN,
                            icon: NUMBER_DEVICE,
                            items: vec![
                                menu_item("ELECNUMBER", "Number Devices", NUMBER_DEVICE),
                                menu_item("ELECNUMBERPREVIEW", "Preview Numbering", NUMBER_PREVIEW),
                            ],
                            default: "ELECNUMBER",
                        },
                        RibbonItem::LabeledDropdown {
                            id: "ELEC_PLC_IO",
                            label: "I/O & PLC",
                            icon: IO,
                            items: vec![
                                menu_item("ELECIO", "I/O Cross-Refs", IO),
                                menu_item("ELECPLC", "PLC Module", PLC),
                                menu_item("ELECPLCNEXT", "PLC Next Module", PLC_NEXT),
                                menu_item("ELECIOIMPORT", "Import I/O List", IO_IMPORT),
                            ],
                            default: "ELECIO",
                        },
                    ],
                },
                // ── Parts (local library) ────────────────────────────────────
                // The catalogue is local by design: a folder next to the project, no
                // account and no network, so a project travels with what it needs.
                RibbonGroup {
                    title: "Parts",
                    tools: vec![
                        RibbonItem::LargeTool(elec_tool("ELECPARTS", "Parts\nLibrary", PARTS)),
                        RibbonItem::LabeledTool(elec_tool("ELECPARTLIST", "Browse Parts", PART)),
                        RibbonItem::LabeledTool(elec_tool(
                            "ELECASSIGN",
                            "Assign Part",
                            PART_ASSIGN,
                        )),
                    ],
                },
                // ── Terminals ────────────────────────────────────────────────
                // A terminal strip is a component; its two sides are what the
                // netlist cannot see, so they get their own report.
                RibbonGroup {
                    title: "Terminals",
                    tools: vec![
                        RibbonItem::LargeTool(elec_tool(
                            "ELECTERMINALS",
                            "Terminal\nStrips",
                            TERMINAL_STRIP,
                        )),
                        RibbonItem::LabeledTool(elec_tool(
                            "ELECTSIDES",
                            "Terminal Sides",
                            TERMINAL_SIDES,
                        )),
                    ],
                },
                // ── Cross-Reference ──────────────────────────────────────────
                // Report where a device's parts are, then write those
                // references onto the drawing beside each part.
                RibbonGroup {
                    title: "Cross-Reference",
                    tools: vec![
                        RibbonItem::LargeTool(elec_tool("ELECXREF", "Cross\nReference", XREF)),
                        RibbonItem::LabeledTool(elec_tool(
                            "ELECXREFWRITE",
                            "Write Xrefs",
                            XREF_WRITE,
                        )),
                    ],
                },
                // ── Reports ──────────────────────────────────────────────────
                // What leaves the drawing: the bill of material, the wire list,
                // the panel arrangement, and the parts catalogue that fills the
                // manufacturer and description columns of the first.
                RibbonGroup {
                    title: "Reports",
                    tools: vec![
                        RibbonItem::LargeTool(elec_tool("ELECBOM", "Bill of\nMaterial", BOM)),
                        RibbonItem::LargeTool(elec_tool("ELECWIRELIST", "Wire\nList", WIRE_LIST)),
                        RibbonItem::LabeledTool(elec_tool("ELECPANEL", "Panel Layout", PANEL)),
                        RibbonItem::LabeledTool(elec_tool(
                            "ELECWIRELABELS",
                            "Wire Labels",
                            WIRE_NUMBER,
                        )),
                        RibbonItem::LabeledTool(elec_tool("ELECPART", "Parts Catalogue", PART)),
                        RibbonItem::LabeledDropdown {
                            id: "ELEC_CABLES",
                            label: "Cables",
                            icon: CABLE,
                            items: vec![
                                menu_item("ELECCABLELIST", "Cable Schedule", CABLE),
                                menu_item("ELECCABLE", "Add Cable", CABLE),
                            ],
                            default: "ELECCABLELIST",
                        },
                    ],
                },
                // ── Project & Sheet ──────────────────────────────────────────
                // The sheet frame and the project that indexes the sheets, then
                // the project-file menu and the drawing setup behind it.
                RibbonGroup {
                    title: "Project & Sheet",
                    tools: vec![
                        RibbonItem::LargeTool(elec_tool("ELECSHEET", "Sheet\nFrame", SHEET)),
                        RibbonItem::LargeTool(elec_tool(
                            "ELECPROJECT",
                            "Project\nReport",
                            PROJECT,
                        )),
                        RibbonItem::LabeledDropdown {
                            id: "ELEC_PROJECT_FILES",
                            label: "Project Files",
                            icon: PROJECT,
                            items: vec![
                                menu_item("ELECPROJLIST", "List Sheets", PROJECT),
                                menu_item("ELECPROJNEW", "New Project", PROJECT_NEW),
                                menu_item("ELECPROJOPEN", "Open Project", PROJECT_OPEN),
                                menu_item("ELECPROJNUM", "Renumber Project", PROJECT_RENUMBER),
                            ],
                            default: "ELECPROJLIST",
                        },
                        RibbonItem::LabeledDropdown {
                            id: "ELEC_SHEET_DRAWING",
                            label: "Drawing",
                            icon: NEW_DRAWING,
                            items: vec![
                                menu_item("ELECNEW", "Prepare Drawing", NEW_DRAWING),
                                menu_item("ELECSETUP", "Electrical Setup", SETUP),
                                menu_item("ELECHELP", "Command Reference", SETUP),
                            ],
                            default: "ELECNEW",
                        },
                        RibbonItem::LabeledDropdown {
                            id: "ELEC_REVISION_CONTROL",
                            label: "Revisions",
                            icon: REVISION,
                            items: vec![
                                menu_item("ELECREV", "Revision Control", REVISION),
                                menu_item("ELECBUILT", "Built Project", BUILT),
                            ],
                            default: "ELECREV",
                        },
                    ],
                },
            ]
        })
    }
}

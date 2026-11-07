//! Electrical (ECAD) commands.
//!
//! These are the host-side features of the electrical layer. They read placed symbols
//! from the drawing, turn each block insert and its attributes into an `elec::Component`,
//! and report from that model. Everything heavier (numbering, terminal strips, cross
//! references) builds on the same extraction.
//!
//! Commands are dispatched by name like every other CAD command, so they are reachable
//! from the command line and from a ribbon tool.

use super::{Message, OpenCADStudio};
use ::elec::{bill_of_materials, Component};
use iced::Task;
use std::collections::{BTreeMap, BTreeSet};

/// The parts catalogue loaded for this session, if any.
///
/// A catalogue belongs to the user's machine rather than to one drawing, so it lives beside
/// the app instead of inside a document. It is loaded explicitly with `ELECPART` and stays
/// until replaced; `ELECBOM` uses it to fill manufacturer and description for parts the
/// drawing names but does not describe.
static CATALOGUE: std::sync::OnceLock<std::sync::RwLock<Option<::elec::Catalogue>>> =
    std::sync::OnceLock::new();

fn catalogue() -> &'static std::sync::RwLock<Option<::elec::Catalogue>> {
    CATALOGUE.get_or_init(|| std::sync::RwLock::new(None))
}

/// Folder the last `ELECLIB` loaded symbols from.
static LIBRARY: std::sync::OnceLock<std::sync::RwLock<Option<std::path::PathBuf>>> =
    std::sync::OnceLock::new();

fn library_dir() -> &'static std::sync::RwLock<Option<std::path::PathBuf>> {
    LIBRARY.get_or_init(|| std::sync::RwLock::new(None))
}

/// Where the last library folder is remembered between runs.
///
/// A library belongs to the machine, not the drawing, so it should not have to be retyped
/// every session - and a path is exactly the thing that is painful to type in the GUI.
fn library_config_path() -> Option<std::path::PathBuf> {
    let base = std::env::var_os("APPDATA").map(std::path::PathBuf::from)?;
    Some(base.join("AryaCAD").join("elec_library.txt"))
}

fn remember_library(directory: &std::path::Path) {
    let Some(path) = library_config_path() else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(path, directory.display().to_string());
}

fn remembered_library() -> Option<std::path::PathBuf> {
    let path = library_config_path()?;
    let text = std::fs::read_to_string(path).ok()?;
    let directory = std::path::PathBuf::from(text.trim());
    if directory.is_dir() {
        Some(directory)
    } else {
        None
    }
}

/// One symbol's catalogue entry: which class it belongs to and which attributes an
/// instance of it carries.
#[derive(Debug, Clone, Default)]
struct LibraryEntry {
    /// The symbol's name as the library spells it, for the palette.
    name: String,
    class: String,
    attributes: Vec<String>,
    /// Attribute text height that suits this symbol's own size, measured by `ELECLIB`
    /// from the loaded geometry. Zero means "not measured".
    label_height: f64,
    /// Scale the source drawings insert this block at; 1.0 when they use none.
    scale: f64,
    /// Attribute definitions recorded in the block, as (tag, x, y) in symbol units.
    /// `ELECPLACE` uses these so pins and labels land where the symbol author drew them
    /// instead of stacking on the insertion point.
    attribute_offsets: Vec<(String, f64, f64)>,
}

/// Folder the last `ELECPROJECT` scanned, so module addressing can continue the
/// project's I/O sequence instead of restarting it on every sheet.
static PROJECT_DIR: std::sync::OnceLock<std::sync::RwLock<Option<std::path::PathBuf>>> =
    std::sync::OnceLock::new();

/// One drawing in a project: where it is, and the metadata the project file gives it.
#[derive(Debug, Clone)]
struct ProjectSheet {
    path: std::path::PathBuf,
    number: String,
    title: String,
}

/// The project file currently open, if any.
static PROJECT_FILE: std::sync::OnceLock<std::sync::RwLock<Option<std::path::PathBuf>>> =
    std::sync::OnceLock::new();

pub(crate) fn project_file() -> &'static std::sync::RwLock<Option<std::path::PathBuf>> {
    PROJECT_FILE.get_or_init(|| std::sync::RwLock::new(None))
}

pub(crate) fn project_dir() -> &'static std::sync::RwLock<Option<std::path::PathBuf>> {
    PROJECT_DIR.get_or_init(|| std::sync::RwLock::new(None))
}

static LIBRARY_INDEX: std::sync::OnceLock<
    std::sync::RwLock<std::collections::BTreeMap<String, LibraryEntry>>,
> = std::sync::OnceLock::new();

fn library_index() -> &'static std::sync::RwLock<std::collections::BTreeMap<String, LibraryEntry>> {
    LIBRARY_INDEX.get_or_init(|| std::sync::RwLock::new(std::collections::BTreeMap::new()))
}

/// Load the library manifest lazily when a drawing is read on its own.
///
/// `ELECLIB` fills this index when a session loads symbols, but a project opened
/// the next day (or from a recent file) starts with an empty index — and then
/// every report called the symbols `CONTACT_NO` instead of `Contactor`. The
/// remembered library folder has the class names the drawing was built with, so
/// read its manifest without inserting any blocks: this is a lookup table for
/// reports, not a library load.
fn ensure_library_index() {
    let already_loaded = library_index()
        .read()
        .map(|index| !index.is_empty())
        .unwrap_or(false);
    if already_loaded {
        return;
    }
    let Some(directory) = remembered_library().or_else(library_directory) else {
        return;
    };
    let manifest = read_library_manifest(&directory);
    if manifest.is_empty() {
        return;
    }
    if let Ok(mut index) = library_index().write() {
        for (symbol, entry) in manifest {
            index.entry(symbol).or_insert(entry);
        }
    }
}

/// Read `library.tsv`: symbol, class, `|`-separated attribute tags.
///
/// The tags matter because these symbols keep their attributes on the *insert*, not as
/// attribute definitions inside the block, so the set has to be recorded next to the
/// geometry or an inserted symbol comes in mute.
fn read_library_manifest(
    directory: &std::path::Path,
) -> std::collections::BTreeMap<String, LibraryEntry> {
    let mut index = std::collections::BTreeMap::new();
    let Ok(text) = std::fs::read_to_string(directory.join("library.tsv")) else {
        return index;
    };
    for (number, line) in text.lines().enumerate() {
        if number == 0 || line.trim().is_empty() {
            continue;
        }
        let mut parts = line.split('\t');
        let symbol = parts.next().unwrap_or("").trim();
        let class = parts.next().unwrap_or("").trim();
        let attributes = parts.next().unwrap_or("").trim();
        // Older manifests have no scale column; one means "as drawn".
        let scale = parts
            .next()
            .and_then(|value| value.trim().parse::<f64>().ok())
            .filter(|value| *value > 0.0)
            .unwrap_or(1.0);
        if symbol.is_empty() {
            continue;
        }
        index.insert(
            symbol.to_ascii_uppercase(),
            LibraryEntry {
                name: symbol.to_string(),
                class: class.to_string(),
                attributes: if attributes.is_empty() {
                    Vec::new()
                } else {
                    attributes.split('|').map(|tag| tag.to_string()).collect()
                },
                // Measured once the geometry is loaded, not by the manifest reader.
                label_height: 0.0,
                scale,
                attribute_offsets: Vec::new(),
            },
        );
    }
    index
}

/// Attribute names that carry the well-known component fields.
///
/// The set covers the conventions this project has to read: AutoCAD Electrical's
/// `TAG1`/`CAT`/`MFG`/`DESC`/`LOC`, plain `TAG`/`PART`, and SkyCAD-style long names.
fn field_for_attribute(tag: &str) -> Option<&'static str> {
    match tag.trim().to_ascii_uppercase().as_str() {
        "TAG" | "TAG1" | "TAG2" | "TAGSTR" | "WORKID" => Some("tag"),
        "CAT" | "PART" | "PARTNUMBER" | "CATALOGUEID" | "CATALOG" => Some("part"),
        "MFG" | "MANUFACTURER" | "MFR" => Some("manufacturer"),
        "DESC" | "DESC1" | "DESC2" | "DESCRIPTION" => Some("description"),
        "LOC" | "LOCATION" | "INST" | "INSTALLATION" => Some("location"),
        _ => None,
    }
}

/// The per-drawing electrical setup, as AutoCAD Electrical stores it on `WD_M`.
///
/// These are the project's own rules, not ours: `TAGFMT`/`WIREFMT` are format templates
/// (`%F` family, `%N` number, `%S` sheet), and the `*-START`/`*-RSUF` attributes seed the
/// counters. Reading them means a drawing produced by ACADE renumbers the way its template
/// says, and — because `TAGMODE`/`WIREMODE` `R` means "existing references are fixed" —
/// that its real device tags are left alone.
#[derive(Debug, Clone)]
struct ElecSetup {
    /// True when the drawing actually carries a `WD_M` block we could read.
    present: bool,
    tag_format: String,
    tag_mode: String,
    tag_start: u32,
    tag_suffixes: Vec<String>,
    wire_format: String,
    wire_mode: String,
    wire_start: u32,
    xref_format: String,
    sheet: String,
    /// Step between consecutive device tags (SkyCAD's "Order number increment").
    tag_increment: u32,
    /// Number contacts with their coil instead of one number per symbol
    /// (SkyCAD's "Follow through").
    tag_fallthrough: bool,
    /// Step between consecutive wire numbers.
    wire_increment: u32,
    /// Alphabetical wire numbers (`A`, `B`, ...) instead of numerical.
    wire_alphabetical: bool,
    /// Leave the first wire number blank (SkyCAD's "Empty first value").
    wire_empty_first: bool,
    /// SkyCAD's "Uniqueness scope": `Global` keeps a number from being reused on
    /// another sheet of the project, `Local` only inside this drawing, and `None`
    /// does not consult other tags at all.
    tag_unique: String,
}

impl Default for ElecSetup {
    fn default() -> Self {
        Self {
            present: false,
            tag_format: "%F%N".to_string(),
            tag_mode: "S".to_string(),
            tag_start: 1,
            tag_suffixes: Vec::new(),
            wire_format: "%N".to_string(),
            wire_mode: "S".to_string(),
            wire_start: 1,
            xref_format: "%N".to_string(),
            sheet: String::new(),
            tag_increment: 1,
            tag_fallthrough: true,
            wire_increment: 1,
            wire_alphabetical: false,
            wire_empty_first: false,
            tag_unique: "Global".to_string(),
        }
    }
}

impl ElecSetup {
    /// True when the drawing says its references are fixed (ACADE `TAGMODE`/`WIREMODE` `R`).
    fn references_are_fixed(&self) -> bool {
        self.tag_mode.eq_ignore_ascii_case("R") || self.wire_mode.eq_ignore_ascii_case("R")
    }

    /// True when a number may not be reused on another sheet of the project.
    fn uniqueness_is_global(&self) -> bool {
        self.tag_unique.eq_ignore_ascii_case("Global")
    }
}

/// One of SkyCAD's three uniqueness scopes, whatever spelling came in.
fn canonical_uniqueness(value: &str) -> String {
    match value.trim().to_ascii_lowercase().as_str() {
        "local" => "Local",
        "none" => "None",
        _ => "Global",
    }
    .to_string()
}

/// Read the electrical setup off the first `WD_M` block in the document.
///
/// A drawing without one gets the defaults, so every command still works on a drawing
/// that was never touched by AutoCAD Electrical.
fn read_setup(document: &codec::Document) -> ElecSetup {
    let mut setup = ElecSetup::default();
    for entity in document.entities() {
        let codec::EntityType::Insert(insert) = entity else {
            continue;
        };
        if !insert.block_name.trim().eq_ignore_ascii_case("WD_M") {
            continue;
        }
        setup.present = true;
        for attribute in &insert.attributes {
            let value = attribute.value.trim();
            if value.is_empty() {
                continue;
            }
            match attribute.tag.trim().to_ascii_uppercase().as_str() {
                "TAGFMT" => setup.tag_format = value.to_string(),
                "TAGMODE" => setup.tag_mode = value.to_string(),
                "TAG-START" => setup.tag_start = value.parse().unwrap_or(1),
                "TAG-RSUF" => {
                    setup.tag_suffixes = value
                        .split(',')
                        .map(|part| part.trim().to_string())
                        .filter(|part| !part.is_empty())
                        .collect()
                }
                "WIREFMT" => setup.wire_format = value.to_string(),
                "WIREMODE" => setup.wire_mode = value.to_string(),
                "WIRE-START" => setup.wire_start = value.parse().unwrap_or(1),
                "XREFFMT" => setup.xref_format = value.to_string(),
                "SHEET" => setup.sheet = value.to_string(),
                _ => {}
            }
        }
        // The first WD_M wins; a sheet carries two identical inserts in practice.
        break;
    }
    // A drawing created here has no WD_M block, so the setup dialog stores the
    // same contract as custom properties. A real WD_M always wins over them.
    if !setup.present {
        let property = |key: &str| {
            document
                .summary_info
                .custom_properties
                .iter()
                .find(|(name, _)| name.trim().eq_ignore_ascii_case(key))
                .map(|(_, value)| value.trim().to_string())
                .filter(|value| !value.is_empty())
        };
        if let Some(value) = property("ELEC_TAGFMT") {
            setup.tag_format = value;
        }
        if let Some(value) = property("ELEC_TAGMODE") {
            setup.tag_mode = value;
        }
        if let Some(value) = property("ELEC_TAG_START") {
            setup.tag_start = value.parse().unwrap_or(1);
        }
        if let Some(value) = property("ELEC_TAG_RSUF") {
            setup.tag_suffixes = value
                .split(',')
                .map(|part| part.trim().to_string())
                .filter(|part| !part.is_empty())
                .collect();
        }
        if let Some(value) = property("ELEC_WIREFMT") {
            setup.wire_format = value;
        }
        if let Some(value) = property("ELEC_WIREMODE") {
            setup.wire_mode = value;
        }
        if let Some(value) = property("ELEC_WIRE_START") {
            setup.wire_start = value.parse().unwrap_or(1);
        }
        if let Some(value) = property("ELEC_XREFFMT") {
            setup.xref_format = value;
        }
        if let Some(value) = property("ELEC_SHEET") {
            setup.sheet = value;
        }
        // The SkyCAD numbering extras have no WD_M attribute of their own.
        if let Some(value) = property("ELEC_TAGINC") {
            setup.tag_increment = value.parse().unwrap_or(1).max(1);
        }
        if let Some(value) = property("ELEC_TAGFALL") {
            setup.tag_fallthrough = !value.eq_ignore_ascii_case("No");
        }
        if let Some(value) = property("ELEC_WIREINC") {
            setup.wire_increment = value.parse().unwrap_or(1).max(1);
        }
        if let Some(value) = property("ELEC_WIREKIND") {
            setup.wire_alphabetical = value.eq_ignore_ascii_case("Alphabetical");
        }
        if let Some(value) = property("ELEC_WIREEMPTY") {
            setup.wire_empty_first = value.eq_ignore_ascii_case("Yes");
        }
        if let Some(value) = property("ELEC_TAGUNIQ") {
            setup.tag_unique = canonical_uniqueness(&value);
        }
    }
    setup
}


/// Store a setup on the drawing as custom properties (the `ELEC_*` contract).
///
/// The dialog and `ELECSETUP KEY=VALUE` both write through here, so the two cannot
/// drift apart, and `read_setup` reads exactly these keys back.
fn write_setup_properties(document: &mut codec::Document, setup: &ElecSetup) {
    let values = [
        ("ELEC_TAGFMT", setup.tag_format.trim().to_string()),
        ("ELEC_TAGMODE", setup.tag_mode.trim().to_string()),
        ("ELEC_TAG_START", setup.tag_start.to_string()),
        ("ELEC_TAG_RSUF", setup.tag_suffixes.join(",")),
        ("ELEC_WIREFMT", setup.wire_format.trim().to_string()),
        ("ELEC_WIREMODE", setup.wire_mode.trim().to_string()),
        ("ELEC_WIRE_START", setup.wire_start.to_string()),
        ("ELEC_XREFFMT", setup.xref_format.trim().to_string()),
        ("ELEC_SHEET", setup.sheet.trim().to_string()),
        ("ELEC_TAGINC", setup.tag_increment.max(1).to_string()),
        (
            "ELEC_TAGFALL",
            if setup.tag_fallthrough { "Yes" } else { "No" }.to_string(),
        ),
        ("ELEC_WIREINC", setup.wire_increment.max(1).to_string()),
        (
            "ELEC_WIREKIND",
            if setup.wire_alphabetical {
                "Alphabetical"
            } else {
                "Numerical"
            }
            .to_string(),
        ),
        (
            "ELEC_WIREEMPTY",
            if setup.wire_empty_first { "Yes" } else { "No" }.to_string(),
        ),
        ("ELEC_TAGUNIQ", canonical_uniqueness(&setup.tag_unique)),
    ];
    for (key, value) in values {
        if let Some(entry) = document
            .summary_info
            .custom_properties
            .iter_mut()
            .find(|(name, _)| name.trim().eq_ignore_ascii_case(key))
        {
            entry.1 = value;
        } else {
            document
                .summary_info
                .custom_properties
                .push((key.to_string(), value));
        }
    }
}


/// The last library folder a session loaded, for sibling modules.
pub(super) fn library_directory() -> Option<std::path::PathBuf> {
    library_dir().read().ok().and_then(|guard| guard.clone())
}

/// Library facts a system insertion needs: class and attribute tags.
pub(super) fn library_symbol(symbol: &str) -> Option<(String, Vec<String>, f64, f64)> {
    ensure_library_index();
    let index = library_index().read().ok()?;
    let entry = index.get(&symbol.trim().to_ascii_uppercase())?;
    Some((
        entry.class.clone(),
        entry.attributes.clone(),
        entry.scale,
        entry.label_height,
    ))
}

/// True when the loaded project is flagged as already built and wired.
fn project_is_built() -> bool {
    let Some(file) = project_file().read().ok().and_then(|guard| guard.clone()) else {
        return false;
    };
    let Ok(text) = std::fs::read_to_string(file) else {
        return false;
    };
    text.lines().any(|line| {
        let line = line.trim().to_ascii_lowercase();
        let Some(rest) = line.strip_prefix("built ") else {
            return false;
        };
        matches!(rest.trim(), "yes" | "on" | "true" | "1")
    })
}

/// The loaded project's folder, for sibling command modules (revision control).
pub(super) fn project_dir_for_revisions() -> Option<std::path::PathBuf> {
    project_dir().read().ok().and_then(|guard| guard.clone())
}

// ---------------------------------------------------------------------------------------
// Wire graph: wire segments, device pins and the hints that join them
// ---------------------------------------------------------------------------------------

/// Wire segments and device pins of one drawing, shared by every connectivity command.
///
/// Before this existed each command built its own list, and the lists disagreed: the wire
/// list saw *every* `LINE` in the drawing (symbol strokes, the sheet frame and the title
/// block included — 132 "wire segments" on a sheet that has 8), the terminals report saw
/// only attributed inserts, and no command knew where a symbol's pins were. A net list is
/// only as good as those two decisions, so they are made once, here.
struct WireGraph {
    wires: Vec<::elec::geometry::WireSeg>,
    anchors: Vec<::elec::geometry::Anchor>,
    hints: ::elec::geometry::NetlistHints,
}

impl WireGraph {
    fn netlist(&self, tolerance: f64) -> ::elec::geometry::GeoNetlist {
        ::elec::geometry::netlist_with_hints(
            &self.wires,
            &self.anchors,
            tolerance,
            &self.hints,
            true,
        )
    }

    fn label_of(&self, id: u64) -> String {
        self.anchors
            .iter()
            .find(|anchor| anchor.id == id)
            .map(|anchor| anchor.display_label())
            .unwrap_or_else(|| "?".to_string())
    }
}

/// A wire endpoint is a pin's connection point up to this distance.
///
/// Measured on the customer's sheets: an ACADE attribute pin is exact (0.000), while the
/// generated symbol library leaves its pin stub 0 units away but a hand-placed wire can be a
/// few thousandths off. 0.2 also covers the 0.125 symbol-pin gap the print files show, where
/// the wire stops short of the contact.
const PIN_ASSOC: f64 = 0.2;

/// Two wire ends on either side of one terminal block: the pass-through gap.
const TERMINAL_GAP: f64 = 0.14;

/// The midpoint of a terminal pass-through must be this close to the terminal block.
const TERMINAL_MIDPOINT: f64 = 0.09;

/// Wire length below which a layer-0 line is symbol geometry rather than a wire.
const FLAT_WIRE_MIN: f64 = 0.5;

/// The nearest of `points` to `target`, if it is within `max`.
fn nearest_point(
    points: &[::elec::geometry::Point],
    target: ::elec::geometry::Point,
    max: f64,
) -> Option<::elec::geometry::Point> {
    let mut best: Option<(f64, ::elec::geometry::Point)> = None;
    for point in points {
        let distance = ((point.x - target.x).powi(2) + (point.y - target.y).powi(2)).sqrt();
        if distance <= max && best.map(|(seen, _)| distance < seen).unwrap_or(true) {
            best = Some((distance, *point));
        }
    }
    best.map(|(_, point)| point)
}

/// The pins a block insert exposes, in world coordinates.
///
/// Two sources, in order of authority:
///
/// * ACADE wire-connection attributes (`X1TERM02`): the attribute's insertion point is the
///   pin, or its value when the writer stored `x,y` there (this app's PLC modules do);
/// * the block's own geometry: the ends of its lines and the cardinal points of its circles.
///   The generated symbol library carries no pin attributes, so a symbol placed with
///   `ELECINSERT` is anchored from its stubs.
///
/// The returned name is empty for geometry-derived pins: the drawing does not say which
/// number that point is, so the caller numbers the pins that actually find a wire.
fn insert_pins(
    document: &codec::Document,
    insert: &codec::entities::Insert,
) -> Vec<(::elec::geometry::Point, String)> {
    use ::elec::geometry::Point;

    let mut declared: Vec<(Point, String)> = Vec::new();
    for attribute in &insert.attributes {
        let Some(number) = ::elec::pin_number(&attribute.tag) else {
            continue;
        };
        let point = ::elec::parse_point(&attribute.value)
            .map(|(x, y)| Point::new(x, y))
            .unwrap_or_else(|| {
                Point::new(attribute.insertion_point.x, attribute.insertion_point.y)
            });
        declared.push((point, number));
    }
    if !declared.is_empty() {
        return declared;
    }

    let Some(record) = document.block_records.get(insert.block_name.as_str()) else {
        return Vec::new();
    };
    let base = record.base_point;
    let (scale_x, scale_y) = (insert.x_scale(), insert.y_scale());
    let (sin, cos) = insert.rotation.sin_cos();
    let to_world = |x: f64, y: f64| -> Point {
        let local_x = (x - base.x) * scale_x;
        let local_y = (y - base.y) * scale_y;
        Point::new(
            insert.insert_point.x + local_x * cos - local_y * sin,
            insert.insert_point.y + local_x * sin + local_y * cos,
        )
    };
    let mut points: Vec<Point> = Vec::new();
    for handle in &record.entity_handles {
        match document.get_entity(*handle) {
            Some(codec::EntityType::Line(line)) => {
                points.push(to_world(line.start.x, line.start.y));
                points.push(to_world(line.end.x, line.end.y));
            }
            Some(codec::EntityType::LwPolyline(polyline)) => {
                for vertex in &polyline.vertices {
                    points.push(to_world(vertex.location.x, vertex.location.y));
                }
            }
            Some(codec::EntityType::Circle(circle)) => {
                let (cx, cy) = (circle.center.x, circle.center.y);
                let radius = circle.radius;
                for (dx, dy) in [(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)] {
                    points.push(to_world(cx + dx * radius, cy + dy * radius));
                }
            }
            _ => {}
        }
    }
    // The block corners and the body ends are all in here; the netlist only keeps the ones a
    // wire actually reaches, so a candidate that is symbol outline costs nothing.
    points.sort_by(|left, right| {
        left.x
            .partial_cmp(&right.x)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(left.y.partial_cmp(&right.y).unwrap_or(std::cmp::Ordering::Equal))
    });
    points.dedup_by(|left, right| {
        (left.x - right.x).abs() <= 1e-6 && (left.y - right.y).abs() <= 1e-6
    });
    points
        .into_iter()
        .map(|point| (point, String::new()))
        .collect()
}

/// Every entity that draws one device: the block inserts carrying its tag, plus any `TEXT`
/// that spells the tag (a flattened sheet keeps only the text).
fn device_entities(document: &codec::Document, tag: &str) -> Vec<codec::Handle> {
    let wanted = tag.trim();
    if wanted.is_empty() {
        return Vec::new();
    }
    let mut handles = Vec::new();
    for entity in document.entities() {
        if !OpenCADStudio::in_drawing_space(document, entity.common().owner_handle) {
            continue;
        }
        match entity {
            codec::EntityType::Insert(insert) => {
                let matches = insert.attributes.iter().any(|attribute| {
                    is_tag_attribute(&attribute.tag)
                        && attribute.value.trim().eq_ignore_ascii_case(wanted)
                });
                if matches {
                    handles.push(insert.common.handle);
                }
            }
            codec::EntityType::Text(text) => {
                if !text.common.layer.eq_ignore_ascii_case("TITLE_BLOCK")
                    && text.value.trim().eq_ignore_ascii_case(wanted)
                {
                    handles.push(text.common.handle);
                }
            }
            _ => {}
        }
    }
    handles
}

/// Build the drawing's wires, device pins and connectivity hints.
///
/// The wire rule is the one measured on the customer's drawings: a sheet that keeps its
/// ACADE `WIRES` layer is read from that layer alone; a flattened print (30 of the 34
/// sheets) falls back to layer-0 lines at least half a unit long, because symbol strokes
/// are shorter than that and the frame is longer. Everything else on the sheet is not a
/// wire: `WIRENO` labels, symbol strokes, hatch and title block.
fn wire_graph(document: &codec::Document) -> WireGraph {
    use ::elec::geometry::{Anchor, NetlistHints, Point, WireSeg};

    let mut lines: Vec<(String, Point, Point)> = Vec::new();
    let mut polylines: Vec<(String, Vec<Point>)> = Vec::new();
    let mut terminals: Vec<Point> = Vec::new();
    let mut junctions: Vec<Point> = Vec::new();
    let mut inserts: Vec<&codec::entities::Insert> = Vec::new();
    let mut reference_texts: Vec<(String, f64, f64)> = Vec::new();

    for entity in document.entities() {
        if !OpenCADStudio::in_drawing_space(document, entity.common().owner_handle) {
            continue;
        }
        match entity {
            codec::EntityType::Line(line) => lines.push((
                line.common.layer.clone(),
                Point::new(line.start.x, line.start.y),
                Point::new(line.end.x, line.end.y),
            )),
            codec::EntityType::LwPolyline(polyline) => polylines.push((
                polyline.common.layer.clone(),
                polyline
                    .vertices
                    .iter()
                    .map(|vertex| Point::new(vertex.location.x, vertex.location.y))
                    .collect(),
            )),
            codec::EntityType::Insert(insert) => {
                if insert.block_name.eq_ignore_ascii_case("TERMINAL") {
                    terminals.push(Point::new(
                        insert.insert_point.x,
                        insert.insert_point.y,
                    ));
                }
                if insert.block_name.eq_ignore_ascii_case("DOT")
                    || insert.block_name.eq_ignore_ascii_case("WDDOT")
                {
                    junctions.push(Point::new(
                        insert.insert_point.x,
                        insert.insert_point.y,
                    ));
                }
                inserts.push(insert);
            }
            codec::EntityType::Circle(circle)
                if circle.common.layer.eq_ignore_ascii_case("JUNCTIONS") =>
            {
                junctions.push(Point::new(circle.center.x, circle.center.y));
            }
            codec::EntityType::Text(text) => {
                if text.common.layer.eq_ignore_ascii_case("TITLE_BLOCK") {
                    continue;
                }
                let value = text.value.trim();
                let height = text.height.abs();
                if value.is_empty() || !(0.09..=0.26).contains(&height) {
                    continue;
                }
                if OpenCADStudio::looks_like_reference(value) {
                    reference_texts.push((
                        value.to_string(),
                        text.insertion_point.x,
                        text.insertion_point.y,
                    ));
                }
            }
            _ => {}
        }
    }

    let has_wire_layer = lines
        .iter()
        .any(|(layer, _, _)| ::elec::is_wire_layer(layer))
        || polylines
            .iter()
            .any(|(layer, _)| ::elec::is_wire_layer(layer));

    let mut wires: Vec<WireSeg> = Vec::new();
    let mut next_wire = 0u64;
    let keep = |layer: &str, a: Point, b: Point| -> bool {
        if has_wire_layer {
            ::elec::is_wire_layer(layer)
        } else {
            (layer.trim().is_empty() || layer.trim() == "0")
                && (a.x - b.x).hypot(a.y - b.y) >= FLAT_WIRE_MIN
        }
    };
    for (layer, from, to) in &lines {
        if !keep(layer, *from, *to) {
            continue;
        }
        next_wire += 1;
        wires.push(WireSeg::new(next_wire, *from, *to));
    }
    for (layer, points) in &polylines {
        for pair in points.windows(2) {
            if !keep(layer, pair[0], pair[1]) {
                continue;
            }
            next_wire += 1;
            wires.push(WireSeg::new(next_wire, pair[0], pair[1]));
        }
    }

    // Device pins, then the wire endpoint each pin reaches.
    let mut endpoints: Vec<Point> = Vec::new();
    for wire in &wires {
        endpoints.push(wire.from);
        endpoints.push(wire.to);
    }

    // Terminal pass-through: two wire ends on opposite sides of one terminal block.
    let mut bridges: Vec<(Point, Point)> = Vec::new();
    for terminal in &terminals {
        let near: Vec<Point> = endpoints
            .iter()
            .copied()
            .filter(|point| {
                (point.x - terminal.x).hypot(point.y - terminal.y) <= TERMINAL_GAP
            })
            .collect();
        for (index, first) in near.iter().enumerate() {
            for second in near.iter().skip(index + 1) {
                let gap = (first.x - second.x).hypot(first.y - second.y);
                if gap <= 0.0 || gap > TERMINAL_GAP {
                    continue;
                }
                let midpoint = Point::new((first.x + second.x) / 2.0, (first.y + second.y) / 2.0);
                if (midpoint.x - terminal.x).hypot(midpoint.y - terminal.y) <= TERMINAL_MIDPOINT {
                    bridges.push((*first, *second));
                }
            }
        }
    }

    let mut anchors: Vec<Anchor> = Vec::new();
    let mut next_anchor = 0u64;
    for insert in &inserts {
        if is_setup_block(&insert.block_name) {
            continue;
        }
        let has_attributes = !insert.attributes.is_empty();
        if !has_attributes && ::elec::is_non_device_block(&insert.block_name) {
            continue;
        }
        let mut tag = insert
            .attributes
            .iter()
            .find(|attribute| is_tag_attribute(&attribute.tag))
            .map(|attribute| attribute.value.trim().to_string())
            .unwrap_or_default();
        if tag.is_empty() {
            // A symbol whose attributes were exploded keeps only its tag TEXT nearby.
            let mut best: Option<(f64, String)> = None;
            for (value, x, y) in &reference_texts {
                let distance = (x - insert.insert_point.x).hypot(y - insert.insert_point.y);
                if distance <= 1.0 && best.as_ref().map(|(seen, _)| distance < *seen).unwrap_or(true)
                {
                    best = Some((distance, value.clone()));
                }
            }
            if let Some((_, value)) = best {
                tag = value;
            }
        }
        let label = if tag.is_empty() {
            insert.block_name.clone()
        } else {
            tag
        };

        let pins = insert_pins(document, insert);
        if pins.is_empty() {
            continue;
        }
        // A declared pin (attribute) stays visible even when no wire reaches it; a geometry
        // candidate only becomes a pin when a wire ends on it. Two candidates can land on
        // the same endpoint, so the endpoint wins once.
        let mut accepted: Vec<(Point, String)> = Vec::new();
        for (point, number) in &pins {
            let endpoint = if number.is_empty() {
                nearest_point(&endpoints, *point, PIN_ASSOC)
            } else {
                Some(nearest_point(&endpoints, *point, PIN_ASSOC).unwrap_or(*point))
            };
            let Some(endpoint) = endpoint else {
                continue;
            };
            if accepted
                .iter()
                .any(|(seen, _)| (seen.x - endpoint.x).abs() <= 1e-6 && (seen.y - endpoint.y).abs() <= 1e-6)
            {
                // Two pins of one symbol can reach the same wire end (a contact's pins sit
                // 0.125 apart, so both are within PIN_ASSOC of the one end the wire reaches).
                // The declared pin that did not get the wire keeps its own position and is
                // reported unconnected rather than vanishing from the list.
                if !number.is_empty() {
                    accepted.push((*point, number.clone()));
                }
                continue;
            }
            accepted.push((endpoint, number.clone()));
        }
        accepted.sort_by(|left, right| {
            left.0
                .x
                .partial_cmp(&right.0.x)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(left.0.y.partial_cmp(&right.0.y).unwrap_or(std::cmp::Ordering::Equal))
        });
        for (index, (point, number)) in accepted.iter().enumerate() {
            next_anchor += 1;
            let pin = if number.is_empty() {
                (index + 1).to_string()
            } else {
                number.clone()
            };
            anchors.push(Anchor::new(next_anchor, &label, *point).with_pin(&pin));
        }
    }

    WireGraph {
        wires,
        anchors,
        hints: NetlistHints {
            bridges,
            junctions,
            require_junctions: true,
        },
    }
}

/// Every device pin in the drawing as `(x, y, label)`.
///
/// Other command families (the cable schedule) resolve an endpoint to a device with the same
/// pin rules the netlist uses, so a cable end and a wire end name a device identically.
pub(super) fn device_pin_points(document: &codec::Document) -> Vec<(f64, f64, String)> {
    let graph = wire_graph(document);
    graph
        .anchors
        .iter()
        .map(|anchor| (anchor.at.x, anchor.at.y, anchor.display_label()))
        .collect()
}

/// The drawing's electrical content as one JSON value.
///
/// The node graph, the automation read op and the reports all answer from the same
/// [`wire_graph`], so a node, an MCP client and `ELECWIRELIST` never disagree about what a
/// net is. The value is deliberately flat, stable JSON: nets carry pins and wire counts,
/// pins carry their device coordinates, components carry the attributes a BOM needs.
pub(crate) fn graph_electrical(document: &codec::Document) -> serde_json::Value {
    let graph = wire_graph(document);
    let netlist = graph.netlist(PIN_ASSOC);
    let wire_length: f64 = graph
        .wires
        .iter()
        .map(|wire| {
            ((wire.to.x - wire.from.x).powi(2) + (wire.to.y - wire.from.y).powi(2)).sqrt()
        })
        .sum();
    let pins: Vec<serde_json::Value> = graph
        .anchors
        .iter()
        .map(|anchor| {
            serde_json::json!({
                "label": anchor.display_label(),
                "device": anchor.label,
                "pin": anchor.pin,
                "x": anchor.at.x,
                "y": anchor.at.y,
            })
        })
        .collect();
    let nets: Vec<serde_json::Value> = netlist
        .nets
        .iter()
        .enumerate()
        .map(|(index, net)| {
            let labels: Vec<String> = net.anchors.iter().map(|id| graph.label_of(*id)).collect();
            serde_json::json!({
                "name": format!("N{}", index + 1),
                "pins": labels,
                "wire_count": net.wires.len(),
            })
        })
        .collect();
    let loose: Vec<String> = netlist
        .loose_anchors
        .iter()
        .map(|id| graph.label_of(*id))
        .collect();
    let components: Vec<serde_json::Value> = OpenCADStudio::components_of(document)
        .iter()
        .map(|component| {
            serde_json::json!({
                "tag": component.tag,
                "class": component.class,
                "part": component.part,
                "manufacturer": component.manufacturer,
                "description": component.description,
            })
        })
        .collect();
    serde_json::json!({
        "wire_segments": graph.wires.len(),
        "wire_length": (wire_length * 1000.0).round() / 1000.0,
        "nets": nets,
        "pins": pins,
        "loose_pins": loose,
        "components": components,
    })
}

/// The device tags a project marks with one option code (`option <tag> <code>` lines).
fn project_option_tags(option: &str) -> BTreeSet<String> {
    let mut tags: BTreeSet<String> = BTreeSet::new();
    let Some(file) = project_file().read().ok().and_then(|guard| guard.clone()) else {
        return tags;
    };
    let Ok(text) = std::fs::read_to_string(file) else {
        return tags;
    };
    let wanted = option.trim();
    for line in text.lines() {
        let Some(rest) = line.trim().strip_prefix("option ") else {
            continue;
        };
        let mut words = rest.split_whitespace();
        let (Some(tag), Some(code)) = (words.next(), words.next()) else {
            continue;
        };
        if code.eq_ignore_ascii_case(wanted) {
            tags.insert(tag.trim().to_ascii_uppercase());
        }
    }
    tags
}

/// Tagged symbol inserts with their positions, for `ELECSYSTEM SAVE`.
pub(super) fn system_devices(document: &codec::Document) -> Vec<(String, String, f64, f64)> {
    let spaces = OpenCADStudio::drawing_spaces(document);
    let mut out = Vec::new();
    for entity in document.entities() {
        if !spaces.contains(&entity.common().owner_handle) {
            continue;
        }
        let codec::EntityType::Insert(insert) = entity else {
            continue;
        };
        if insert.attributes.is_empty() || is_setup_block(&insert.block_name) {
            continue;
        }
        let tag = insert
            .attributes
            .iter()
            .find(|attribute| is_tag_attribute(&attribute.tag))
            .map(|attribute| attribute.value.trim().to_string())
            .unwrap_or_default();
        if tag.is_empty() {
            continue;
        }
        out.push((
            tag,
            insert.block_name.clone(),
            insert.insert_point.x,
            insert.insert_point.y,
        ));
    }
    out
}

/// The drawing's wire segments as `(x1, y1, x2, y2)`, for `ELECSYSTEM SAVE`.
pub(super) fn wire_segments(document: &codec::Document) -> Vec<(f64, f64, f64, f64)> {
    wire_graph(document)
        .wires
        .into_iter()
        .map(|wire| (wire.from.x, wire.from.y, wire.to.x, wire.to.y))
        .collect()
}

/// A readable class name for a symbol when no library manifest is available.
///
/// `CONTACT_NO` becomes `Contact No`: a report should not shout the raw block
/// name at the reader, even when it has no measured class to show.
fn friendly_class(block_name: &str) -> String {
    let mut words: Vec<String> = Vec::new();
    for word in block_name.split(|character| character == '_' || character == '-' || character == ' ') {
        let word = word.trim();
        if word.is_empty() {
            continue;
        }
        let mut characters = word.chars();
        let Some(first) = characters.next() else {
            continue;
        };
        let mut friendly = String::new();
        friendly.extend(first.to_uppercase());
        friendly.push_str(&characters.as_str().to_lowercase());
        words.push(friendly);
    }
    if words.is_empty() {
        block_name.to_string()
    } else {
        words.join(" ")
    }
}

impl OpenCADStudio {
    /// Show the read-only electrical report dialog.
    ///
    /// The report commands print to the command line for scripts; the ribbon
    /// buttons open this dialog so pressing "Component List", "Terminal
    /// Strips", "Wire List", "Bill of Material" or "Parts Library" has a
    /// visible result instead of looking like nothing happened. Both outputs
    /// are kept in sync: every line goes to the command line and to the body.
    pub(crate) fn show_elec_report(&mut self, title: impl Into<String>, lines: &[String]) {
        let title = title.into();
        for line in lines {
            self.command_line.push_output(line);
        }
        // A scripted/dialog-less run (`--batch`, `--each`) must stay headless:
        // a modal would swallow the following rows (QSAVE included) and leave
        // the drawing unsaved, so the command-line log is the whole result.
        if self.script_running {
            return;
        }
        self.elec_report = crate::ui::window::elec_report::ElecReport {
            title,
            summary: lines.to_vec(),
            ..crate::ui::window::elec_report::ElecReport::default()
        };
        self.active_modal = Some(crate::app::ModalKind::ElecReport);
        self.reset_modal_geometry();
    }

    /// Show the report dialog as a table.
    ///
    /// `log` is the command-line/script output; it stays byte-for-byte the
    /// text the command used to print. The dialog shows `summary` above a
    /// `columns` x `rows` table instead. `class_column` names the column the
    /// dialog's class chips filter on (the component class or part class).
    pub(crate) fn set_elec_report_table(
        &mut self,
        title: impl Into<String>,
        log: &[String],
        summary: Vec<String>,
        columns: Vec<String>,
        rows: Vec<Vec<String>>,
        class_column: Option<usize>,
    ) {
        for line in log {
            self.command_line.push_output(line);
        }
        if self.script_running {
            return;
        }
        self.elec_report = crate::ui::window::elec_report::ElecReport {
            title: title.into(),
            summary,
            columns,
            rows,
            class_column,
            search: String::new(),
            class_filter: None,
            filter_label: String::new(),
        };
        self.active_modal = Some(crate::app::ModalKind::ElecReport);
        self.reset_modal_geometry();
    }
}

/// Write a catalogue back to `library.tsv`, keeping the column order that
/// [`read_library_manifest`] expects (symbol, class, attributes, scale).
fn write_library_manifest(
    directory: &std::path::Path,
    index: &std::collections::BTreeMap<String, LibraryEntry>,
) -> std::io::Result<usize> {
    let mut text = String::from("symbol\tclass\tattributes\tscale\n");
    let mut entries: Vec<&LibraryEntry> = index.values().collect();
    entries.sort_by(|left, right| left.name.to_lowercase().cmp(&right.name.to_lowercase()));
    for entry in &entries {
        text.push_str(&entry.name);
        text.push('\t');
        text.push_str(&entry.class);
        text.push('\t');
        text.push_str(&entry.attributes.join("|"));
        text.push('\t');
        text.push_str(&format!("{}", entry.scale));
        text.push('\n');
    }
    std::fs::write(directory.join("library.tsv"), text)?;
    Ok(entries.len())
}

impl OpenCADStudio {
    /// Folder the symbol manager lists, if a library has been loaded.
    pub(crate) fn library_manager_folder(&self) -> Option<std::path::PathBuf> {
        library_directory().or_else(remembered_library)
    }

    /// Catalogue rows for the symbol manager: name, class, pin count, scale.
    pub(crate) fn library_manager_symbols(&self) -> Vec<(String, String, usize, f64)> {
        let Ok(index) = library_index().read() else {
            return Vec::new();
        };
        let mut rows: Vec<(String, String, usize, f64)> = index
            .values()
            .map(|entry| {
                let pins = entry
                    .attributes
                    .iter()
                    .filter(|tag| tag.to_ascii_uppercase().contains("TERM"))
                    .count();
                (entry.name.clone(), entry.class.clone(), pins, entry.scale)
            })
            .collect();
        rows.sort_by(|left, right| left.0.to_lowercase().cmp(&right.0.to_lowercase()));
        rows
    }

    /// Rename a symbol's class in the catalogue file and in memory.
    pub(crate) fn library_manager_save_class(&mut self, symbol: &str, class: &str) -> String {
        let Some(folder) = self.library_manager_folder() else {
            return "No library folder is loaded.".to_string();
        };
        let class = class.trim();
        if class.is_empty() {
            return "The class cannot be empty.".to_string();
        }
        let mut index = read_library_manifest(&folder);
        let Some(entry) = index.get_mut(&symbol.to_ascii_uppercase()) else {
            return format!("{symbol} is not in this catalogue.");
        };
        entry.class = class.to_string();
        let message = match write_library_manifest(&folder, &index) {
            Ok(_) => format!("Saved class \"{class}\" for {symbol}."),
            Err(error) => format!("Could not write library.tsv: {error}"),
        };
        if let Ok(mut guard) = library_index().write() {
            if let Some(memory) = guard.get_mut(&symbol.to_ascii_uppercase()) {
                memory.class = class.to_string();
            }
            for (key, entry) in index {
                if let Some(memory) = guard.get_mut(&key) {
                    memory.class = entry.class;
                }
            }
        }
        message
    }

    /// Remove a symbol's DXF and catalogue row.
    pub(crate) fn library_manager_delete(&mut self, symbol: &str) -> String {
        let Some(folder) = self.library_manager_folder() else {
            return "No library folder is loaded.".to_string();
        };
        let mut index = read_library_manifest(&folder);
        let key = symbol.to_ascii_uppercase();
        let Some(entry) = index.remove(&key) else {
            return format!("{symbol} is not in this catalogue.");
        };
        let mut removed_file = false;
        for candidate in [
            format!("{}.dxf", entry.name),
            format!("{}.DXF", entry.name),
            format!("{symbol}.dxf"),
        ] {
            let path = folder.join(candidate);
            if path.is_file() {
                match std::fs::remove_file(&path) {
                    Ok(()) => removed_file = true,
                    Err(error) => return format!("Could not delete {}: {error}", path.display()),
                }
                break;
            }
        }
        let note = match write_library_manifest(&folder, &index) {
            Ok(_) => String::new(),
            Err(error) => return format!("Could not write library.tsv: {error}"),
        };
        if let Ok(mut guard) = library_index().write() {
            guard.remove(&key);
        }
        let file_note = if removed_file {
            "and its DXF"
        } else {
            "from the catalogue (its DXF was not found)"
        };
        format!("Deleted {symbol} {file_note}.{note}")
    }

    /// Reveal the library folder in the system file browser.
    pub(crate) fn library_manager_open_folder(&self) {
        let Some(folder) = self.library_manager_folder() else {
            return;
        };
        #[cfg(target_os = "windows")]
        {
            let _ = std::process::Command::new("explorer").arg(&folder).spawn();
        }
        #[cfg(target_os = "macos")]
        {
            let _ = std::process::Command::new("open").arg(&folder).spawn();
        }
        #[cfg(all(unix, not(target_os = "macos")))]
        {
            let _ = std::process::Command::new("xdg-open").arg(&folder).spawn();
        }
    }

    /// Fill the symbol manager's row list from the catalogue.
    pub(crate) fn refresh_library_manager(&mut self) {
        self.elec_lib_manager.folder = self
            .library_manager_folder()
            .map(|folder| folder.display().to_string())
            .unwrap_or_default();
        self.elec_lib_manager.rows = self
            .library_manager_symbols()
            .into_iter()
            .map(|(name, class, pins, scale)| {
                crate::ui::window::elec_lib_manager::LibSymbolRow {
                    name,
                    class,
                    pins,
                    scale,
                }
            })
            .collect();
        if self.elec_lib_manager.selected.is_none() && !self.elec_lib_manager.rows.is_empty() {
            self.elec_lib_manager.selected =
                Some(self.elec_lib_manager.rows[0].name.clone());
        }
        self.elec_lib_manager.class_edit = self
            .elec_lib_manager
            .selected
            .as_ref()
            .and_then(|name| {
                self.elec_lib_manager
                    .rows
                    .iter()
                    .find(|row| &row.name == name)
            })
            .map(|row| row.class.clone())
            .unwrap_or_default();
    }

    /// Open the symbol manager with its list refreshed.
    pub(crate) fn open_library_manager(&mut self) {
        self.elec_lib_manager.confirm_delete = false;
        self.elec_lib_manager.status.clear();
        if let Some(folder) = self.library_manager_folder() {
            let path = folder.display().to_string();
            let i = self.active_tab;
            let loaded = library_index()
                .read()
                .map(|index| !index.is_empty())
                .unwrap_or(false);
            if !loaded {
                let _ = self.elec_lib(i, &path);
            }
        }
        self.refresh_library_manager();
        self.active_modal = Some(crate::app::ModalKind::ElecLibManager);
        self.reset_modal_geometry();
    }
}

impl OpenCADStudio {
    /// Turn every block insert in the active tab into a component.
    ///
    /// Inserts with no attributes are skipped: a bare block is drawing furniture (a
    /// border, a title block, a graphic) rather than an electrical device, and counting
    /// them would make the bill of material meaningless.
/// Components of any document, so the same extraction serves one drawing and a project.
pub(super) fn components_of(document: &codec::Document) -> Vec<Component> {
    let mut out = Vec::new();
    let mut next_id = 0u64;
    // Reports opened straight from a saved project must know the library's class
    // names, not just the block names the drawing stores.
    ensure_library_index();
    for entity in document.entities() {
        if !Self::in_drawing_space(document, entity.common().owner_handle) {
            continue;
        }
        let codec::EntityType::Insert(insert) = entity else {
            continue;
        };
        if insert.attributes.is_empty() || is_setup_block(&insert.block_name) {
            continue;
        }
        next_id += 1;
        // Report the class the library gives the symbol (`CONT-NO` is a `Contactor`), not the
        // block name: a bill of material should say what the part *is*. A drawing read
        // without a loaded library still lists something useful rather than nothing.
        let class = library_index()
            .read()
            .ok()
            .and_then(|index| {
                index
                    .get(&insert.block_name.trim().to_ascii_uppercase())
                    .map(|entry| entry.class.clone())
            })
            .unwrap_or_else(|| friendly_class(&insert.block_name));
        let mut component = Component::new(next_id, &class, "");
        for attribute in &insert.attributes {
            let tag = attribute.tag.trim();
            if tag.is_empty() {
                continue;
            }
            let value = attribute.value.trim();
            if value.is_empty() {
                continue;
            }
            match field_for_attribute(tag) {
                Some("tag") => component.tag = value.to_string(),
                Some("part") => component.part = Some(value.to_string()),
                Some("manufacturer") => component.manufacturer = Some(value.to_string()),
                Some("description") => component.description = Some(value.to_string()),
                Some("location") => component.location = Some(value.to_string()),
                _ => {}
            }
            component = component.with_attribute(tag, value);
        }
        out.push(component);
    }

    // AutoCAD Electrical sheets that were exploded (or plotted after a flatten) keep no
    // attributes on the symbols: the tag survives only as plain TEXT. A reference
    // designator is a device with or without its symbol, so recover those too — otherwise
    // a real customer sheet reports "no components found" while the tags are right there.
    let mut known: std::collections::BTreeSet<String> = out
        .iter()
        .map(|component| component.tag.trim().to_ascii_uppercase())
        .filter(|tag| !tag.is_empty())
        .collect();
    let mut references: Vec<(String, f64, f64)> = Vec::new();
    let mut descriptions: Vec<(String, f64, f64)> = Vec::new();
    for entity in document.entities() {
        if !Self::in_drawing_space(document, entity.common().owner_handle) {
            continue;
        }
        let codec::EntityType::Text(text) = entity else {
            continue;
        };
        if text.common.layer.eq_ignore_ascii_case("TITLE_BLOCK") {
            continue;
        }
        let value = text.value.trim();
        if value.is_empty() {
            continue;
        }
        // Project text sits at 0.125 or 0.15 units; anything far outside that is a title or
        // a note rather than a device label.
        let height = text.height.abs();
        if !(0.09..=0.26).contains(&height) {
            continue;
        }
        let point = (text.insertion_point.x, text.insertion_point.y);
        if Self::looks_like_reference(value) {
            references.push((value.to_string(), point.0, point.1));
        } else if value.chars().any(|character| character.is_alphabetic())
            && !Self::is_note_label(value)
        {
            descriptions.push((value.to_string(), point.0, point.1));
        }
    }
    for (tag, x, y) in references {
        // Coil and contacts share one tag: one device, however many parts are drawn.
        if !known.insert(tag.to_ascii_uppercase()) {
            continue;
        }
        next_id += 1;
        let mut component = Component::new(next_id, Self::class_for_reference(&tag), &tag);
        // The description is a separate TEXT that sits just below its tag; a note or a legend
        // elsewhere on the sheet is not this device's description, so only look underneath.
        let nearest = descriptions
            .iter()
            .filter(|(_, dx, dy)| {
                let below = dy - y;
                (-1.6..=-0.05).contains(&below) && (dx - x).abs() <= 1.6
            })
            .map(|(text, dx, dy)| {
                let distance = ((dx - x).powi(2) + (dy - y).powi(2)).sqrt();
                (text, distance)
            })
            .min_by(|left, right| {
                left.1
                    .partial_cmp(&right.1)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
        if let Some((text, _)) = nearest {
            component.description = Some(text.clone());
        }
        out.push(component);
    }
    out
}

/// True for a string shaped like an IEC 81346 reference designator rather than prose.
///
/// The three shapes that occur on real sheets: `K12` (family then number), `326M` (module
/// reference), and `XXX` (a device that has not been numbered yet). The exclusions are what
/// keeps part numbers, phase labels, ratings and notes out of the component list.
fn looks_like_reference(text: &str) -> bool {
    let value = text.trim();
    if value.is_empty() || value.len() > 9 {
        return false;
    }
    // A hyphen almost always means a catalogue number (`LPJ-100SP`, `TS-1`), not a tag.
    if value.contains('-') {
        return false;
    }
    let characters: Vec<char> = value.chars().collect();
    if !characters
        .iter()
        .all(|character| character.is_ascii_digit() || character.is_ascii_uppercase())
    {
        return false;
    }
    // `XXX` / `XXXXA`: reserved for a device still awaiting a number.
    if characters.len() >= 3 && characters[..3].iter().all(|character| *character == 'X') {
        return true;
    }
    let leading_digits = characters
        .iter()
        .take_while(|character| character.is_ascii_digit())
        .count();
    let trailing_letters = characters
        .iter()
        .rev()
        .take_while(|character| character.is_ascii_uppercase())
        .count();
    if (3..=5).contains(&leading_digits)
        && (2..=4).contains(&trailing_letters)
        && leading_digits + trailing_letters == characters.len()
    {
        let suffix: String = characters[leading_digits..].iter().collect();
        return !matches!(suffix.as_str(), "VAC" | "VDC" | "AMP" | "HZ" | "AWG" | "RPM");
    }
    let leading_letters = characters
        .iter()
        .take_while(|character| character.is_ascii_uppercase())
        .count();
    let trailing_digits = characters
        .iter()
        .rev()
        .take_while(|character| character.is_ascii_digit())
        .count();
    if (1..=4).contains(&leading_letters)
        && (1..=4).contains(&trailing_digits)
        && leading_letters + trailing_digits == characters.len()
    {
        let prefix: String = characters[..leading_letters].iter().collect();
        return !matches!(
            prefix.as_str(),
            "L" | "IN" | "OUT" | "CH" | "IP" | "TYP" | "REF" | "MIN" | "MAX" | "AWG"
        );
    }
    false
}

/// True when the owner is a drawing space: model space or a layout's paper space.
///
/// The electrical collectors must read what the user is looking at. Placing a symbol on a
/// layout sheet and then being told "no components found" is exactly the bolted-on
/// behaviour this layer must not have; a null owner is accepted too, because some writers
/// leave the `330` group out.
/// Handles of every space an entity can live in: model space plus each layout's paper
/// space. The collectors use this instead of comparing to model space alone, because a
/// symbol drawn on a layout sheet is still part of the drawing.
fn drawing_spaces(document: &codec::Document) -> Vec<codec::Handle> {
    let mut spaces = vec![document.header.model_space_block_handle];
    for record in document.block_records.iter() {
        let name = record.name.to_ascii_lowercase();
        if (name == "*model_space" || name.starts_with("*paper_space"))
            && !spaces.contains(&record.handle)
        {
            spaces.push(record.handle);
        }
    }
    spaces
}

fn in_drawing_space(document: &codec::Document, owner: codec::Handle) -> bool {
    if owner.is_null() {
        return true;
    }
    document.block_records.iter().any(|record| {
        record.handle == owner
            && (record.name.eq_ignore_ascii_case("*Model_Space")
                || record.name.to_ascii_lowercase().starts_with("*paper_space"))
    })
}

/// True for sheet furniture that must never become a device description (`TYP.`, `BLOCK`…).
fn is_note_label(value: &str) -> bool {
    let upper = value.trim().to_ascii_uppercase();
    if upper.starts_with("SEE ") {
        return true;
    }
    matches!(
        upper.as_str(),
        "TYP." | "TYP" | "BLOCK" | "LOCATED IN" | "CONTINUED" | "WIRE" | "WIRES" | "N/A"
    )
}

/// The device class an IEC 81346 reference letter stands for, for tags recovered from text.
///
/// Two shapes occur: family-first (`K12`) and number-first (`264CB`, where the family is the
/// trailing letters). Longest known family matching wins so `CB` is a breaker before `C`.
fn class_for_reference(tag: &str) -> &'static str {
    let letters: String = tag
        .chars()
        .filter(|character| character.is_ascii_uppercase())
        .collect();
    let number_first = tag.starts_with(|character: char| character.is_ascii_digit());
    let family = if number_first {
        letters.as_str()
    } else {
        &letters[..letters.len().min(1)]
    };
    if family.starts_with("PDB") {
        return "Power Distribution";
    }
    if family.starts_with("CB") {
        return "Circuit Breaker";
    }
    if family.starts_with("FB") {
        return "Fuse";
    }
    if family.starts_with("CON") {
        return "Connector";
    }
    if family.starts_with("PB") {
        return "Pushbutton";
    }
    if family.starts_with("OL") {
        return "Overload";
    }
    if family.starts_with("PS") {
        return "Power Supply";
    }
    match family.chars().next() {
        Some('A') => "Assembly",
        Some('B') => "Sensor",
        Some('C') => "Capacitor",
        Some('E') => "Other",
        Some('F') => "Fuse",
        Some('G') => "Power Supply",
        Some('H') => "Lamp",
        Some('K') => "Contactor",
        Some('L') => "Inductor",
        Some('M') => "Motor",
        Some('P') => "Indicator",
        Some('Q') => "Circuit Breaker",
        Some('R') => "Resistor",
        Some('S') => "Switch",
        Some('T') => "Transformer",
        Some('U') => "Drive",
        Some('W') => "Cable",
        Some('X') => "Terminal",
        _ => "Device",
    }
}


    fn elec_components(&self, i: usize) -> Vec<Component> {
        Self::components_of(&self.tabs[i].scene.document)
    }

    /// `ELECHELP` lists the electrical commands with a one-line usage each.
    ///
    /// The ribbon is the fast path; this is what a user reaches for when they know the job
    /// but not the command name. Every line names a command the dispatcher really handles,
    /// so the reference cannot drift without the list looking obviously wrong in review.
    pub(super) fn elec_help(&mut self) -> Task<Message> {
        let sections: [(&str, &[&str]); 12] = [
            (
                "Symbols",
                &[
                    "ELECLIB <folder>              load a folder of symbol DXFs",
                    "ELECPALETTE                   browse the loaded symbols",
                    "ELECINSERT <symbol>           place a symbol",
                    "ELECMAKESYMBOL <name> [CLASS=x] [PINS=n]  turn the selection into a symbol",
                    "ELECSETATTR <tag> NAME=value  edit a device's attributes",
                ],
            ),
            (
                "Local parts",
                &[
                    "ELECPARTS [folder]            load the local parts catalogue",
                    "ELECPARTLIST [filter]         search the catalogue",
                    "ELECASSIGN <tag> <part>       attach a catalogue part to a device",
                ],
            ),
            (
                "Wiring",
                &[
                    "ELECWIRE                      draw conductors (chained)",
                    "ELECJUNCTIONS                 place junction dots",
                    "ELECWIRELIST [out.csv]        report the nets, optionally export the wire list",
                    "ELECWIRENUMBER                number the nets",
                ],
            ),
            (
                "Devices",
                &[
                    "ELECLIST                      list the drawing's components",
                    "ELECNUMBER / ELECNUMBERPREVIEW  number devices / preview",
                    "ELECPLC <tag> <points> <base> place a PLC module",
                    "ELECPLCNEXT <tag> <points>    continue I/O addressing",
                    "ELECIO                        per-point cross references",
                    "ELECIOIMPORT <xlsx> [x,y]     generate PLC drawings from an I/O list",
                ],
            ),
            (
                "Terminals",
                &[
                    "ELECTERMINALS                 report terminal strips",
                    "ELECTSIDES                    assign terminal sides",
                ],
            ),
            (
                "Cross-reference",
                &[
                    "ELECXREF                      contact-level references",
                    "ELECXREFWRITE                 write the labels on the drawing",
                ],
            ),
            (
                "Navigation",
                &[
                    "ELECHIGHLIGHT <tag>           select every part of one device",
                    "ELECGOTO <tag>                centre the view on a device",
                ],
            ),
            (
                "Configuration",
                &[
                    "ELECSYSTEM [LIST]             list reusable systems",
                    "ELECSYSTEM INSERT <name> [x,y]  place a system and retag its devices",
                    "ELECVARIANT NEW|LIST|USE <name>  project variants beside the master",
                    "ELECOPTION <tag> <option>     mark a device with an option code",
                ],
            ),
            (
                "Cables",
                &[
                    "ELECCABLEDRAW <tag> [CORES=] [PART=] [SHIELD=yes]  draw the cable route",
                    "ELECCABLE <tag> CORES=<n> [PART=] [MFG=] [LEN=] [DESC=]",
                    "ELECCABLELIST [out.csv]       print or export the cable schedule",
                    "ELECCABLEDEL <tag>            remove a cable",
                ],
            ),
            (
                "Reports",
                &[
                    "ELECBOM [out.csv] [OPTION=<code>]  bill of material, optionally for one option code",
                    "ELECPANEL <out.dxf>           panel arrangement drawing",
                    "ELECREPORT <out.xlsx>         workbook report",
                    "ELECPART <parts.csv>          load a plain parts catalogue",
                ],
            ),
            (
                "Project & sheet",
                &[
                    "ELECSHEET [A4|A3|A2|A1] [title]  frame and title block",
                    "ELECNEW [template.dxf]        new drawing",
                    "ELECPROJNEW/OPEN/LIST         project file commands",
                    "ELECPROJNUM [apply]           renumber across the project",
                    "ELECPROJECT [out.xlsx]        project report",
                    "ELECBUILT [on|off]            built-project control",
                    "ELECREV SNAPSHOT|LIST|COMPARE|RESTORE  revision control",
                    "ELECSETUP                     show this drawing's electrical setup",
                    "ELECSETUP TAGINC=2 WIREKIND=Alphabetical ...  store the setup headlessly",
                    "ELECSETUPEDIT                 edit the numbering formats and counters",
                ],
            ),
            (
                "This list",
                &["ELECHELP                      this reference"],
            ),
        ];
        self.command_line
            .push_output("ELECHELP: electrical commands (each is also on the Electrical ribbon)");
        for (title, lines) in sections {
            self.command_line.push_output(&format!("  {title}"));
            for line in lines {
                self.command_line.push_output(&format!("    {line}"));
            }
        }
        Task::none()
    }

    /// `ELECWIRELABELS [out.csv]` exports the wire-number labels for printing.
    ///
    /// A wire number is TEXT on the `WIRENO` layer (that is what `ELECWIRENUMBER` writes), so
    /// the label list is those texts with their sheet positions — the list a wireman prints
    /// and fits on the conductors. SkyCAD lists this as "exportable wire label lists".
    pub(super) fn elec_wire_labels(&mut self, i: usize, out: Option<&str>) -> Task<Message> {
        const LABEL_LAYER: &str = "WIRENO";
        let mut labels: Vec<(String, f64, f64)> = Vec::new();
        {
            let doc = &self.tabs[i].scene.document;
            let spaces = Self::drawing_spaces(doc);
            for entity in doc.entities() {
                if !spaces.contains(&entity.common().owner_handle) {
                    continue;
                }
                let codec::EntityType::Text(text) = entity else {
                    continue;
                };
                if !text.common.layer.eq_ignore_ascii_case(LABEL_LAYER) {
                    continue;
                }
                let value = text.value.trim().to_string();
                if value.is_empty() {
                    continue;
                }
                labels.push((value, text.insertion_point.x, text.insertion_point.y));
            }
        }
        if labels.is_empty() {
            self.command_line.push_output(
                "ELECWIRELABELS: no wire numbers on this drawing — run ELECWIRENUMBER first.",
            );
            return Task::none();
        }
        labels.sort_by(|left, right| left.0.cmp(&right.0));
        self.command_line.push_output(&format!(
            "{:<12} {:>10} {:>10}",
            "WIRE", "X", "Y"
        ));
        for (value, x, y) in &labels {
            self.command_line.push_output(&format!(
                "{:<12} {:>10.2} {:>10.2}",
                value, x, y
            ));
        }
        let Some(path) = out.map(str::trim).filter(|path| !path.is_empty()) else {
            self.command_line.push_output(&format!(
                "ELECWIRELABELS: {} label(s); give a .csv path to export them.",
                labels.len()
            ));
            return Task::none();
        };
        let mut text = String::from("WireNumber,X,Y\n");
        for (value, x, y) in &labels {
            text.push_str(&format!("{},{:.3},{:.3}\n", csv_field(value), x, y));
        }
        match std::fs::write(path, text) {
            Ok(()) => self.command_line.push_output(&format!(
                "ELECWIRELABELS: wrote {} label(s) to {path}.",
                labels.len()
            )),
            Err(error) => self
                .command_line
                .push_output(&format!("ELECWIRELABELS: cannot write {path}: {error}")),
        }
        Task::none()
    }

    /// `ELECSETUP` reports the drawing's electrical setup and the loaded symbol library.
    ///
    /// This is the read-only face of what an AutoCAD Electrical drawing keeps on `WD_M`:
    /// the tag and wire formats, their start values and suffixes. It answers "what will
    /// `ELECNUMBER` / `ELECWIRENUMBER` do on this sheet?" before anything is written.
    pub(super) fn elec_setup(&mut self, i: usize) -> Task<Message> {
        let setup = read_setup(&self.tabs[i].scene.document);
        let origin = if setup.present {
            "from this drawing's WD_M block"
        } else {
            "defaults — this drawing has no WD_M block"
        };
        let fixed = if setup.references_are_fixed() {
            " (existing references are fixed)"
        } else {
            ""
        };
        let suffixes = if setup.tag_suffixes.is_empty() {
            "none".to_string()
        } else {
            setup.tag_suffixes.join(",")
        };
        let sheet = if setup.sheet.is_empty() {
            "-".to_string()
        } else {
            setup.sheet.clone()
        };
        let library = {
            let loaded = library_index()
                .read()
                .map(|index| index.len())
                .unwrap_or_default();
            let folder = library_dir()
                .read()
                .ok()
                .and_then(|guard| guard.clone())
                .or_else(remembered_library);
            match (folder, loaded) {
                (Some(path), count) => format!("{} ({count} symbol(s))", path.display()),
                (None, _) => "not loaded — ELECLIB <folder>".to_string(),
            }
        };

        self.command_line
            .push_output(&format!("ELECSETUP: {origin}"));
        self.command_line.push_output(&format!(
            "  tag   {}  mode {}{}  start {}  step {}  follow through {}  unique {}  suffixes {suffixes}",
            setup.tag_format,
            setup.tag_mode,
            fixed,
            setup.tag_start,
            setup.tag_increment,
            if setup.tag_fallthrough { "yes" } else { "no" },
            setup.tag_unique
        ));
        self.command_line.push_output(&format!(
            "  wire  {}  mode {}  start {}  step {}  numbers {}  empty first {}",
            setup.wire_format,
            setup.wire_mode,
            setup.wire_start,
            setup.wire_increment,
            if setup.wire_alphabetical {
                "alphabetical"
            } else {
                "numerical"
            },
            if setup.wire_empty_first { "yes" } else { "no" }
        ));
        self.command_line
            .push_output(&format!("  xref  {}  sheet {sheet}", setup.xref_format));
        self.command_line
            .push_output(&format!("  library  {library}"));
        self.command_line
            .push_output("  edit  ELECSETUPEDIT opens the numbering setup dialog");
        Task::none()
    }


    /// `ELECSETUPEDIT` opens the dialog that edits the numbering contract.
    pub(super) fn elec_setup_edit(&mut self, i: usize) -> Task<Message> {
        use crate::ui::window::elec_setup::{mode_label, State};
        let setup = read_setup(&self.tabs[i].scene.document);
        self.elec_setup_dialog = Some(State {
            tag_format: setup.tag_format,
            tag_mode: mode_label(&setup.tag_mode),
            tag_start: setup.tag_start.to_string(),
            tag_suffixes: setup.tag_suffixes.join(","),
            wire_format: setup.wire_format,
            wire_mode: mode_label(&setup.wire_mode),
            wire_start: setup.wire_start.to_string(),
            xref_format: setup.xref_format,
            sheet: setup.sheet,
            tag_increment: setup.tag_increment.to_string(),
            tag_fallthrough: if setup.tag_fallthrough { "Yes" } else { "No" }.to_string(),
            tag_unique: setup.tag_unique,
            wire_increment: setup.wire_increment.to_string(),
            wire_kind: if setup.wire_alphabetical {
                "Alphabetical"
            } else {
                "Numerical"
            }
            .to_string(),
            wire_empty_first: if setup.wire_empty_first { "Yes" } else { "No" }.to_string(),
            error: None,
        });
        self.active_modal = Some(crate::app::ModalKind::ElecSetup);
        self.reset_modal_geometry();
        Task::none()
    }

    /// One message from the electrical setup dialog.
    pub(in crate::app) fn on_elec_setup(
        &mut self,
        msg: crate::ui::window::elec_setup::SetupMsg,
    ) -> Task<Message> {
        use crate::ui::window::elec_setup::SetupMsg;
        let Some(state) = self.elec_setup_dialog.as_mut() else {
            return Task::none();
        };
        state.error = None;
        match msg {
            SetupMsg::TagFormat(value) => state.tag_format = value,
            SetupMsg::TagMode(value) => state.tag_mode = value,
            SetupMsg::TagStart(value) => state.tag_start = value,
            SetupMsg::TagSuffixes(value) => state.tag_suffixes = value,
            SetupMsg::TagIncrement(value) => state.tag_increment = value,
            SetupMsg::TagFallthrough(value) => state.tag_fallthrough = value,
        SetupMsg::TagUnique(value) => state.tag_unique = value,
            SetupMsg::WireFormat(value) => state.wire_format = value,
            SetupMsg::WireMode(value) => state.wire_mode = value,
            SetupMsg::WireStart(value) => state.wire_start = value,
            SetupMsg::WireIncrement(value) => state.wire_increment = value,
            SetupMsg::WireKind(value) => state.wire_kind = value,
            SetupMsg::WireEmptyFirst(value) => state.wire_empty_first = value,
            SetupMsg::XrefFormat(value) => state.xref_format = value,
            SetupMsg::Sheet(value) => state.sheet = value,
            SetupMsg::Ok => return self.apply_elec_setup(),
        }
        Task::none()
    }

    /// Why the dialog refused to apply the values it shows.
    fn refuse_elec_setup(&mut self, message: &str) -> Task<Message> {
        if let Some(state) = self.elec_setup_dialog.as_mut() {
            state.error = Some(message.to_string());
        }
        Task::none()
    }

    /// Save: store the values on the drawing as one undo step.
    fn apply_elec_setup(&mut self) -> Task<Message> {
        use crate::ui::window::elec_setup::mode_letter;
        let Some(state) = self.elec_setup_dialog.clone() else {
            return Task::none();
        };
        let tag_start: u32 = match state.tag_start.trim().parse() {
            Ok(value) => value,
            Err(_) => return self.refuse_elec_setup("Tag start must be a whole number."),
        };
        let wire_start: u32 = match state.wire_start.trim().parse() {
            Ok(value) => value,
            Err(_) => return self.refuse_elec_setup("Wire start must be a whole number."),
        };
        // An increment of zero would stamp the same number on every device.
        let tag_increment: u32 = match state.tag_increment.trim().parse() {
            Ok(value) if value >= 1 => value,
            _ => return self.refuse_elec_setup("The tag increment must be 1 or more."),
        };
        let wire_increment: u32 = match state.wire_increment.trim().parse() {
            Ok(value) if value >= 1 => value,
            _ => return self.refuse_elec_setup("The wire increment must be 1 or more."),
        };
        if state.tag_format.trim().is_empty() || state.wire_format.trim().is_empty() {
            return self.refuse_elec_setup("The tag and wire formats cannot be empty.");
        }
        let tag_mode = mode_letter(&state.tag_mode);
        let wire_mode = mode_letter(&state.wire_mode);
        let i = self.active_tab;
        self.push_undo_snapshot(i, "ELECSETUPEDIT");
        let stored = ElecSetup {
            present: true,
            tag_format: state.tag_format.trim().to_string(),
            tag_mode: tag_mode.clone(),
            tag_start,
            tag_suffixes: state
                .tag_suffixes
                .split(',')
                .map(|part| part.trim().to_string())
                .filter(|part| !part.is_empty())
                .collect(),
            wire_format: state.wire_format.trim().to_string(),
            wire_mode: wire_mode.clone(),
            wire_start,
            xref_format: state.xref_format.trim().to_string(),
            sheet: state.sheet.trim().to_string(),
            tag_increment,
            tag_fallthrough: !state.tag_fallthrough.trim().eq_ignore_ascii_case("No"),
            wire_increment,
            wire_alphabetical: state
                .wire_kind
                .trim()
                .eq_ignore_ascii_case("Alphabetical"),
            wire_empty_first: state.wire_empty_first.trim().eq_ignore_ascii_case("Yes"),
            tag_unique: canonical_uniqueness(&state.tag_unique),
        };
        write_setup_properties(&mut self.tabs[i].scene.document, &stored);
        self.tabs[i].dirty = true;
        // Fields showing a custom property read it again.
        let hosts = crate::entities::field::acvar_hosts(&self.tabs[i].scene.document);
        if !hosts.is_empty() {
            self.tabs[i].scene.update_fields(32, Some(&hosts));
        }
        self.tabs[i].scene.bump_geometry();
        self.elec_setup_dialog = None;
        self.active_modal = None;
        self.command_line.push_output(&format!(
            "ELECSETUPEDIT: tag {} mode {tag_mode} start {tag_start} step {tag_increment}; wire {} mode {wire_mode} start {wire_start} step {wire_increment} ({}).",
            state.tag_format.trim(),
            state.wire_format.trim(),
            state.wire_kind.trim(),
        ));
        Task::none()
    }

    /// `ELECSETUP KEY=VALUE ...` stores the numbering contract without a dialog.
    ///
    /// The dialog is the human path; scripts and the smoke suite need the same
    /// contract headlessly, and both write through `write_setup_properties`.
    pub(super) fn elec_setup_assign(&mut self, i: usize, argument: &str) -> Task<Message> {
        use crate::ui::window::elec_setup::{mode_label, mode_letter};

        const VALID: &str = "TAGFMT, TAGMODE, TAGSTART, TAGSUF, TAGINC, TAGFALL, TAGUNIQ, \
WIREFMT, WIREMODE, WIRESTART, WIREINC, WIREKIND, WIREEMPTY, XREFFMT, SHEET";

        let mut setup = read_setup(&self.tabs[i].scene.document);
        let mut error: Option<String> = None;
        let mut unknown: Vec<String> = Vec::new();
        for word in argument.split_whitespace() {
            let Some((key, value)) = word.split_once('=') else {
                unknown.push(word.to_string());
                continue;
            };
            let key = key
                .trim()
                .to_ascii_uppercase()
                .replace('-', "")
                .replace('_', "");
            let value = value.trim();
            let mut counter = |setup_value: &mut u32| match value.parse::<u32>() {
                Ok(parsed) if parsed >= 1 => *setup_value = parsed,
                _ => error = Some(format!("{key} must be a whole number of 1 or more.")),
            };
            match key.as_str() {
                "TAGFMT" => setup.tag_format = value.to_string(),
                "TAGMODE" => setup.tag_mode = mode_letter(value),
                "TAGSTART" => counter(&mut setup.tag_start),
                "TAGSUF" | "TAGRSUF" => {
                    setup.tag_suffixes = value
                        .split(',')
                        .map(|part| part.trim().to_string())
                        .filter(|part| !part.is_empty())
                        .collect()
                }
                "TAGINC" => counter(&mut setup.tag_increment),
                "TAGFALL" => setup.tag_fallthrough = !value.eq_ignore_ascii_case("No"),
                "TAGUNIQ" => {
                    if !matches!(
                        value.to_ascii_lowercase().as_str(),
                        "global" | "local" | "none"
                    ) {
                        error = Some("TAGUNIQ must be Global, Local or None.".to_string());
                    }
                    setup.tag_unique = canonical_uniqueness(value);
                }
                "WIREFMT" => setup.wire_format = value.to_string(),
                "WIREMODE" => setup.wire_mode = mode_letter(value),
                "WIRESTART" => counter(&mut setup.wire_start),
                "WIREINC" => counter(&mut setup.wire_increment),
                "WIREKIND" => {
                    setup.wire_alphabetical = value.eq_ignore_ascii_case("Alphabetical")
                }
                "WIREEMPTY" => setup.wire_empty_first = value.eq_ignore_ascii_case("Yes"),
                "XREFFMT" => setup.xref_format = value.to_string(),
                "SHEET" => setup.sheet = value.to_string(),
                _ => unknown.push(key.clone()),
            }
        }
        if let Some(message) = error {
            self.command_line.push_output(&format!("ELECSETUP: {message}"));
            return Task::none();
        }
        if !unknown.is_empty() {
            self.command_line.push_output(&format!(
                "ELECSETUP: unknown setting(s) {} — valid: {VALID}.",
                unknown.join(", ")
            ));
            return Task::none();
        }
        if setup.tag_format.trim().is_empty() || setup.wire_format.trim().is_empty() {
            self.command_line
                .push_output("ELECSETUP: the tag and wire formats cannot be empty.");
            return Task::none();
        }
        self.push_undo_snapshot(i, "ELECSETUP");
        write_setup_properties(&mut self.tabs[i].scene.document, &setup);
        self.tabs[i].dirty = true;
        let hosts = crate::entities::field::acvar_hosts(&self.tabs[i].scene.document);
        if !hosts.is_empty() {
            self.tabs[i].scene.update_fields(32, Some(&hosts));
        }
        self.tabs[i].scene.bump_geometry();
        self.command_line.push_output(&format!(
            "ELECSETUP: tag {} mode {} start {} step {} (unique {}); wire {} mode {} start {} step {} ({}); saved on this drawing.",
            setup.tag_format,
            mode_label(&setup.tag_mode),
            setup.tag_start,
            setup.tag_increment,
            setup.tag_unique,
            setup.wire_format,
            mode_label(&setup.wire_mode),
            setup.wire_start,
            setup.wire_increment,
            if setup.wire_alphabetical {
                "alphabetical"
            } else {
                "numerical"
            },
        ));
        Task::none()
    }

    /// `ELECBOM` prints the bill of material of the active drawing.
    ///
    /// `ELECBOM <file.csv>` also writes the rows to a CSV file, using the column names
    /// the shipped report templates use, so a report can be produced without a GUI.
    ///
    /// `OPTION=<code>` filters the bill to the devices a project marks with that option code
    /// (`ELECOPTION <tag> <code>`), which is how a variant-specific parts list is produced.
    pub(super) fn elec_bom(
        &mut self,
        i: usize,
        path: Option<&str>,
        option: Option<&str>,
    ) -> Task<Message> {
        let mut components = self.elec_components(i);
        if components.is_empty() {
            self.show_elec_report(
                "Bill of Material",
                &["ELECBOM: no electrical components found (no block insert with attributes)."
                    .to_string()],
            );
            return Task::none();
        }
        let mut lines = Vec::new();
        if let Some(option) = option.map(str::trim).filter(|code| !code.is_empty()) {
            let total = components.len();
            let tags = project_option_tags(option);
            components.retain(|component| {
                tags.contains(&component.tag.trim().to_ascii_uppercase())
            });
            if components.is_empty() {
                lines.push(format!(
                    "ELECBOM: no components assigned to option {option} (of {total} on this drawing)."
                ));
                self.show_elec_report("Bill of Material", &lines);
                return Task::none();
            }
            lines.push(format!(
                "ELECBOM: option {option} selects {} of {total} component(s).",
                components.len()
            ));
        }
        // A loaded catalogue supplies the manufacturer and description the drawing does not
        // carry. Values on the drawing win, so this only fills gaps.
        let mut enriched = 0usize;
        if let Ok(guard) = catalogue().read() {
            if let Some(catalogue) = guard.as_ref() {
                for component in &mut components {
                    if catalogue.enrich(component) {
                        enriched += 1;
                    }
                }
            }
        }
        let rows = bill_of_materials(&components);
        lines.push(format!(
            "ELECBOM: {} component(s) -> {} bill-of-material row(s).",
            components.len(),
            rows.len()
        ));
        if enriched > 0 {
            lines.push(format!(
                "  {enriched} component(s) completed from the parts catalogue"
            ));
        }
        // Everything so far is the headline of the report; the table follows.
        let mut summary = vec![format!(
            "{} component(s) grouped into {} part line(s)",
            components.len(),
            rows.len()
        )];
        if enriched > 0 {
            summary.push(format!(
                "{enriched} completed from the parts catalogue"
            ));
        }
        if let Some(option) = option.map(str::trim).filter(|code| !code.is_empty()) {
            summary.push(format!("Option {option} selects {} component(s)", components.len()));
        }
        lines.push(format!(
            "{:<22} {:<26} {:<16} {:>4}  {:<28} {}",
            "CLASS", "PART", "MANUFACTURER", "QTY", "WHERE USED", "DESCRIPTION"
        ));
        for row in &rows {
            let where_used = row.where_used.join(", ");
            lines.push(format!(
                "{:<22} {:<26} {:<16} {:>4}  {:<28} {}",
                row.class,
                row.part.clone().unwrap_or_else(|| "-".to_string()),
                row.manufacturer.clone().unwrap_or_else(|| "-".to_string()),
                row.quantity,
                truncate(&where_used, 28),
                row.description.clone().unwrap_or_default(),
            ));
        }
        if let Some(path) = path {
            match write_bom_csv(path, &rows) {
                Ok(()) => lines.push(format!(
                    "ELECBOM: wrote {} row(s) to {path}",
                    rows.len()
                )),
                Err(error) => lines.push(format!("ELECBOM: cannot write {path}: {error}")),
            }
        }
        let table_rows: Vec<Vec<String>> = rows
            .iter()
            .map(|row| {
                vec![
                    row.class.clone(),
                    row.part.clone().unwrap_or_else(|| "-".to_string()),
                    row.manufacturer.clone().unwrap_or_else(|| "-".to_string()),
                    row.quantity.to_string(),
                    row.where_used.join(", "),
                    row.description.clone().unwrap_or_default(),
                ]
            })
            .collect();
        let title = format!("Bill of Material ({} row(s))", rows.len());
        self.set_elec_report_table(
            title,
            &lines,
            summary,
            vec![
                "Class".to_string(),
                "Part".to_string(),
                "Manufacturer".to_string(),
                "Qty".to_string(),
                "Where used".to_string(),
                "Description".to_string(),
            ],
            table_rows,
            Some(0),
        );
        Task::none()
    }

    /// `ELECLIST` lists the components the extractor found, one per line.
    pub(super) fn elec_list(&mut self, i: usize) -> Task<Message> {
        let components = self.elec_components(i);
        if components.is_empty() {
            self.show_elec_report(
                "Component List",
                &["ELECLIST: no electrical components found.".to_string()],
            );
            return Task::none();
        }
        let mut lines = Vec::with_capacity(components.len() + 1);
        lines.push(format!(
            "{:<22} {:<12} {:<26} {:<20} {}",
            "CLASS", "TAG", "PART", "LOCATION", "DESCRIPTION"
        ));
        for component in &components {
            lines.push(format!(
                "{:<22} {:<12} {:<26} {:<20} {}",
                component.class,
                component.tag,
                component.part.clone().unwrap_or_else(|| "-".to_string()),
                component.location.clone().unwrap_or_else(|| "-".to_string()),
                component.description.clone().unwrap_or_default(),
            ));
        }
        let rows: Vec<Vec<String>> = components
            .iter()
            .map(|component| {
                vec![
                    component.class.clone(),
                    component.tag.clone(),
                    component.part.clone().unwrap_or_else(|| "-".to_string()),
                    component.location.clone().unwrap_or_else(|| "-".to_string()),
                    component.description.clone().unwrap_or_default(),
                ]
            })
            .collect();
        let title = format!("Component List ({})", components.len());
        self.set_elec_report_table(
            title,
            &lines,
            vec![format!(
                "{} component(s) on this sheet",
                components.len()
            )],
            vec![
                "Class".to_string(),
                "Tag".to_string(),
                "Part".to_string(),
                "Location".to_string(),
                "Description".to_string(),
            ],
            rows,
            Some(0),
        );
        Task::none()
    }

    /// `ELECNUMBER` assigns device tags to the components of the active drawing.
    ///
    /// Components are grouped by class, ordered top-to-bottom then left-to-right, and
    /// numbered with the prefix already used on the sheet (or the class's IEC letter when
    /// there is none). Structured tags such as `X1:1` are left alone. `ELECNUMBER ?` only
    /// reports what would change.
    pub(super) fn elec_number(&mut self, i: usize, dry_run: bool) -> Task<Message> {
        use ::elec::{default_prefix, infer_tag_prefix, reading_order_key, renumber, Numbered, NumberingFormat};
        use std::collections::BTreeMap;

        struct Item {
            handle: codec::Handle,
            class: String,
            tag: String,
            x: f64,
            y: f64,
        }

        let mut items: Vec<Item> = Vec::new();
        {
            let doc = &self.tabs[i].scene.document;
            let _model_space = doc.header.model_space_block_handle;
            let spaces = Self::drawing_spaces(doc);
            for entity in doc.entities() {
                if !spaces.contains(&entity.common().owner_handle) { continue; }
                let codec::EntityType::Insert(insert) = entity else {
                    continue;
                };
                if insert.attributes.is_empty() || is_setup_block(&insert.block_name) {
                    continue;
                }
                let tag = insert
                    .attributes
                    .iter()
                    .find(|attribute| is_tag_attribute(&attribute.tag))
                    .map(|attribute| attribute.value.trim().to_string())
                    .unwrap_or_default();
                items.push(Item {
                    handle: insert.common.handle,
                    class: insert.block_name.clone(),
                    tag,
                    x: insert.insert_point.x,
                    y: insert.insert_point.y,
                });
            }
        }
        if items.is_empty() {
            self.command_line
                .push_output("ELECNUMBER: no electrical components found.");
            return Task::none();
        }

        // The drawing's own numbering contract (or the defaults when it has none).
        let setup = read_setup(&self.tabs[i].scene.document);

        let mut by_class: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        // Group by tag *prefix*, not by block or class: a relay (HCR1) and a contactor
        // (CONT-NO) are both -K devices and must share one sequence, or each family
        // restarts at -K1 and two different devices end up with the same tag.
        let class_of = |block: &str| -> String {
            library_index()
                .read()
                .ok()
                .and_then(|index| {
                    index
                        .get(&block.to_ascii_uppercase())
                        .map(|entry| entry.class.clone())
                })
                .unwrap_or_else(|| block.to_string())
        };
        for (index, item) in items.iter().enumerate() {
            let block_class = class_of(&item.class);
            let prefix = if item.tag.is_empty() {
                default_prefix(&block_class)
            } else {
                infer_tag_prefix(&[item.tag.as_str()])
                    .unwrap_or_else(|| default_prefix(&block_class))
            };
            by_class.entry(prefix).or_default().push(index);
        }

        // SkyCAD's uniqueness scope decides where a number may be reused. `Global`
        // reserves every tag already used by the other sheets of the project, so a
        // second sheet does not hand out -K1 again; `Local` and `None` only look at
        // this drawing (which the numbering pass already does).
        let reserved: BTreeSet<String> = if setup.uniqueness_is_global() {
            let open = self.tabs[i]
                .current_path
                .as_ref()
                .map(|path| path.to_string_lossy().to_ascii_lowercase());
            let mut tags = BTreeSet::new();
            for path in Self::project_sheets() {
                if open
                    .as_deref()
                    .is_some_and(|open| path.to_string_lossy().eq_ignore_ascii_case(open))
                {
                    continue;
                }
                let Ok(document) = crate::io::load_file(&path) else {
                    continue;
                };
                for component in Self::components_of(&document) {
                    let tag = component.tag.trim();
                    if !tag.is_empty() {
                        tags.insert(tag.to_ascii_uppercase());
                    }
                }
            }
            tags
        } else {
            BTreeSet::new()
        };

        let mut changes: Vec<(codec::Handle, String)> = Vec::new();
        let mut report: Vec<String> = Vec::new();
        let mut skipped: Vec<String> = Vec::new();

        for (class, mut indices) in by_class {
            let existing: Vec<&str> = indices
                .iter()
                .map(|index| items[*index].tag.as_str())
                .filter(|tag| !tag.is_empty())
                .collect();
            let prefix = match infer_tag_prefix(&existing) {
                Some(prefix) => prefix,
                None if existing.is_empty() => default_prefix(&class),
                None => {
                    // Structured tags (strip:terminal, location/device) keep their own rules.
                    skipped.push(class.clone());
                    continue;
                }
            };
            // Parts of one device share a tag - a coil and its contacts are one device. Group
            // by tag first so renumbering assigns one number per *device*; numbering each
            // part separately split a device into three and renamed its parts -K2, -K3, -K1.
            let mut devices: BTreeMap<String, Vec<usize>> = BTreeMap::new();
            // SkyCAD numbers by *symbol* when "Follow through" is off, so a coil and
            // each of its contacts take their own number instead of sharing the coil's.
            let fallthrough = setup.tag_fallthrough;
            for index in indices.drain(..) {
                let key = {
                    let tag = items[index].tag.trim();
                    if tag.is_empty() || !fallthrough {
                        format!("#{index}")
                    } else {
                        tag.to_ascii_uppercase()
                    }
                };
                devices.entry(key).or_default().push(index);
            }
            let mut device_list: Vec<Vec<usize>> = devices.into_values().collect();
            device_list.sort_by_key(|parts| {
                let first = parts[0];
                reading_order_key(items[first].x, items[first].y)
            });
            // Take the drawing's own `TAGFMT`/`TAG-START` when it has one: the crate's
            // `NumberingFormat` already speaks the ACADE `%F`/`%N` language, so the settings
            // only have to be copied across rather than re-implemented.
            let mut format = NumberingFormat::tag(&prefix);
            // `read_setup` already merges the WD_M block with the properties the setup
            // dialog writes, so copy the contract across whether or not a WD_M exists.
            if !setup.tag_format.trim().is_empty() {
                format.pattern = setup.tag_format.clone();
            }
            format.start = setup.tag_start.max(1);
            format.increment = setup.tag_increment.max(1);
            // A drawing that came from AutoCAD Electrical says whether its references are
            // fixed. When they are, an existing tag must keep its number: renumbering the
            // customer's real `M326` to `M1` would silently rewrite a released drawing.
            // A project flagged as built is already assembled and wired on site:
            // only new devices may be numbered, existing references are fixed.
            let freeze_existing =
                project_is_built() || (setup.present && setup.references_are_fixed());
            let numbered: Vec<Numbered> = device_list
                .iter()
                .enumerate()
                .map(|(device, parts)| {
                    let item = &items[parts[0]];
                    Numbered {
                        id: device as u64,
                        current: if item.tag.is_empty() {
                            None
                        } else {
                            Some(item.tag.clone())
                        },
                        frozen: freeze_existing && !item.tag.is_empty(),
                    }
                })
                .collect();
            let results = renumber(&format, &numbered);
            // Numbers already spoken for: the other sheets (global scope) and the
            // tags a frozen device keeps. `renumber` numbers by ordinal, so a
            // collision is resolved by stepping the ordinal until the label is free.
            let mut taken: BTreeSet<String> = reserved.clone();
            for item in &numbered {
                if item.frozen {
                    if let Some(tag) = &item.current {
                        taken.insert(tag.trim().to_ascii_uppercase());
                    }
                }
            }
            let mut assigned = Vec::new();
            let mut changed = 0usize;
            for result in results {
                let mut label = result.value.clone();
                let frozen = numbered[result.id as usize].frozen;
                if !frozen {
                    let mut ordinal = result.id as u32;
                    while taken.contains(&label.trim().to_ascii_uppercase()) {
                        ordinal += 1;
                        label = format.value(ordinal);
                    }
                }
                taken.insert(label.trim().to_ascii_uppercase());
                // One result per device; every part of that device takes the same tag.
                let parts = &device_list[result.id as usize];
                for part in parts {
                    let item = &items[*part];
                    if item.tag != label {
                        changed += 1;
                        changes.push((item.handle, label.clone()));
                    }
                }
                assigned.push(label);
            }
            report.push(format!(
                "  {class}: {} ({changed} changed)",
                assigned.join(", ")
            ));
        }

        let verb = if dry_run { "would write" } else { "wrote" };
        let summary = format!(
            "ELECNUMBER: {} component(s) in {} class(es); {verb} {} tag(s).",
            items.len(),
            report.len(),
            changes.len()
        );
        self.command_line.push_output(&summary);
        for line in &report {
            self.command_line.push_output(line);
        }
        for class in &skipped {
            let line = format!("  {class}: skipped (structured tags)");
            self.command_line.push_output(&line);
        }

        if dry_run || changes.is_empty() {
            return Task::none();
        }
        self.push_undo_snapshot(i, "ELECNUMBER");
        {
            let doc = &mut self.tabs[i].scene.document;
            for (handle, value) in &changes {
                let Some(codec::EntityType::Insert(insert)) = doc.get_entity_mut(*handle) else {
                    continue;
                };
                for attribute in &mut insert.attributes {
                    if is_tag_attribute(&attribute.tag) {
                        attribute.value = value.clone();
                    }
                }
            }
        }
        self.tabs[i].dirty = true;
        Task::none()
    }

    /// `ELECWIRELIST [out.csv]` derives connectivity from the drawn wires and reports the nets.
    ///
    /// Wires are the drawing's `WIRES` layer (or, on a flattened print, layer-0 lines long
    /// enough to be a conductor); a device anchors to a net at its **pin**, not at the
    /// symbol's insertion point. Points within `tolerance` are the same node, a terminal
    /// pass-through bridges the two ends on either side of it, and a T-junction only
    /// connects where a junction dot marks it.
    ///
    /// With a path, the conductor list is written as CSV with the columns the shop needs:
    /// `Wire, Net, From pin, To pin, From X, From Y, To X, To Y`.
    pub(super) fn elec_wirelist(&mut self, i: usize, out: Option<&str>) -> Task<Message> {
        use ::elec::infer_tag_prefix;
        use ::elec::wire_rows_with_pins;

        const TOLERANCE: f64 = 0.05;

        let graph = {
            let doc = &self.tabs[i].scene.document;
            wire_graph(doc)
        };
        if graph.wires.is_empty() {
            self.show_elec_report(
                "Wire List",
                &["ELECWIRELIST: no wires found (LINE or LWPOLYLINE entities).".to_string()],
            );
            return Task::none();
        }

        let netlist = graph.netlist(TOLERANCE);
        let mut lines = Vec::new();
        lines.push(format!(
            "ELECWIRELIST: {} wire segment(s), {} pin(s) -> {} net(s); {} unconnected pin(s), {} loose wire(s).",
            graph.wires.len(),
            graph.anchors.len(),
            netlist.nets.len(),
            netlist.loose_anchors.len(),
            netlist.loose_wires.len()
        ));

        for net in &netlist.nets {
            let members: Vec<String> = net
                .anchors
                .iter()
                .map(|id| graph.label_of(*id))
                .collect();
            lines.push(format!(
                "  net {}: {} wire(s) -> {}",
                net.id,
                net.wires.len(),
                if members.is_empty() {
                    "(no devices)".to_string()
                } else {
                    members.join(", ")
                }
            ));
        }

        if !netlist.loose_anchors.is_empty() {
            let names: Vec<String> = netlist
                .loose_anchors
                .iter()
                .map(|id| graph.label_of(*id))
                .collect();
            lines.push(format!("  unconnected: {}", names.join(", ")));
        }

        if let Some(path) = out.map(str::trim).filter(|path| !path.is_empty()) {
            let rows = wire_rows_with_pins(&graph.wires, &netlist, &graph.anchors);
            match std::fs::File::create(path) {
                Ok(file) => {
                    use std::io::Write as _;
                    let mut writer = std::io::BufWriter::new(file);
                    let mut text = String::from(
                        "Wire,Net,From pin,To pin,From X,From Y,To X,To Y\n",
                    );
                    for row in &rows {
                        text.push_str(&format!(
                            "{},{},{},{},{:.4},{:.4},{:.4},{:.4}\n",
                            row.wire,
                            row.net,
                            csv_field(row.from_pin.as_deref().unwrap_or("-")),
                            csv_field(row.to_pin.as_deref().unwrap_or("-")),
                            row.from.x,
                            row.from.y,
                            row.to.x,
                            row.to.y
                        ));
                    }
                    match writer.write_all(text.as_bytes()) {
                        Ok(()) => lines.push(format!(
                            "ELECWIRELIST: wrote {} conductor row(s) to {path}.",
                            rows.len()
                        )),
                        Err(error) => {
                            lines.push(format!("ELECWIRELIST: cannot write {path}: {error}"))
                        }
                    }
                }
                Err(error) => lines.push(format!("ELECWIRELIST: cannot write {path}: {error}")),
            }
        }

        // A drawing that names its devices is a drawing we can sanity-check: report the
        // prefixes in use so a numbering mistake is visible from the same command.
        let tags: Vec<&str> = graph
            .anchors
            .iter()
            .map(|anchor| anchor.label.as_str())
            .collect();
        if let Some(prefix) = infer_tag_prefix(&tags) {
            lines.push(format!("  device prefix: {prefix}"));
        }

        let title = format!("Wire List ({} net(s))", netlist.nets.len());
        let mut summary = vec![format!(
            "{} wire segment(s) · {} pin(s) · {} net(s) · {} unconnected pin(s)",
            graph.wires.len(),
            graph.anchors.len(),
            netlist.nets.len(),
            netlist.loose_anchors.len()
        )];
        if let Some(prefix) = infer_tag_prefix(
            &graph
                .anchors
                .iter()
                .map(|anchor| anchor.label.as_str())
                .collect::<Vec<_>>(),
        ) {
            summary.push(format!("Device prefix {prefix}"));
        }
        let table_rows: Vec<Vec<String>> = netlist
            .nets
            .iter()
            .map(|net| {
                let members: Vec<String> = net
                    .anchors
                    .iter()
                    .map(|id| graph.label_of(*id))
                    .collect();
                vec![
                    format!("N{}", net.id),
                    net.wires.len().to_string(),
                    if members.is_empty() {
                        "-".to_string()
                    } else {
                        members.join(", ")
                    },
                ]
            })
            .collect();
        self.set_elec_report_table(
            title,
            &lines,
            summary,
            vec![
                "Net".to_string(),
                "Segments".to_string(),
                "Connected pins".to_string(),
            ],
            table_rows,
            None,
        );
        Task::none()
    }

    /// `ELECWIRENUMBER` numbers the nets and labels each one on the drawing.
    ///
    /// Nets are numbered in reading order (top-to-bottom, then left-to-right) using the
    /// wire numbering format, and a text label is placed at the midpoint of the net's first
    /// wire on layer `WIRENO`. An existing label at the same point is updated rather than
    /// duplicated, so the command is safe to run again after a design change.
    pub(super) fn elec_wirenumber(&mut self, i: usize) -> Task<Message> {
        use ::elec::{reading_order_key, NumberingFormat};

        const TOLERANCE: f64 = 0.05;
        const LABEL_LAYER: &str = "WIRENO";
        const LABEL_HEIGHT: f64 = 2.5;

        let graph = {
            let doc = &self.tabs[i].scene.document;
            wire_graph(doc)
        };
        let wires = &graph.wires;
        let existing: Vec<(codec::Handle, String, f64, f64)> = {
            let doc = &self.tabs[i].scene.document;
            let spaces = Self::drawing_spaces(doc);
            doc.entities()
                .filter(|entity| spaces.contains(&entity.common().owner_handle))
                .filter_map(|entity| match entity {
                    codec::EntityType::Text(text)
                        if text.common.layer.eq_ignore_ascii_case(LABEL_LAYER) =>
                    {
                        Some((
                            text.common.handle,
                            text.value.clone(),
                            text.insertion_point.x,
                            text.insertion_point.y,
                        ))
                    }
                    _ => None,
                })
                .collect()
        };

        let netlist = graph.netlist(TOLERANCE);
        if netlist.nets.is_empty() {
            self.command_line
                .push_output("ELECWIRENUMBER: no connected nets found.");
            return Task::none();
        }

        // Order nets by where their first wire sits, so numbers follow the drawing.
        let mut ordered: Vec<(i64, i64, u64)> = Vec::new();
        for net in &netlist.nets {
            // A net of coincident symbols has no wires and nothing to label.
            let Some(first_wire) = net.wires.first().copied() else {
                continue;
            };
            let Some(first) = wires.iter().find(|wire| wire.id == first_wire) else {
                continue;
            };
            let mid_x = (first.from.x + first.to.x) / 2.0;
            let mid_y = (first.from.y + first.to.y) / 2.0;
            let (row, column) = reading_order_key(mid_x, mid_y);
            ordered.push((row, column, net.id));
        }
        ordered.sort_unstable();

        // The drawing's own wire numbering contract when it has one, so a sheet drawn in
        // AutoCAD Electrical numbers wires the way its `WIREFMT` says.
        let setup = read_setup(&self.tabs[i].scene.document);
        let mut format = NumberingFormat::wire();
        if !setup.wire_format.trim().is_empty() {
            format.pattern = setup.wire_format.clone();
        }
        format.start = setup.wire_start.max(1);
        format.increment = setup.wire_increment.max(1);
        if setup.wire_alphabetical {
            format.kind = ::elec::Kind::Alphabetical;
        }
        // SkyCAD's "Empty first value" leaves the first conductor unnumbered.
        let skip_first = setup.wire_empty_first;
        let mut updates: Vec<(codec::Handle, String)> = Vec::new();
        let mut creates: Vec<(String, f64, f64)> = Vec::new();
        let mut report: Vec<String> = Vec::new();

        for (ordinal, (_row, _column, net_id)) in ordered.iter().enumerate() {
            if skip_first && ordinal == 0 {
                report.push("  (first net left blank)".to_string());
                continue;
            }
            let net = netlist.nets.iter().find(|net| net.id == *net_id).unwrap();
            let number = format.value(ordinal as u32);
            let Some(first_wire) = net.wires.first().copied() else {
                continue;
            };
            let Some(first) = wires.iter().find(|wire| wire.id == first_wire) else {
                continue;
            };
            let x = (first.from.x + first.to.x) / 2.0;
            let y = (first.from.y + first.to.y) / 2.0 + 1.0;
            let reuse = existing.iter().find(|(_, _, ex, ey)| {
                (ex - x).abs() <= 0.1 && (ey - y).abs() <= 0.1
            });
            let names: Vec<String> = net
                .anchors
                .iter()
                .map(|id| graph.label_of(*id))
                .collect();
            match reuse {
                Some((handle, current, _, _)) if current == &number => {
                    report.push(format!("  {number}: net {net_id} (unchanged)"));
                }
                Some((handle, _, _, _)) => {
                    updates.push((*handle, number.clone()));
                    report.push(format!(
                        "  {number}: net {net_id} ({} -> {number})",
                        names.join(", ")
                    ));
                }
                None => {
                    creates.push((number.clone(), x, y));
                    report.push(format!("  {number}: net {net_id} ({})", names.join(", ")));
                }
            }
        }

        let summary = format!(
            "ELECWIRENUMBER: {} net(s); {} label(s) created, {} updated.",
            ordered.len(),
            creates.len(),
            updates.len()
        );
        self.command_line.push_output(&summary);
        for line in &report {
            self.command_line.push_output(line);
        }

        if creates.is_empty() && updates.is_empty() {
            return Task::none();
        }
        self.push_undo_snapshot(i, "ELECWIRENUMBER");
        self.tabs[i].scene.ensure_layer(LABEL_LAYER);
        for (handle, value) in updates {
            let doc = &mut self.tabs[i].scene.document;
            if let Some(codec::EntityType::Text(text)) = doc.get_entity_mut(handle) {
                text.value = value;
            }
        }
        for (value, x, y) in creates {
            let mut text = codec::entities::Text::new();
            text.value = value;
            text.insertion_point = codec::types::Vector3::new(x, y, 0.0);
            text.height = LABEL_HEIGHT;
            text.common.layer = LABEL_LAYER.to_string();
            self.tabs[i].scene.add_entity(codec::EntityType::Text(text));
        }
        self.tabs[i].dirty = true;
        Task::none()
    }

    /// Collect the drawing's wire segments and device pins, as the connectivity commands see
    /// them. One collector, so the terminals list, the Excel report and the I/O pass cannot
    /// disagree about which segments and pins exist.
    fn elec_collect(
        &self,
        i: usize,
    ) -> (
        Vec<::elec::geometry::WireSeg>,
        Vec<::elec::geometry::Anchor>,
    ) {
        let graph = wire_graph(&self.tabs[i].scene.document);
        (graph.wires, graph.anchors)
    }

    /// `ELECHIGHLIGHT <tag>` selects every part of one device.
    ///
    /// A multi-part device (a coil, its contacts, its overload) is scattered over the sheet;
    /// picking it out by tag is the one navigation move an electrician asks for first. Both
    /// the block inserts that carry the tag and the plain TEXT a flattened sheet keeps are
    /// selected, so the same command works on the customer's prints.
    pub(super) fn elec_highlight(&mut self, i: usize, tag: &str) -> Task<Message> {
        let handles = {
            let doc = &self.tabs[i].scene.document;
            device_entities(doc, tag)
        };
        let wanted = tag.trim();
        if handles.is_empty() {
            let message = format!("ELECHIGHLIGHT: no device tagged {wanted}.");
            self.command_line.push_output(&message);
            return Task::none();
        }
        self.tabs[i].scene.deselect_all();
        self.tabs[i].scene.select_entities(&handles);
        let message = format!(
            "ELECHIGHLIGHT: selected {} part(s) of {wanted}.",
            handles.len()
        );
        self.command_line.push_output(&message);
        Task::none()
    }

    /// `ELECGOTO <tag>` centres the view on a device and its parts.
    pub(super) fn elec_goto(&mut self, i: usize, tag: &str) -> Task<Message> {
        let handles = {
            let doc = &self.tabs[i].scene.document;
            device_entities(doc, tag)
        };
        let wanted = tag.trim();
        if handles.is_empty() {
            let message = format!("ELECGOTO: no device tagged {wanted}.");
            self.command_line.push_output(&message);
            return Task::none();
        }
        self.tabs[i].scene.remember_current_view();
        if self.tabs[i].scene.zoom_to_entities(&handles) {
            let message = format!("ELECGOTO: centred on {wanted} ({} part(s)).", handles.len());
            self.command_line.push_output(&message);
        } else {
            let message = format!("ELECGOTO: {wanted} has no visible bounds.");
            self.command_line.push_output(&message);
        }
        Task::none()
    }

    /// `ELECTERMINALS` reports the terminal strips in the drawing.
    ///
    /// Terminals are components tagged `strip:index` (`X1:12`). Each strip is listed in
    /// terminal order with the devices its terminals reach through their nets.
    pub(super) fn elec_terminals(&mut self, i: usize) -> Task<Message> {
        use ::elec::terminal_strips;

        const TOLERANCE: f64 = 0.05;

        let graph = {
            let doc = &self.tabs[i].scene.document;
            wire_graph(doc)
        };
        let netlist = graph.netlist(TOLERANCE);
        let anchors = &graph.anchors;
        let label_of = |id: u64| graph.label_of(id);

        let mut terminals: Vec<(String, Vec<String>)> = Vec::new();
        let mut others: Vec<String> = Vec::new();
        for anchor in anchors {
            let peers: Vec<String> = netlist
                .nets
                .iter()
                .find(|net| net.anchors.contains(&anchor.id))
                .map(|net| {
                    net.anchors
                        .iter()
                        .filter(|id| **id != anchor.id)
                        .map(|id| label_of(*id))
                        .collect()
                })
                .unwrap_or_default();
            if ::elec::parse_terminal_tag(&anchor.label).is_some() {
                terminals.push((anchor.label.clone(), peers));
            } else {
                others.push(anchor.label.clone());
            }
        }

        if terminals.is_empty() {
            self.show_elec_report(
                "Terminal Strips",
                &["ELECTERMINALS: no terminals found (no component tagged strip:index)."
                    .to_string()],
            );
            return Task::none();
        }

        let strips = terminal_strips(&terminals);
        let mut lines = Vec::new();
        lines.push(format!(
            "ELECTERMINALS: {} strip(s), {} terminal(s).",
            strips.len(),
            terminals.len()
        ));
        for strip in &strips {
            lines.push(format!(
                "  {} ({} terminal(s))",
                strip.name,
                strip.terminals.len()
            ));
            for terminal in &strip.terminals {
                let detail = if terminal.peers.is_empty() {
                    "unconnected".to_string()
                } else {
                    terminal.peers.join(", ")
                };
                lines.push(format!("    {}: {}", terminal.tag, detail));
            }
        }
        if !others.is_empty() {
            lines.push(format!("  not terminals: {}", others.join(", ")));
        }
        let title = format!("Terminal Strips ({} strip(s))", strips.len());
        let terminal_count: usize = strips.iter().map(|strip| strip.terminals.len()).sum();
        let summary = vec![
            format!(
                "{} strip(s), {} terminal(s)",
                strips.len(),
                terminal_count
            ),
            format!("{} other pin(s) on the sheet", others.len()),
        ];
        let mut table_rows: Vec<Vec<String>> = Vec::new();
        for strip in &strips {
            for terminal in &strip.terminals {
                table_rows.push(vec![
                    strip.name.clone(),
                    terminal.tag.clone(),
                    if terminal.peers.is_empty() {
                        "—".to_string()
                    } else {
                        terminal.peers.join(", ")
                    },
                ]);
            }
        }
        self.set_elec_report_table(
            title,
            &lines,
            summary,
            vec![
                "Strip".to_string(),
                "Terminal".to_string(),
                "Connects to".to_string(),
            ],
            table_rows,
            Some(0),
        );
        self.elec_report.filter_label = "All strips".to_string();
        Task::none()
    }

    /// `ELECXREF` reports devices whose parts are spread across the drawing.
    ///
    /// Every part of a device carries the device's tag, so grouping by tag recovers the
    /// device. The report lists multi-part devices first, with each part's class and
    /// position, which is the information a cross-reference table exists to carry.
    pub(super) fn elec_xref(&mut self, i: usize) -> Task<Message> {
        use ::elec::{cross_references, multi_part, PlacedPart};

        let mut parts: Vec<PlacedPart> = Vec::new();
        {
            let doc = &self.tabs[i].scene.document;
            let _model_space = doc.header.model_space_block_handle;
            let spaces = Self::drawing_spaces(doc);
            for entity in doc.entities() {
                if !spaces.contains(&entity.common().owner_handle) { continue; }
                let codec::EntityType::Insert(insert) = entity else {
                    continue;
                };
                if insert.attributes.is_empty() || is_setup_block(&insert.block_name) {
                    continue;
                }
                let attribute_value = |names: &[&str]| {
                    insert
                        .attributes
                        .iter()
                        .find(|attribute| {
                            let tag = attribute.tag.trim().to_ascii_uppercase();
                            names.iter().any(|name| tag == *name)
                        })
                        .map(|attribute| attribute.value.trim().to_string())
                        .unwrap_or_default()
                };
                let tag = attribute_value(&["TAG", "TAG1", "TAG2", "TAGSTR", "WORKID"]);
                let part_id = attribute_value(&["PIN", "CONTACT", "CONTACTNUMBER", "PARTNUMBER2"]);
                let mut placed = PlacedPart::new(
                    &insert.block_name,
                    &tag,
                    insert.insert_point.x,
                    insert.insert_point.y,
                );
                if !part_id.is_empty() {
                    placed = placed.with_part_id(&part_id);
                }
                parts.push(placed);
            }
        }

        if parts.is_empty() {
            self.command_line
                .push_output("ELECXREF: no electrical components found.");
            return Task::none();
        }

        let devices = cross_references(&parts);
        let multi = multi_part(&devices);
        let summary = format!(
            "ELECXREF: {} device(s); {} with more than one part.",
            devices.len(),
            multi.len()
        );
        self.command_line.push_output(&summary);

        if multi.is_empty() {
            self.command_line
                .push_output("  (no multi-part devices on this drawing)");
            return Task::none();
        }

        for device in multi {
            let contacts = device.contact_list();
            let header = if contacts.is_empty() {
                format!("  {} ({} part(s))", device.tag, device.parts.len())
            } else {
                format!(
                    "  {} ({} part(s)): {}",
                    device.tag,
                    device.parts.len(),
                    contacts
                )
            };
            self.command_line.push_output(&header);
            for part in &device.parts {
                let contact = part.part_id.clone().unwrap_or_else(|| "-".to_string());
                let line = format!(
                    "    {:<20} {:<8} x={:.2} y={:.2}",
                    part.class, contact, part.x, part.y
                );
                self.command_line.push_output(&line);
            }
        }

        let untagged = parts.iter().filter(|part| part.tag.trim().is_empty()).count();
        if untagged > 0 {
            let line = format!("  {untagged} untagged symbol(s) cannot be cross-referenced");
            self.command_line.push_output(&line);
        }

        Task::none()
    }

    /// `ELECPART <file.csv>` loads the parts catalogue for this session.
    ///
    /// The sheet is the one the report templates describe: `Part number`, `Manufacturer`,
    /// `Description`, with any extra columns ignored. Loading a second file replaces the
    /// first.
    pub(super) fn elec_parts(&mut self, path: &str) -> Task<Message> {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) => {
                let message = format!("ELECPART: cannot read {path}: {error}");
                self.command_line.push_output(&message);
                return Task::none();
            }
        };
        match ::elec::Catalogue::from_csv(&text) {
            Ok(loaded) => {
                let count = loaded.len();
                if let Ok(mut guard) = catalogue().write() {
                    *guard = Some(loaded);
                }
                let message = format!("ELECPART: loaded {count} part(s) from {path}");
                self.command_line.push_output(&message);
            }
            Err(error) => {
                let message = format!("ELECPART: {path}: {error}");
                self.command_line.push_output(&message);
            }
        }
        Task::none()
    }

    /// Collect the drawing's placed parts (class, tag, contact id, position).
    fn elec_placed_parts(&self, i: usize) -> Vec<::elec::PlacedPart> {
        use ::elec::PlacedPart;
        let doc = &self.tabs[i].scene.document;
        let _model_space = doc.header.model_space_block_handle;
        let spaces = Self::drawing_spaces(doc);
        let mut parts = Vec::new();
        for entity in doc.entities() {
            if !spaces.contains(&entity.common().owner_handle) { continue; }
            let codec::EntityType::Insert(insert) = entity else {
                continue;
            };
            if insert.attributes.is_empty() || is_setup_block(&insert.block_name) {
                continue;
            }
            let attribute_value = |names: &[&str]| {
                insert
                    .attributes
                    .iter()
                    .find(|attribute| {
                        let tag = attribute.tag.trim().to_ascii_uppercase();
                        names.iter().any(|name| tag == *name)
                    })
                    .map(|attribute| attribute.value.trim().to_string())
                    .unwrap_or_default()
            };
            let tag = attribute_value(&["TAG", "TAG1", "TAG2", "TAGSTR", "WORKID"]);
            let part_id = attribute_value(&["PIN", "CONTACT", "CONTACTNUMBER", "PARTNUMBER2"]);
            let mut placed = PlacedPart::new(
                &insert.block_name,
                &tag,
                insert.insert_point.x,
                insert.insert_point.y,
            );
            if !part_id.is_empty() {
                placed = placed.with_part_id(&part_id);
            }
            parts.push(placed);
        }
        parts
    }

    /// `ELECXREFWRITE` annotates each part of a multi-part device with the references to
    /// its other parts, the way an ECAD prints `13-14` beside a coil.
    ///
    /// Text goes on layer `XREF`, offset from the symbol's insertion point. A label at the
    /// same spot is updated rather than duplicated, so the command can be re-run after the
    /// drawing changes.
    pub(super) fn elec_xref_write(&mut self, i: usize) -> Task<Message> {
        use ::elec::{cross_references, multi_part};

        const LABEL_LAYER: &str = "XREF";
        const LABEL_HEIGHT: f64 = 2.0;
        const OFFSET: f64 = 3.0;

        let parts = self.elec_placed_parts(i);
        if parts.is_empty() {
            self.command_line
                .push_output("ELECXREFWRITE: no electrical components found.");
            return Task::none();
        }

        let mut existing: Vec<(codec::Handle, String, f64, f64)> = Vec::new();
        {
            let doc = &self.tabs[i].scene.document;
            let _model_space = doc.header.model_space_block_handle;
            let spaces = Self::drawing_spaces(doc);
            for entity in doc.entities() {
                if !spaces.contains(&entity.common().owner_handle) { continue; }
                if let codec::EntityType::Text(text) = entity {
                    if text.common.layer.eq_ignore_ascii_case(LABEL_LAYER) {
                        existing.push((
                            text.common.handle,
                            text.value.clone(),
                            text.insertion_point.x,
                            text.insertion_point.y,
                        ));
                    }
                }
            }
        }

        let devices = cross_references(&parts);
        let multi = multi_part(&devices);

        let mut updates: Vec<(codec::Handle, String)> = Vec::new();
        let mut creates: Vec<(String, f64, f64)> = Vec::new();
        let mut unchanged = 0usize;

        for device in &multi {
            for (index, part) in device.parts.iter().enumerate() {
                let reference = device.reference_for(index);
                if reference.is_empty() {
                    continue;
                }
                let x = part.x + OFFSET;
                let y = part.y + OFFSET;
                let reuse = existing
                    .iter()
                    .find(|(_, _, ex, ey)| (ex - x).abs() <= 0.1 && (ey - y).abs() <= 0.1);
                match reuse {
                    Some((handle, current, _, _)) if current == &reference => unchanged += 1,
                    Some((handle, _, _, _)) => updates.push((*handle, reference)),
                    None => creates.push((reference, x, y)),
                }
            }
        }

        let summary = format!(
            "ELECXREFWRITE: {} device(s); {} label(s) created, {} updated, {} unchanged.",
            multi.len(),
            creates.len(),
            updates.len(),
            unchanged
        );
        self.command_line.push_output(&summary);

        if creates.is_empty() && updates.is_empty() {
            return Task::none();
        }
        self.push_undo_snapshot(i, "ELECXREFWRITE");
        self.tabs[i].scene.ensure_layer(LABEL_LAYER);
        for (handle, value) in updates {
            let doc = &mut self.tabs[i].scene.document;
            if let Some(codec::EntityType::Text(text)) = doc.get_entity_mut(handle) {
                text.value = value;
            }
        }
        for (value, x, y) in creates {
            let mut text = codec::entities::Text::new();
            text.value = value;
            text.insertion_point = codec::types::Vector3::new(x, y, 0.0);
            text.height = LABEL_HEIGHT;
            text.common.layer = LABEL_LAYER.to_string();
            self.tabs[i].scene.add_entity(codec::EntityType::Text(text));
        }
        self.tabs[i].dirty = true;
        Task::none()
    }

    /// `ELECREPORT <file.xlsx>` writes the drawing's reports into one workbook.
    ///
    /// Three sheets, matching the column names of the shipped templates: the bill of
    /// material (enriched from the parts catalogue when one is loaded), the wire list, and
    /// the terminal strips.
    pub(super) fn elec_report(&mut self, i: usize, path: &str) -> Task<Message> {
        use ::elec::{bom_sheet, terminal_sheet, wire_rows_with_pins, wire_sheet_from};

        const TOLERANCE: f64 = 0.05;

        let mut components = self.elec_components(i);
        if let Ok(guard) = catalogue().read() {
            if let Some(catalogue) = guard.as_ref() {
                for component in &mut components {
                    catalogue.enrich(component);
                }
            }
        }
        let rows = bill_of_materials(&components);

        let graph = {
            let doc = &self.tabs[i].scene.document;
            wire_graph(doc)
        };
        let netlist = graph.netlist(TOLERANCE);
        let wire_rows = wire_rows_with_pins(&graph.wires, &netlist, &graph.anchors);
        let anchors = &graph.anchors;
        let label_of = |id: u64| graph.label_of(id);
        let mut terminals: Vec<(String, Vec<String>)> = Vec::new();
        for anchor in anchors {
            if ::elec::parse_terminal_tag(&anchor.label).is_none() {
                continue;
            }
            let peers: Vec<String> = netlist
                .nets
                .iter()
                .find(|net| net.anchors.contains(&anchor.id))
                .map(|net| {
                    net.anchors
                        .iter()
                        .filter(|id| **id != anchor.id)
                        .map(|id| label_of(*id))
                        .collect()
                })
                .unwrap_or_default();
            terminals.push((anchor.label.clone(), peers));
        }
        let strips = ::elec::terminal_strips(&terminals);

        let mut sheets = vec![
            bom_sheet(&rows),
            wire_sheet_from(&wire_rows),
            terminal_sheet(&strips),
        ];
        // A cable schedule travels with the rest of the report when the project has cables.
        if let Some(cables) = super::elec_cables::cable_sheet(&self.tabs[i].scene.document) {
            sheets.push(cables);
        }

        match write_xlsx(path, &sheets) {
            Ok(()) => {
                let cables = sheets
                    .get(3)
                    .map(|sheet| sheet.body_len())
                    .unwrap_or(0);
                let summary = format!(
                    "ELECREPORT: wrote {path} (BOM {} row(s), wire list {} row(s), terminals {} row(s), cables {} row(s)).",
                    sheets[0].body_len(),
                    sheets[1].body_len(),
                    sheets[2].body_len(),
                    cables
                );
                self.command_line.push_output(&summary);
            }
            Err(error) => {
                let message = format!("ELECREPORT: cannot write {path}: {error}");
                self.command_line.push_output(&message);
            }
        }
        Task::none()
    }

    /// `ELECINSERT <block> [part]` places another instance of a symbol already in the
    /// drawing.
    ///
    /// This is the cheapest honest way to get a new component on the sheet: the block comes
    /// from the drawing itself, so the geometry is whatever the project's symbols are. The
    /// new instance is tagged with the next free number for its class and, when a part
    /// number is given, priced from the parts catalogue.
    pub(super) fn elec_insert(
        &mut self,
        i: usize,
        block: &str,
        part: Option<&str>,
    ) -> Task<Message> {
        let mut template: Option<codec::entities::Insert> = None;
        let mut tagged: Vec<(String, String)> = Vec::new();
        let mut blocks_in_drawing: Vec<String> = Vec::new();
        let mut rightmost = f64::MIN;
        let mut lowest = f64::MAX;
        let mut placed = 0usize;
        {
            let doc = &self.tabs[i].scene.document;
            let _model_space = doc.header.model_space_block_handle;
            let spaces = Self::drawing_spaces(doc);
            for entity in doc.entities() {
                if !spaces.contains(&entity.common().owner_handle) { continue; }
                let codec::EntityType::Insert(insert) = entity else {
                    continue;
                };
                if is_setup_block(&insert.block_name) {
                    continue;
                }
                if !blocks_in_drawing
                    .iter()
                    .any(|name| name.eq_ignore_ascii_case(&insert.block_name))
                {
                    blocks_in_drawing.push(insert.block_name.clone());
                }
                // Placement clears *every* symbol, not just the same block: inserting a
                // motor next to a contactor must not drop it on top of the contactor.
                placed += 1;
                rightmost = rightmost.max(insert.insert_point.x);
                lowest = lowest.min(insert.insert_point.y);
                // Tags are collected per *class*, not per block: HCR1 and CONT-NO are both
                // contactors, so they share the -K sequence and must not both start at -K1.
                let tag = insert
                    .attributes
                    .iter()
                    .find(|attribute| is_tag_attribute(&attribute.tag))
                    .map(|attribute| attribute.value.trim().to_string())
                    .unwrap_or_default();
                tagged.push((insert.block_name.clone(), tag));
                if !insert.block_name.eq_ignore_ascii_case(block) {
                    continue;
                }
                if template.is_none() {
                    template = Some(insert.clone());
                }
            }
        }
        // A block's class comes from the library; without one, the block name is the class.
        let class_of = |block_name: &str| {
            library_index()
                .read()
                .ok()
                .and_then(|index| {
                    index
                        .get(&block_name.to_ascii_uppercase())
                        .map(|entry| entry.class.clone())
                })
                .unwrap_or_else(|| block_name.to_string())
        };

        let template = match template {
            Some(template) => Some(template),
            None => {
                // No instance in the drawing, but the block may exist (a symbol library
                // loads its geometry as block definitions).
                let block_exists = self.tabs[i]
                    .scene
                    .document
                    .block_records
                    .get(block)
                    .is_some();
                if !block_exists {
                    let available = if blocks_in_drawing.is_empty() {
                        "none".to_string()
                    } else {
                        blocks_in_drawing.join(", ")
                    };
                    let message = format!(
                        "ELECINSERT: no block named \"{block}\" in this drawing (available: {available}). Load a symbol library with ELECLIB <folder>."
                    );
                    self.command_line.push_output(&message);
                    return Task::none();
                }
                None
            }
        };

        // The library knows what class a symbol is, and the class decides the tag prefix:
        // a relay gets -K, a drive -U, a terminal -X.
        let class = class_of(block);
        // Numbering shares one sequence per *prefix*, not per class: a relay (HCR1) and a
        // contactor (CONT-NO) are both -K devices, so they must not both start at -K1.
        let prefix = ::elec::default_prefix(&class);
        let tags: Vec<String> = tagged
            .iter()
            .filter(|(block_name, _)| ::elec::default_prefix(&class_of(block_name)) == prefix)
            .map(|(_, tag)| tag.clone())
            .collect();
        let tag_refs: Vec<&str> = tags.iter().map(String::as_str).collect();
        let new_tag = ::elec::next_tag(&tag_refs, &class);
        // A drawing with no components of this block has no anchor to place beside, and
        // `f64::MIN` is finite, so the count - not the sentinel - decides.
        let new_position = if placed == 0 {
            codec::types::Vector3::ZERO
        } else {
            codec::types::Vector3::new(rightmost + 40.0, lowest, 0.0)
        };

        let record = part.and_then(|part| {
            catalogue()
                .read()
                .ok()
                .and_then(|guard| guard.as_ref().and_then(|cat| cat.lookup(part)).cloned())
        });

        // Either clone the instance already on the sheet, or build one from the block's
        // attribute definitions when the geometry exists but has never been placed here.
        let cloned = template.is_some();
        let mut insert = match template {
            Some(template) => {
                let delta = codec::types::Vector3::new(
                    new_position.x - template.insert_point.x,
                    new_position.y - template.insert_point.y,
                    0.0,
                );
                let mut insert = template;
                for attribute in &mut insert.attributes {
                    attribute.insertion_point.x += delta.x;
                    attribute.insertion_point.y += delta.y;
                    attribute.alignment_point.x += delta.x;
                    attribute.alignment_point.y += delta.y;
                }
                insert.insert_point = new_position;
                insert
            }
            None => {
                let mut insert = codec::entities::Insert::new(block, new_position);
                let definitions = self.elec_block_definitions(i, block);
                if definitions.is_empty() {
                    // A library symbol may be plain geometry with no attribute definitions.
                    // Without attributes it would not be a component at all, so use the
                    // attribute set the library recorded for it - that is the set the
                    // project's own drawings carry (TAG1, CAT, MFG, DESC1, TERM01, XREF...).
                    let mut template: Vec<String> = library_index()
                        .read()
                        .ok()
                        .and_then(|index| {
                            index
                                .get(&block.to_ascii_uppercase())
                                .map(|entry| entry.attributes.clone())
                        })
                        .filter(|tags| !tags.is_empty())
                        .unwrap_or_else(|| {
                            vec![
                                "TAG1".to_string(),
                                "CAT".to_string(),
                                "MFG".to_string(),
                                "DESC1".to_string(),
                            ]
                        });
                    // A symbol whose recorded set has no tag attribute (a terminal block
                    // template that only lists X1TERM01...) would insert untagged and
                    // vanish from every report. Give it one.
                    if !template.iter().any(|tag| is_tag_attribute(tag)) {
                        template.insert(0, "TAG1".to_string());
                    }
                    let insert_height = library_index()
                        .read()
                        .ok()
                        .and_then(|index| {
                            index
                                .get(&block.to_ascii_uppercase())
                                .map(|entry| entry.label_height)
                        })
                        .filter(|height| *height > 0.0)
                        .unwrap_or(2.0);
                    let step = insert_height * 1.25;
                    for (row, tag) in template.iter().enumerate() {
                        let mut attribute =
                            codec::entities::AttributeEntity::new(tag.clone(), String::new());
                        attribute.height = insert_height;
                        attribute.insertion_point = codec::types::Vector3::new(
                            new_position.x,
                            new_position.y - insert_height - row as f64 * step,
                            0.0,
                        );
                        attribute.alignment_point = attribute.insertion_point;
                        insert.attributes.push(attribute);
                    }
                } else {
                    for definition in definitions {
                        // `None` takes the definition's own default value.
                        let mut attribute =
                            codec::entities::AttributeEntity::from_definition(&definition, None);
                        attribute.insertion_point.x += new_position.x;
                        attribute.insertion_point.y += new_position.y;
                        attribute.alignment_point.x += new_position.x;
                        attribute.alignment_point.y += new_position.y;
                        insert.attributes.push(attribute);
                    }
                }
                insert
            }
        };

        // A clone arrives carrying the template's handle, and its attributes carry the
        // template's attribute handles. A drawing cannot hold two objects under one handle:
        // reports that resolve entities by handle would see the last instance everywhere.
        // Strip the identity so the scene allocates a fresh one for the insert and for every
        // attribute (`adopt_attributes`).
        if cloned {
            insert.common.handle = codec::Handle::NULL;
            insert.common.owner_handle = codec::Handle::NULL;
            insert.seqend_handle = None;
            for attribute in &mut insert.attributes {
                attribute.common.handle = codec::Handle::NULL;
                attribute.common.owner_handle = codec::Handle::NULL;
                if let Some(model) = &mut attribute.embedded_mtext {
                    model.common.handle = codec::Handle::NULL;
                    model.common.owner_handle = codec::Handle::NULL;
                }
            }
        }

        for attribute in &mut insert.attributes {
            let name = attribute.tag.trim().to_ascii_uppercase();
            if matches!(
                name.as_str(),
                "TAG" | "TAG1" | "TAG2" | "TAGSTR" | "WORKID"
            ) {
                attribute.value = new_tag.clone();
            } else if let Some(part) = part {
                if matches!(name.as_str(), "CAT" | "PART" | "PARTNUMBER" | "CATALOGUEID") {
                    attribute.value = part.to_string();
                }
            }
            if let Some(record) = record.as_ref() {
                if matches!(name.as_str(), "MFG" | "MANUFACTURER" | "MFR") {
                    if let Some(manufacturer) = &record.manufacturer {
                        attribute.value = manufacturer.clone();
                    }
                }
                if matches!(name.as_str(), "DESC" | "DESC1" | "DESC2" | "DESCRIPTION") {
                    if let Some(description) = &record.description {
                        attribute.value = description.clone();
                    }
                }
            }
        }

        self.push_undo_snapshot(i, "ELECINSERT");
        self.tabs[i]
            .scene
            .add_entity(codec::EntityType::Insert(insert));
        self.tabs[i].dirty = true;

        let detail = match part {
            Some(part) => format!(" with part {part}"),
            None => String::new(),
        };
        let message = format!(
            "ELECINSERT: placed {block} as {new_tag}{detail} at {:.2},{:.2}.",
            new_position.x, new_position.y
        );
        self.command_line.push_output(&message);
        Task::none()
    }

    /// Attribute definitions belonging to one block, as owned values.
    pub(super) fn elec_block_definitions(
        &self,
        i: usize,
        block: &str,
    ) -> Vec<codec::entities::AttributeDefinition> {
        let doc = &self.tabs[i].scene.document;
        let Some(record) = doc.block_records.get(block) else {
            return Vec::new();
        };
        // Attribute definitions are owned by the block record, not by the BLOCK entity.
        let record_handle = record.handle;
        let entity_handles = record.entity_handles.clone();
        doc.entities()
            .filter_map(|entity| match entity {
                codec::EntityType::AttributeDefinition(definition)
                    if definition.common.owner_handle == record_handle
                        || entity_handles.contains(&definition.common.handle) =>
                {
                    Some(definition.clone())
                }
                _ => None,
            })
            .collect()
    }

    /// `ELECLIB <folder>` loads a symbol library: one DXF per symbol, the file name giving
    /// the block name.
    ///
    /// A library folder of plain drawings is the simplest thing that can work - no
    /// manifest, no metadata, just "CONTACTOR.dxf draws a contactor". Each drawing's model
    /// space becomes a block definition, so `ELECINSERT CONTACTOR` can then place it even
    /// on a drawing that has never contained one.
    pub(crate) fn elec_lib(&mut self, i: usize, folder: &str) -> Task<Message> {
        let directory = std::path::Path::new(folder);
        if !directory.is_dir() {
            let message = format!("ELECLIB: {folder} is not a folder.");
            self.command_line.push_output(&message);
            return Task::none();
        }

        let mut paths: Vec<std::path::PathBuf> = Vec::new();
        if let Ok(entries) = std::fs::read_dir(directory) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("dxf"))
                {
                    paths.push(path);
                }
            }
        }
        paths.sort();

        let mut loaded = 0usize;
        let mut skipped = 0usize;
        let mut offset_points = 0usize;
        let mut measured_offsets: std::collections::BTreeMap<String, Vec<(String, f64, f64)>> =
            std::collections::BTreeMap::new();
        let mut failed: Vec<String> = Vec::new();
        let manifest = read_library_manifest(directory);

        for path in paths {
            let Some(stem) = path
                .file_stem()
                .and_then(|name| name.to_str())
                .map(|name| name.to_string())
            else {
                continue;
            };
            if self.tabs[i].scene.document.block_records.get(&stem).is_some() {
                skipped += 1;
                continue;
            }
            match crate::io::load_file(&path) {
                Ok(document) => {
                    let mut entities: Vec<codec::EntityType> =
                        document.entities().cloned().collect();
                    if entities.is_empty() {
                        // A symbol file keeps its geometry inside the block definition
                        // rather than in model space, which is how a harvested library is
                        // written. Take the block's own entities, attribute definitions
                        // included.
                        let first = document
                            .block_records
                            .iter()
                            .map(|record| record.name.clone())
                            .find(|name| !name.starts_with('*'));
                        if let Some(block_name) = first {
                            if let Some(record) = document.block_records.get(&block_name) {
                                // Block contents are owned by the block **record** (or listed
                                // in its `entity_handles`); `block_entity_handle` is the BLOCK
                                // entity marker itself. Filtering on the marker returns no
                                // entities, so every symbol whose geometry lives in its block
                                // definition loaded as "no geometry".
                                let owner = record.handle;
                                let owned = record.entity_handles.clone();
                                entities = document
                                    .entities()
                                    .filter(|entity| {
                                        entity.common().owner_handle == owner
                                            || owned.contains(&entity.common().handle)
                                    })
                                    .cloned()
                                    .collect();
                            }
                        }
                    }
                    if entities.is_empty() {
                        failed.push(format!("{stem}: no geometry"));
                        continue;
                    }
                    // Give the block attribute *definitions* from the library template, so
                    // the symbol is self-describing: the app's own INSERT can create the
                    // attributes, and `ATTDEF`/`ATTMAN` have something to show. Harvested
                    // symbols carry their attributes on the insert and have none of their
                    // own, which is exactly why they arrive mute.
                    let template = manifest
                        .get(&stem.to_ascii_uppercase())
                        .map(|entry| entry.attributes.clone())
                        .unwrap_or_default();
                    let already_defined = entities.iter().any(|entity| {
                        matches!(entity, codec::EntityType::AttributeDefinition(_))
                    });
                    if !already_defined && !template.is_empty() {
                        for (row, tag) in template.iter().enumerate() {
                            let mut definition = codec::entities::AttributeDefinition::new(
                                tag.clone(),
                                tag.clone(),
                                String::new(),
                            );
                            definition.height = 2.0;
                            definition.insertion_point = codec::types::Vector3::new(
                                0.0,
                                4.0 - row as f64 * 2.5,
                                0.0,
                            );
                            definition.alignment_point = definition.insertion_point;
                            entities.push(codec::EntityType::AttributeDefinition(definition));
                        }
                    }
                    // Read the attribute definition points from the entity list that is about
                    // to become the block: looking them up by handle afterwards depends on
                    // block-definition indexing that the loader does not maintain here.
                    let offsets: Vec<(String, f64, f64)> = entities
                        .iter()
                        .filter_map(|entity| match entity {
                            codec::EntityType::AttributeDefinition(definition) => {
                                let tag = definition.tag.trim();
                                if tag.is_empty() {
                                    None
                                } else {
                                    Some((
                                        tag.to_string(),
                                        definition.insertion_point.x,
                                        definition.insertion_point.y,
                                    ))
                                }
                            }
                            _ => None,
                        })
                        .collect();
                    match self.tabs[i].scene.define_block_from_owned_entities(
                        entities,
                        &stem,
                        glam::DVec3::ZERO,
                    ) {
                        Ok(_) => {
                            loaded += 1;
                            offset_points += offsets.len();
                            // Remember where the block's own attribute definitions sit, so
                            // ELECPLACE can put pins on the symbol's edges instead of
                            // stacking every attribute on the insertion point. The catalogue
                            // index is only filled after the whole folder has loaded, so keep
                            // these aside and merge them below.
                            measured_offsets.insert(stem.to_ascii_uppercase(), offsets);
                        }
                        Err(error) => failed.push(format!("{stem}: {error}")),
                    }
                }
                Err(error) => failed.push(format!("{stem}: {error}")),
            }
        }

        if let Ok(mut guard) = library_dir().write() {
            *guard = Some(directory.to_path_buf());
        }
        remember_library(directory);
        let manifest_count = manifest.len();
        if let Ok(mut guard) = library_index().write() {
            // The manifest is the catalogue's source of truth for class/attributes/scale,
            // but the definition points were measured while the blocks were loaded above:
            // keep them instead of replacing the whole index with fresh entries.
            for (symbol, mut entry) in manifest {
                if let Some(offsets) = measured_offsets.remove(&symbol) {
                    entry.attribute_offsets = offsets;
                } else if let Some(existing) = guard.get(&symbol) {
                    entry.attribute_offsets = existing.attribute_offsets.clone();
                }
                guard.insert(symbol, entry);
            }
        }

        // Measure each symbol so its attribute text suits its own size: a drive and a
        // terminal should not get the same label height.
        if let Ok(mut guard) = library_index().write() {
            for entry in guard.values_mut() {
                let segments = self.elec_symbol_segments(i, &entry.name);
                let (mut min_x, mut min_y) = (f64::MAX, f64::MAX);
                let (mut max_x, mut max_y) = (f64::MIN, f64::MIN);
                for ((ax, ay), (bx, by)) in &segments {
                    min_x = min_x.min(*ax).min(*bx);
                    min_y = min_y.min(*ay).min(*by);
                    max_x = max_x.max(*ax).max(*bx);
                    max_y = max_y.max(*ay).max(*by);
                }
                let span = if min_x <= max_x {
                    (max_x - min_x).max(max_y - min_y)
                } else {
                    0.0
                };
                entry.label_height = if span > 0.0 {
                    // The project sheets set text at roughly one eighth of a symbol (their
                    // text is 0.125 tall against symbols under a unit). A "drawing style"
                    // floor of 0.8 was ~16x their text and dwarfed every small symbol.
                    (span / 8.0).clamp(0.01, 50.0)
                } else {
                    0.125
                };
            }
        }

        // How much geometry the palette can preview. Zero here means the thumbnails are
        // empty - worth knowing without opening the palette and squinting at it.
        let preview_segments: usize = self
            .elec_library_symbols()
            .iter()
            .map(|row| row.2.len())
            .sum();
        let summary = format!(
            "ELECLIB: loaded {loaded} symbol(s) from {folder}; {skipped} already in this drawing, {} failed, {manifest_count} in the catalogue, {offset_points} attribute definition point(s), {preview_segments} preview segment(s).",
            failed.len()
        );
        self.command_line.push_output(&summary);
        for failure in &failed {
            let line = format!("  {failure}");
            self.command_line.push_output(&line);
        }
        Task::none()
    }

    /// `ELECTSIDES` reports what each terminal connects to on each of its two sides.
    ///
    /// A terminal is a bridge between two nets, not a node in one. The geometric netlist
    /// cannot say this - it only knows points - so this pass walks the wires *without*
    /// crossing the terminal and reports each side separately.
    pub(super) fn elec_sides(&mut self, i: usize) -> Task<Message> {
        use ::elec::terminal_sides;

        const TOLERANCE: f64 = 0.05;

        let (wires, anchors) = self.elec_collect(i);
        if anchors.is_empty() {
            self.command_line
                .push_output("ELECTSIDES: no components found.");
            return Task::none();
        }

        let terminals: Vec<u64> = anchors
            .iter()
            .filter(|anchor| ::elec::parse_terminal_tag(&anchor.label).is_some())
            .map(|anchor| anchor.id)
            .collect();
        if terminals.is_empty() {
            self.command_line
                .push_output("ELECTSIDES: no terminals found (no component tagged strip:index).");
            return Task::none();
        }

        let label_of = |id: u64| {
            anchors
                .iter()
                .find(|anchor| anchor.id == id)
                .map(|anchor| anchor.label.clone())
                .unwrap_or_else(|| "?".to_string())
        };

        let results = terminal_sides(&wires, &anchors, &terminals, TOLERANCE);
        let through = results.iter().filter(|entry| entry.is_through()).count();
        let single = results
            .iter()
            .filter(|entry| entry.sides.len() == 1)
            .count();
        let unconnected = results
            .iter()
            .filter(|entry| entry.is_unconnected())
            .count();

        let summary = format!(
            "ELECTSIDES: {} terminal(s); {through} through, {single} single-sided, {unconnected} unconnected.",
            results.len()
        );
        self.command_line.push_output(&summary);

        for entry in &results {
            if entry.is_unconnected() {
                let line = format!("  {}: unconnected", entry.label);
                self.command_line.push_output(&line);
                continue;
            }
            let mut parts: Vec<String> = Vec::new();
            for (index, side) in entry.sides.iter().enumerate() {
                let name = if index == 0 { "A" } else { "B" };
                let peers: Vec<String> = side.anchors.iter().map(|id| label_of(*id)).collect();
                let peer_text = if peers.is_empty() {
                    "(nothing)".to_string()
                } else {
                    peers.join(", ")
                };
                parts.push(format!("{name}: {peer_text} [{} wire(s)]", side.wires.len()));
            }
            if entry.sides.len() == 1 {
                parts.push("B: (nothing)".to_string());
            }
            let line = format!("  {}: {}", entry.label, parts.join(" | "));
            self.command_line.push_output(&line);
        }

        Task::none()
    }
    /// `ELECPLACE <block>` starts click-to-place for a library symbol.
    ///
    /// Everything the command needs - the attribute template, the next free tag, the
    /// catalogue values - is resolved here, and the palette modal is closed so the canvas
    /// takes the next click.
    pub(super) fn elec_place(&mut self, i: usize, block: &str) -> Task<Message> {
        use crate::modules::electrical::place::{PlaceSymbolCommand, PlacedAttribute};

        let class = library_index()
            .read()
            .ok()
            .and_then(|index| {
                index
                    .get(&block.to_ascii_uppercase())
                    .map(|entry| entry.class.clone())
            })
            .unwrap_or_else(|| block.to_string());

        // Tags already in this family, so the new instance joins the sequence.
        let prefix = ::elec::default_prefix(&class);
        let (_, anchors) = self.elec_collect(i);
        let tags: Vec<String> = anchors
            .iter()
            .filter(|anchor| anchor.label.starts_with(prefix.as_str()))
            .map(|anchor| anchor.label.clone())
            .collect();
        let tag_refs: Vec<&str> = tags.iter().map(String::as_str).collect();
        let new_tag = ::elec::next_tag(&tag_refs, &class);

        let entry = library_index()
            .read()
            .ok()
            .and_then(|index| index.get(&block.to_ascii_uppercase()).cloned());
        let mut template: Vec<String> = entry
            .as_ref()
            .map(|entry| entry.attributes.clone())
            .filter(|attributes| !attributes.is_empty())
            .unwrap_or_else(|| vec!["TAG1".to_string(), "CAT".to_string()]);
        if !template.iter().any(|tag| is_tag_attribute(tag)) {
            template.insert(0, "TAG1".to_string());
        }

        // Attribute definitions recorded in the block travel with the symbol; placing the
        // insert puts each attribute on its own definition point instead of stacking them.
        let offsets = entry
            .as_ref()
            .map(|entry| entry.attribute_offsets.clone())
            .unwrap_or_default();
        let attributes: Vec<PlacedAttribute> = template
            .iter()
            .map(|tag| PlacedAttribute {
                tag: tag.clone(),
                value: if is_tag_attribute(tag) {
                    new_tag.clone()
                } else {
                    String::new()
                },
                offset: offsets
                    .iter()
                    .find(|(defined, _, _)| defined.eq_ignore_ascii_case(tag))
                    .map(|(_, x, y)| (*x, *y)),
            })
            .collect();

        self.active_modal = None;
        // Label height comes from the symbol's own size, measured when the library loaded;
        // the scale is the one the source drawings insert it at.
        let (label_height, symbol_scale) = entry
            .as_ref()
            .map(|entry| (entry.label_height, entry.scale))
            .unwrap_or((0.125, 1.0));
        // The measured height is in symbol units; the sheet sees them through the
        // library scale, so the text must be scaled with the geometry.
        let label_height = if label_height > 0.0 { label_height } else { 0.125 };
        let label_height = label_height * symbol_scale;
        let command = PlaceSymbolCommand::new(
            block.to_string(),
            attributes,
            label_height,
            symbol_scale,
        );
        let prompt = format!("ELECPLACE {block}  Specify insertion point:");
        self.tabs[i].active_cmd = Some(Box::new(command));
        self.command_line.push_output(&prompt);
        Task::none()
    }



    /// `ELECSETATTR <tag> <NAME=value>[,<NAME=value>...]` edits a placed component's
    /// attributes.
    ///
    /// Attributes are addressed by tag rather than by selection so the edit is scriptable
    /// and repeatable - the same reason the rest of this layer is command driven. All
    /// inserts carrying the tag change together, which is what you want when a device has
    /// several parts (a coil and its contacts share `-K1`).
    pub(super) fn elec_set_attr(&mut self, i: usize, tag: &str, assignments: &str) -> Task<Message> {
        let tag = tag.trim();
        if tag.is_empty() {
            self.command_line
                .push_output("ELECSETATTR: give a component tag, e.g. ELECSETATTR -K1 CAT=3RT2016-1AB01");
            return Task::none();
        }

        let mut pairs: Vec<(String, String)> = Vec::new();
        for part in assignments.split(',') {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }
            let Some((name, value)) = part.split_once('=') else {
                let message = format!("ELECSETATTR: \"{part}\" is not NAME=value.");
                self.command_line.push_output(&message);
                return Task::none();
            };
            let name = name.trim().to_ascii_uppercase();
            if name.is_empty() {
                let message = format!("ELECSETATTR: \"{part}\" has no attribute name.");
                self.command_line.push_output(&message);
                return Task::none();
            }
            pairs.push((name, value.trim().to_string()));
        }
        if pairs.is_empty() {
            self.command_line
                .push_output("ELECSETATTR: nothing to set (expected NAME=value).");
            return Task::none();
        }

        let targets: Vec<codec::Handle> = {
            let doc = &self.tabs[i].scene.document;
            doc.entities()
                .filter_map(|entity| match entity {
                    codec::EntityType::Insert(insert) => {
                        let matches = insert.attributes.iter().any(|attribute| {
                            is_tag_attribute(&attribute.tag)
                                && attribute.value.trim().eq_ignore_ascii_case(tag)
                        });
                        matches.then_some(insert.common.handle)
                    }
                    _ => None,
                })
                .collect()
        };

        if targets.is_empty() {
            let message = format!("ELECSETATTR: no component tagged {tag}.");
            self.command_line.push_output(&message);
            return Task::none();
        }

        self.push_undo_snapshot(i, "ELECSETATTR");
        let mut changed = 0usize;
        let mut added = 0usize;
        for handle in &targets {
            let doc = &mut self.tabs[i].scene.document;
            let Some(codec::EntityType::Insert(insert)) = doc.get_entity_mut(*handle) else {
                continue;
            };
            for (name, value) in &pairs {
                if let Some(attribute) = insert
                    .attributes
                    .iter_mut()
                    .find(|attribute| attribute.tag.trim().eq_ignore_ascii_case(name))
                {
                    if attribute.value != *value {
                        attribute.value = value.clone();
                        changed += 1;
                    }
                } else {
                    // The symbol did not carry this attribute; add it rather than silently
                    // dropping the edit, so a spare symbol can be completed by hand.
                    // Height follows the symbol: a fixed 2.0 dwarfs a sub-unit symbol, which
                    // is the bug visible in the drafted schematic.
                    let added_height = library_index()
                        .read()
                        .ok()
                        .and_then(|index| {
                            index
                                .get(&insert.block_name.to_ascii_uppercase())
                                .map(|entry| entry.label_height)
                        })
                        .filter(|height| *height > 0.0)
                        .unwrap_or(0.125);
                    let position = insert.insert_point;
                    let mut attribute = codec::entities::AttributeEntity::new(
                        name.clone(),
                        value.clone(),
                    );
                    attribute.height = added_height;
                    attribute.insertion_point = codec::types::Vector3::new(
                        position.x,
                        position.y
                            - added_height
                            - insert.attributes.len() as f64 * added_height * 1.25,
                        0.0,
                    );
                    attribute.alignment_point = attribute.insertion_point;
                    insert.attributes.push(attribute);
                    added += 1;
                }
            }
        }
        self.tabs[i].dirty = true;

        let listing: Vec<String> = pairs
            .iter()
            .map(|(name, value)| format!("{name}={value}"))
            .collect();
        let message = format!(
            "ELECSETATTR: {tag}: set {} on {} part(s); {changed} value(s) changed, {added} attribute(s) added.",
            listing.join(", "),
            targets.len()
        );
        self.command_line.push_output(&message);
        Task::none()
    }


    /// `ELECWIRE` starts the wire tool.
    pub(super) fn elec_wire(&mut self, i: usize) -> Task<Message> {
        use crate::modules::electrical::wire::WireCommand;

        let command = WireCommand::new();
        self.tabs[i].active_cmd = Some(Box::new(command));
        self.command_line
            .push_output("ELECWIRE  Specify start point:");
        Task::none()
    }

    /// `ELECJUNCTIONS` puts a dot wherever wires actually meet.
    ///
    /// Two wires touching end to end are one conductor; three or more, or a wire ending on
    /// another wire, must be drawn with a dot - on a schematic the dot is what tells an
    /// electrician the wires are connected rather than crossing. Crossing without a shared
    /// point gets nothing, because it is not a connection.
    pub(super) fn elec_junctions(&mut self, i: usize) -> Task<Message> {
        const TOLERANCE: f64 = 0.05;
        const DOT_LAYER: &str = "JUNCTIONS";
        const DOT_RADIUS: f64 = 1.0;

        let (wires, _) = self.elec_collect(i);
        if wires.is_empty() {
            self.command_line
                .push_output("ELECJUNCTIONS: no wires found.");
            return Task::none();
        }

        let points = ::elec::junction_points(&wires, TOLERANCE);
        if points.is_empty() {
            let message = format!(
                "ELECJUNCTIONS: {} wire segment(s); no point where three or more meet, nothing to draw.",
                wires.len()
            );
            self.command_line.push_output(&message);
            return Task::none();
        }

        // Existing dots, so a second run does not stack another circle on the same point.
        let existing: Vec<(f64, f64)> = {
            let doc = &self.tabs[i].scene.document;
            doc.entities()
                .filter_map(|entity| match entity {
                    codec::EntityType::Circle(circle)
                        if circle.common.layer.eq_ignore_ascii_case(DOT_LAYER) =>
                    {
                        Some((circle.center.x, circle.center.y))
                    }
                    _ => None,
                })
                .collect()
        };

        let mut created = 0usize;
        let mut already = 0usize;
        let mut pending: Vec<(f64, f64)> = Vec::new();
        for point in &points {
            if existing
                .iter()
                .any(|(x, y)| (x - point.x).abs() <= TOLERANCE && (y - point.y).abs() <= TOLERANCE)
            {
                already += 1;
                continue;
            }
            pending.push((point.x, point.y));
        }

        if pending.is_empty() {
            let message = format!(
                "ELECJUNCTIONS: {} junction(s), all already drawn.",
                points.len()
            );
            self.command_line.push_output(&message);
            return Task::none();
        }

        self.push_undo_snapshot(i, "ELECJUNCTIONS");
        self.tabs[i].scene.ensure_layer(DOT_LAYER);
        for (x, y) in &pending {
            let mut circle = codec::entities::Circle::new();
            circle.center = codec::types::Vector3::new(*x, *y, 0.0);
            circle.radius = DOT_RADIUS;
            circle.common.layer = DOT_LAYER.to_string();
            self.tabs[i]
                .scene
                .add_entity(codec::EntityType::Circle(circle));
            created += 1;
        }
        self.tabs[i].dirty = true;

        let message = format!(
            "ELECJUNCTIONS: {} wire segment(s); {created} junction(s) drawn, {already} already there.",
            wires.len()
        );
        self.command_line.push_output(&message);
        Task::none()
    }


    /// Bounding box of a set of entities, so a new symbol can be based at its own corner
    /// and labelled at a sensible size.
    fn elec_entity_bounds(entities: &[codec::EntityType]) -> Option<(f64, f64, f64, f64)> {
        let mut min_x = f64::MAX;
        let mut min_y = f64::MAX;
        let mut max_x = f64::MIN;
        let mut max_y = f64::MIN;
        let mut take = |x: f64, y: f64| {
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x);
            max_y = max_y.max(y);
        };
        for entity in entities {
            match entity {
                codec::EntityType::Line(line) => {
                    take(line.start.x, line.start.y);
                    take(line.end.x, line.end.y);
                }
                codec::EntityType::Circle(circle) => {
                    let r = circle.radius.abs();
                    take(circle.center.x - r, circle.center.y - r);
                    take(circle.center.x + r, circle.center.y + r);
                }
                codec::EntityType::Arc(arc) => {
                    let r = arc.radius.abs();
                    take(arc.center.x - r, arc.center.y - r);
                    take(arc.center.x + r, arc.center.y + r);
                }
                codec::EntityType::Text(text) => {
                    take(text.insertion_point.x, text.insertion_point.y);
                }
                codec::EntityType::LwPolyline(polyline) => {
                    for vertex in &polyline.vertices {
                        take(vertex.location.x, vertex.location.y);
                    }
                }
                codec::EntityType::Insert(insert) => {
                    take(insert.insert_point.x, insert.insert_point.y);
                }
                _ => {}
            }
        }
        if min_x > max_x || min_y > max_y {
            None
        } else {
            Some((min_x, min_y, max_x, max_y))
        }
    }

    /// `ELECMAKESYMBOL <name> [CLASS=<class>]` turns the selected geometry into a symbol.
    ///
    /// The block is defined in this drawing and written to the loaded library as its own
    /// DXF, so it can be inserted into any drawing afterwards - that is what makes it a
    /// module rather than a one-off. The geometry is not consumed: the originals stay where
    /// they were, because deleting a user's work on a naming typo is not recoverable in a
    /// way they would thank you for.
    ///
    /// Attribute definitions that are already part of the selection are kept, in order, and
    /// their tags are written to the library manifest as well. That is what lets a custom
    /// IO module, drive or switch declare its own `X1TERM01`-style pins: the tags travel
    /// with the symbol, so `ELECLIB` recreates the attributes and the netlist can find the
    /// pins after a reload. The four component attributes are only added when the selection
    /// does not already define them.
    pub(crate) fn elec_make_symbol(
        &mut self,
        i: usize,
        name: &str,
        class: Option<&str>,
        pins: Option<usize>,
    ) -> Task<Message> {
        const DEFAULT_HEIGHT: f64 = 2.0;

        let name = name.trim();
        if name.is_empty() {
            self.command_line
                .push_output("ELECMAKESYMBOL: give the symbol a name.");
            return Task::none();
        }

        let handles: Vec<codec::Handle> = self.tabs[i]
            .scene
            .selected_entities()
            .into_iter()
            .map(|(handle, _)| handle)
            .collect();
        if handles.is_empty() {
            let message = format!(
                "ELECMAKESYMBOL: select the geometry for \"{name}\" first, then run the command."
            );
            self.command_line.push_output(&message);
            return Task::none();
        }
        if self.tabs[i].scene.document.block_records.get(name).is_some() {
            let message = format!("ELECMAKESYMBOL: a block named \"{name}\" already exists.");
            self.command_line.push_output(&message);
            return Task::none();
        }

        let entities: Vec<codec::EntityType> = {
            let doc = &self.tabs[i].scene.document;
            doc.entities()
                .filter(|entity| handles.contains(&entity.common().handle))
                .cloned()
                .collect()
        };
        let Some((min_x, min_y, max_x, max_y)) = Self::elec_entity_bounds(&entities) else {
            self.command_line.push_output(
                "ELECMAKESYMBOL: the selection has no measurable geometry.",
            );
            return Task::none();
        };

        // Keep the attribute definitions the author already drew; only add the four
        // component attributes that are missing. Every tag, including the pin attributes,
        // is written to the library manifest so a reload recreates the same set.
        let mut attributes = authored_attribute_tags(&entities);
        let authored_count = attributes.len();
        for required in ["TAG1", "CAT", "MFG", "DESC1"] {
            if !attributes
                .iter()
                .any(|tag| tag.eq_ignore_ascii_case(required))
            {
                attributes.push(required.to_string());
            }
        }
        let span = (max_x - min_x).max(max_y - min_y);
        let height = (span / 8.0).clamp(1.0, 4.0).min(if span > 0.0 { span } else { 1.0 });
        let mut with_attributes = entities.clone();
        let mut row = 0usize;
        for (tag, prompt) in [
            ("TAG1", "Tag"),
            ("CAT", "Part number"),
            ("MFG", "Manufacturer"),
            ("DESC1", "Description"),
        ] {
            if attributes[..authored_count]
                .iter()
                .any(|authored| authored.eq_ignore_ascii_case(tag))
            {
                continue;
            }
            let mut definition = codec::entities::AttributeDefinition::new(
                tag.to_string(),
                prompt.to_string(),
                String::new(),
            );
            definition.height = height;
            definition.insertion_point = codec::types::Vector3::new(
                min_x,
                min_y - height * 1.6 * (row as f64 + 1.0),
                0.0,
            );
            definition.alignment_point = definition.insertion_point;
            with_attributes.push(codec::EntityType::AttributeDefinition(definition));
            row += 1;
        }

        // PINS= writes the invisible terminal attributes a module needs. They are spread
        // evenly down the symbol's right (X1) and left (X2) edges, which is the convention
        // the starter symbols use (terminals on both sides), and ELECLIB records their
        // definition points so the netlist anchors each wire to the edge it reaches.
        // `n` is the total pin count: the odd pin goes to the right edge.
        if let Some(count) = pins {
            let count = count.clamp(1, 128);
            let span_y = (max_y - min_y).max(1.0);
            let pin_height = (span_y / (count as f64 * 3.0)).clamp(0.1, 2.5);
            let right = count.div_ceil(2);
            let left = count / 2;
            for (side, x, on_side) in [("X1", max_x, right), ("X2", min_x, left)] {
                for index in 0..on_side {
                    let y = min_y + (index as f64 + 0.5) * span_y / on_side as f64;
                    let tag = format!("{side}TERM{:02}", index + 1);
                    if attributes
                        .iter()
                        .any(|existing| existing.eq_ignore_ascii_case(&tag))
                    {
                        continue;
                    }
                    let mut definition = codec::entities::AttributeDefinition::new(
                        tag.clone(),
                        "Terminal".to_string(),
                        String::new(),
                    );
                    definition.height = pin_height;
                    definition.flags.invisible = true;
                    definition.insertion_point = codec::types::Vector3::new(x, y, 0.0);
                    definition.alignment_point = definition.insertion_point;
                    with_attributes.push(codec::EntityType::AttributeDefinition(definition));
                    attributes.push(tag);
                }
            }
        }
        let _ = DEFAULT_HEIGHT;

        match self.tabs[i].scene.define_block_from_owned_entities(
            with_attributes.clone(),
            name,
            glam::DVec3::new(min_x, min_y, 0.0),
        ) {
            Ok(_) => {}
            Err(error) => {
                let message = format!("ELECMAKESYMBOL: cannot define \"{name}\": {error}");
                self.command_line.push_output(&message);
                return Task::none();
            }
        }
        self.tabs[i].dirty = true;

        // Write the symbol into the library so it outlives this drawing.
        let folder = library_dir()
            .read()
            .ok()
            .and_then(|guard| guard.clone())
            .or_else(remembered_library);
        let mut written: Option<String> = None;
        if let Some(folder) = folder {
            let safe: String = name
                .chars()
                .map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.' { c } else { '_' })
                .collect();
            let path = folder.join(format!("{safe}.dxf"));
            let mut document = codec::Document::new();
            let translate = crate::command::EntityTransform::Translate(glam::DVec3::new(-min_x, -min_y, 0.0));
            let mut ok = true;
            for entity in &with_attributes {
                let mut copy = entity.clone();
                crate::scene::view::dispatch::apply_transform(&mut copy, &translate);
                if document.add_entity(copy).is_err() {
                    ok = false;
                    break;
                }
            }
            if ok && crate::io::save(&document, &path).is_ok() {
                let class = class.map(str::trim).filter(|value| !value.is_empty()).unwrap_or("Symbol");
                let tags = attributes.join("|");
                let manifest = folder.join("library.tsv");
                let mut text = std::fs::read_to_string(&manifest).unwrap_or_else(|_| {
                    "symbol\tclass\tattributes\tsheets\n".to_string()
                });
                if !text.ends_with('\n') {
                    text.push('\n');
                }
                text.push_str(&format!("{safe}\t{class}\t{tags}\t1\n"));
                let _ = std::fs::write(&manifest, text);
                written = Some(path.display().to_string());
            }
        }

        let message = match written {
            Some(path) => format!(
                "ELECMAKESYMBOL: \"{name}\" defined from {} object(s) and written to {path}.",
                entities.len()
            ),
            None => format!(
                "ELECMAKESYMBOL: \"{name}\" defined in this drawing from {} object(s); no library folder loaded, so it was not saved for reuse.",
                entities.len()
            ),
        };
        self.command_line.push_output(&message);
        Task::none()
    }


    /// The loaded symbols with the segments the palette draws, grouped by class then name.
    ///
    /// The geometry comes from the block definitions that `ELECLIB` created, so the
    /// thumbnail is the real symbol rather than a placeholder.
    /// Segments of one symbol's block geometry, for the palette thumbnail.
    ///
    /// Returns empty when the block is missing or its entities are not owned by the block
    /// record - the two reasons a thumbnail can silently draw nothing, which is why the
    /// palette shows the count next to the class while this is being finished.
    fn elec_symbol_segments(&self, i: usize, name: &str) -> Vec<((f64, f64), (f64, f64))> {
        const MAX_SEGMENTS: usize = 400;

        let doc = &self.tabs[i].scene.document;
        let Some(record) = doc.block_records.get(name) else {
            return Vec::new();
        };
        // Block contents are owned by the block **record** (or listed in its
        // entity_handles); `block_entity_handle` is the BLOCK entity itself. Asking the
        // wrong one yields an empty symbol that still looks fine in the palette.
        let record_handle = record.handle;
        let entity_handles = record.entity_handles.clone();
        let mut segments: Vec<((f64, f64), (f64, f64))> = Vec::new();
        for entity in doc.entities() {
            let owned = entity.common().owner_handle == record_handle
                || entity_handles.contains(&entity.common().handle);
            if !owned || segments.len() >= MAX_SEGMENTS {
                continue;
            }
            match entity {
                codec::EntityType::Line(line) => segments.push((
                    (line.start.x, line.start.y),
                    (line.end.x, line.end.y),
                )),
                codec::EntityType::Circle(circle) => {
                    let r = circle.radius.abs();
                    let steps = 16;
                    let mut previous = None;
                    for step in 0..=steps {
                        let angle = std::f64::consts::TAU * (step as f64) / (steps as f64);
                        let point = (
                            circle.center.x + r * angle.cos(),
                            circle.center.y + r * angle.sin(),
                        );
                        if let Some(from) = previous {
                            segments.push((from, point));
                        }
                        previous = Some(point);
                    }
                }
                codec::EntityType::Arc(arc) => {
                    let r = arc.radius.abs();
                    let steps = 8;
                    let sweep = arc.end_angle - arc.start_angle;
                    let mut previous = None;
                    for step in 0..=steps {
                        let angle = arc.start_angle + sweep * (step as f64) / (steps as f64);
                        let point = (
                            arc.center.x + r * angle.cos(),
                            arc.center.y + r * angle.sin(),
                        );
                        if let Some(from) = previous {
                            segments.push((from, point));
                        }
                        previous = Some(point);
                    }
                }
                codec::EntityType::LwPolyline(polyline) => {
                    let mut previous: Option<(f64, f64)> = None;
                    for vertex in &polyline.vertices {
                        let point = (vertex.location.x, vertex.location.y);
                        if let Some(from) = previous {
                            segments.push((from, point));
                        }
                        previous = Some(point);
                    }
                }
                _ => {}
            }
        }
        segments
    }



    /// The loaded symbols as `(name, class)`, grouped by class then name, for the palette.

    /// The loaded symbols with the segments the palette draws, grouped by class then name.
    pub(crate) fn elec_library_symbols(
        &self,
    ) -> Vec<(String, String, Vec<((f64, f64), (f64, f64))>)> {
        let Ok(index) = library_index().read() else {
            return Vec::new();
        };
        let i = self.active_tab;
        let mut rows: Vec<(String, String, Vec<((f64, f64), (f64, f64))>)> = index
            .values()
            .map(|entry| {
                (
                    entry.name.clone(),
                    entry.class.clone(),
                    self.elec_symbol_segments(i, &entry.name),
                )
            })
            .collect();
        rows.sort_by(|a, b| a.1.cmp(&b.1).then_with(|| a.0.cmp(&b.0)));
        rows
    }


    /// `ELECNEW` prepares the drawing for electrical drafting.
    ///
    /// A new drawing arrives as bare model space: no wire layers, no wire-number layer, no
    /// symbol library. Doing that by hand is four commands and a path, and forgetting one
    /// leaves wires on layer 0 where nothing downstream looks for them.
    /// `ELECNEW [template.dxf]` prepares the drawing, optionally from a sheet template.
    ///
    /// The project's own sheets are the authority on frames, title blocks and text sizes -
    /// their content sits at a different scale from their border, so an invented frame
    /// would fight the drawings. Given a template, its layers and model-space geometry are
    /// copied in first, then the electrical setup runs as usual.
    pub(super) fn elec_new(&mut self, i: usize, template: Option<&str>) -> Task<Message> {
        let mut notes: Vec<String> = Vec::new();

        if let Some(path) = template {
            match crate::io::load_file(std::path::Path::new(path)) {
                Ok(document) => {
                    // Bring the template's layers across first: its geometry references them.
                    let mut layer_names: Vec<String> = Vec::new();
                    for record in document.layers.iter() {
                        let name = record.name.clone();
                        if self.tabs[i].scene.document.layers.get(&name).is_none() {
                            let mut layer = record.clone();
                            layer.name = name.clone();
                            let _ = self.tabs[i].scene.document.layers.add(layer);
                        }
                        layer_names.push(name);
                    }

                    let template_spaces = Self::drawing_spaces(&document);
                    let mut copied = 0usize;
                    for entity in document.entities() {
                        if !template_spaces.contains(&entity.common().owner_handle) {
                            continue;
                        }
                        let mut copy = entity.clone();
                        copy.common_mut().handle = codec::Handle::NULL;
                        if self.tabs[i]
                            .scene
                            .document
                            .add_entity(copy)
                            .is_ok()
                        {
                            copied += 1;
                        }
                    }
                    notes.push(format!(
                        "{} object(s) and {} layer(s) from {path}",
                        copied,
                        layer_names.len()
                    ));
                }
                Err(error) => {
                    let message = format!("ELECNEW: cannot read template {path}: {error}");
                    self.command_line.push_output(&message);
                    return Task::none();
                }
            }
        }

        // Layers the electrical commands write to. An existing layer keeps the colour the
        // project gave it; only missing ones are created.
        let wanted: [(&str, i16); 5] = [
            ("WIRES", 3),
            ("JUNCTIONS", 3),
            ("WIRENO", 2),
            ("XREF", 6),
            ("ELEC-SYMBOLS", 7),
        ];
        let mut created: Vec<&str> = Vec::new();
        for (name, color) in wanted {
            let existed = self.tabs[i].scene.document.layers.get(name).is_some();
            self.tabs[i].scene.ensure_layer(name);
            if !existed {
                if let Some(layer) = self.tabs[i].scene.document.layers.get_mut(name) {
                    layer.color = codec::types::Color::from_index(color);
                }
                created.push(name);
            }
        }
        notes.push(format!(
            "{} electrical layer(s) created ({})",
            created.len(),
            if created.is_empty() {
                "none needed".to_string()
            } else {
                created.join(", ")
            }
        ));

        // The library is what makes the sheet draftable.
        let loaded = library_index()
            .read()
            .map(|index| !index.is_empty())
            .unwrap_or(false);
        if !loaded {
            if let Some(directory) = remembered_library() {
                let path = directory.display().to_string();
                let _ = self.elec_lib(i, &path);
                notes.push(format!("library loaded from {path}"));
            } else {
                notes.push("no library remembered - run ELECLIB <folder> once".to_string());
            }
        } else {
            notes.push("library already loaded".to_string());
        }

        self.tabs[i].dirty = true;
        let summary = format!("ELECNEW: {}.", notes.join("; "));
        self.command_line.push_output(&summary);
        self.command_line.push_output(
            "  Next: ELECPALETTE to pick a symbol, ELECWIRE to draw conductors, then ELECNUMBER / ELECBOM.",
        );
        Task::none()
    }





    /// `ELECPROJECT <folder>` reads every drawing in a folder and reports the project,
    /// not just the open sheet.
    ///
    /// Both products people compare this to are project-based: a device appears on several
    /// sheets, the bill of material sums the whole set, and a cross reference is only useful
    /// if it knows which sheet the other part is on. This is the smallest version of that -
    /// scan, aggregate, report - and it runs on the real project exports rather than a
    /// fixture.
    pub(super) fn elec_project(&mut self, folder: Option<&str>, out_path: Option<&str>) -> Task<Message> {
        // With no folder argument the open project file decides which sheets are read.
        let mut sheets: Vec<ProjectSheet> = Vec::new();
        if let Some(folder) = folder {
            let directory = std::path::Path::new(folder);
            if !directory.is_dir() {
                let message = format!("ELECPROJECT: {folder} is not a folder.");
                self.command_line.push_output(&message);
                return Task::none();
            }
            let mut paths: Vec<std::path::PathBuf> = Vec::new();
            if let Ok(entries) = std::fs::read_dir(directory) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path
                        .extension()
                        .is_some_and(|ext| ext.eq_ignore_ascii_case("dxf"))
                    {
                        paths.push(path);
                    }
                }
            }
            paths.sort();
            sheets = paths
                .into_iter()
                .map(|path| ProjectSheet {
                    path,
                    number: String::new(),
                    title: String::new(),
                })
                .collect();
            if let Ok(mut guard) = project_dir().write() {
                *guard = Some(directory.to_path_buf());
            }
        } else if let Some(file) = project_file().read().ok().and_then(|guard| guard.clone()) {
            match Self::read_project(&file.display().to_string()) {
                Ok(list) => sheets = list,
                Err(error) => {
                    let message = format!("ELECPROJECT: {error}");
                    self.command_line.push_output(&message);
                    return Task::none();
                }
            }
        } else {
            self.command_line.push_output(
                "ELECPROJECT: no project open (ELECPROJNEW/ELECPROJOPEN) and no folder given.",
            );
            return Task::none();
        }

        if sheets.is_empty() {
            let message = format!("ELECPROJECT: no .dxf drawings in {}." , folder.unwrap_or("the project"));
            self.command_line.push_output(&message);
            return Task::none();
        }

        // tag -> sheets it appears on; part -> (quantity, sheets)
        let mut device_sheets: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let mut classes: BTreeMap<String, usize> = BTreeMap::new();
        let mut parts: BTreeMap<String, (usize, BTreeSet<String>)> = BTreeMap::new();
        let mut scanned = 0usize;
        let mut failed = 0usize;
        let mut components_total = 0usize;
        // Sheet -> how many components it contributed, for the workbook's sheet summary.
        let mut per_sheet: BTreeMap<String, usize> = BTreeMap::new();

        for entry_sheet in &sheets {
            let Ok(document) = crate::io::load_file(&entry_sheet.path) else {
                failed += 1;
                continue;
            };
            scanned += 1;
            // The project's own number and title identify the sheet; the file name is the
            // fallback when the project file does not supply them.
            let sheet = if entry_sheet.number.is_empty() {
                entry_sheet
                    .path
                    .file_stem()
                    .and_then(|name| name.to_str())
                    .unwrap_or("?")
                    .to_string()
            } else if entry_sheet.title.is_empty() {
                entry_sheet.number.clone()
            } else {
                format!("{} {}", entry_sheet.number, entry_sheet.title)
            };
            let mut on_this_sheet = 0usize;
            for component in Self::components_of(&document) {
                components_total += 1;
                on_this_sheet += 1;
                *classes.entry(component.class.clone()).or_default() += 1;
                let tag = component.tag.trim().to_string();
                if !tag.is_empty() {
                    device_sheets
                        .entry(tag.to_ascii_uppercase())
                        .or_default()
                        .push(sheet.clone());
                }
                if let Some(part) = component.part.clone() {
                    let entry = parts.entry(part).or_default();
                    entry.0 += 1;
                    entry.1.insert(sheet.clone());
                }
            }
            per_sheet.insert(sheet, on_this_sheet);
        }

        let multi_sheet: Vec<(&String, &Vec<String>)> = device_sheets
            .iter()
            .filter(|(_, list)| {
                let unique: BTreeSet<&String> = list.iter().collect();
                unique.len() > 1
            })
            .collect();

        let summary = format!(
            "ELECPROJECT: {scanned} sheet(s) read ({failed} failed); {components_total} component(s) in {} class(es); {} device tag(s), {} used on more than one sheet; {} distinct part(s).",
            classes.len(),
            device_sheets.len(),
            multi_sheet.len(),
            parts.len()
        );
        self.command_line.push_output(&summary);

        if !multi_sheet.is_empty() {
            self.command_line
                .push_output("  Devices across sheets:");
            for (tag, sheets) in multi_sheet.iter().take(15) {
                let unique: BTreeSet<&String> = sheets.iter().collect();
                let listing: Vec<String> = unique.iter().map(|name| (*name).clone()).collect();
                let line = format!("    {tag}: {}", listing.join(", "));
                self.command_line.push_output(&line);
            }
            if multi_sheet.len() > 15 {
                let line = format!("    ... and {} more", multi_sheet.len() - 15);
                self.command_line.push_output(&line);
            }
        }

        if !parts.is_empty() {
            self.command_line.push_output("  Parts across the project:");
            let mut rows: Vec<(&String, &(usize, BTreeSet<String>))> = parts.iter().collect();
            rows.sort_by_key(|(_, (quantity, _))| std::cmp::Reverse(*quantity));
            for (part, (quantity, sheets)) in rows.iter().take(15) {
                let line = format!("    {part}: {quantity} on {} sheet(s)", sheets.len());
                self.command_line.push_output(&line);
            }
        }

        if let Some(path) = out_path {
            match Self::write_project_workbook(path, &parts, &classes, &per_sheet, scanned) {
                Ok(()) => {
                    let message = format!(
                        "  wrote {path}: {} part row(s), {} sheet row(s).",
                        parts.len(),
                        per_sheet.len()
                    );
                    self.command_line.push_output(&message);
                }
                Err(error) => {
                    let message = format!("  cannot write {path}: {error}");
                    self.command_line.push_output(&message);
                }
            }
        }

        Task::none()
    }

    /// Writes the project-wide bill of material and per-sheet summary as a workbook.
    fn write_project_workbook(
        path: &str,
        parts: &BTreeMap<String, (usize, BTreeSet<String>)>,
        classes: &BTreeMap<String, usize>,
        per_sheet: &BTreeMap<String, usize>,
        sheets_read: usize,
    ) -> Result<(), String> {
        use ::elec::{Cell, Sheet};

        let mut bom = Sheet::new(
            "Project BOM",
            &["Part number", "Qty", "Sheets", "Where used"],
        );
        let mut rows: Vec<(&String, &(usize, BTreeSet<String>))> = parts.iter().collect();
        rows.sort_by_key(|(_, (quantity, _))| std::cmp::Reverse(*quantity));
        for (part, (quantity, sheets)) in rows {
            bom.push(vec![
                Cell::text(part.clone()),
                Cell::number(*quantity as f64),
                Cell::number(sheets.len() as f64),
                Cell::text(sheets.iter().cloned().collect::<Vec<_>>().join(", ")),
            ]);
        }

        let mut summary = Sheet::new("Sheets", &["Sheet", "Components"]);
        for (sheet, count) in per_sheet {
            summary.push(vec![Cell::text(sheet.clone()), Cell::number(*count as f64)]);
        }
        summary.push(vec![
            Cell::text("(sheets read)"),
            Cell::number(sheets_read as f64),
        ]);

        let mut by_class = Sheet::new("Classes", &["Class", "Components"]);
        let mut class_rows: Vec<(&String, &usize)> = classes.iter().collect();
        class_rows.sort_by_key(|(_, count)| std::cmp::Reverse(**count));
        for (class, count) in class_rows {
            by_class.push(vec![Cell::text(class.clone()), Cell::number(*count as f64)]);
        }

        write_xlsx(path, &[bom, summary, by_class])
    }


    /// `ELECPLC <tag> <points> <base-address> [x,y]` places a PLC I/O module.
    ///
    /// Both products this is measured against treat a PLC module as one symbol carrying an
    /// address per point, so the module can be referenced as a unit while each I/O point
    /// still has its own address. This builds the module geometry on demand (a body with a
    /// pin per point), tags it, and writes `ADDR01..ADDRnn` from the base address.
    pub(super) fn elec_plc(
        &mut self,
        i: usize,
        tag: &str,
        points: usize,
        base: &str,
        at: Option<(f64, f64)>,
    ) -> Task<Message> {
        let tag = tag.trim();
        if tag.is_empty() || points == 0 {
            self.command_line.push_output(
                "ELECPLC: use ELECPLC <tag> <points> <base-address> [x,y], e.g. ELECPLC -P1 8 I0.0",
            );
            return Task::none();
        }
        if points > 128 {
            self.command_line
                .push_output("ELECPLC: at most 128 points per module.");
            return Task::none();
        }
        let Some(addresses) = Self::plc_addresses(base, points) else {
            let message = format!(
                "ELECPLC: \"{base}\" is not an address like I0.0 or O1.3."
            );
            self.command_line.push_output(&message);
            return Task::none();
        };

        let block_name = format!("PLC-{points}");
        let (x, y) = at.unwrap_or((0.0, 0.0));

        // One body with a pin per point, built once and reused for the same point count.
        if self.tabs[i].scene.document.block_records.get(&block_name).is_none() {
            let spacing = 0.25_f64;
            let width = 0.8_f64;
            let mut entities: Vec<codec::EntityType> = Vec::new();
            let mut line = |ax: f64, ay: f64, bx: f64, by: f64| {
                let mut entity = codec::entities::Line::from_points(
                    codec::types::Vector3::new(ax, ay, 0.0),
                    codec::types::Vector3::new(bx, by, 0.0),
                );
                entity.common.layer = "ELEC-SYMBOLS".to_string();
                entities.push(codec::EntityType::Line(entity));
            };
            let top = points as f64 * spacing;
            line(0.0, 0.0, width, 0.0);
            line(width, 0.0, width, top);
            line(width, top, 0.0, top);
            line(0.0, top, 0.0, 0.0);
            for point in 0..points {
                let y = (point as f64 + 0.5) * spacing;
                line(-0.15, y, 0.0, y);
                line(width, y, width + 0.15, y);
            }
            if let Err(error) = self.tabs[i].scene.define_block_from_owned_entities(
                entities,
                &block_name,
                glam::DVec3::ZERO,
            ) {
                let message = format!("ELECPLC: cannot build {block_name}: {error}");
                self.command_line.push_output(&message);
                return Task::none();
            }
        }

        let spacing = 0.25_f64;
        let mut insert = codec::entities::Insert::new(
            block_name.clone(),
            codec::types::Vector3::new(x, y, 0.0),
        );
        let mut push = |insert: &mut codec::entities::Insert, name: &str, value: &str, row: usize| {
            let mut attribute =
                codec::entities::AttributeEntity::new(name.to_string(), value.to_string());
            attribute.height = 0.1;
            attribute.insertion_point = codec::types::Vector3::new(
                x + 0.9,
                y + (row as f64 + 1.0) * spacing,
                0.0,
            );
            attribute.alignment_point = attribute.insertion_point;
            insert.attributes.push(attribute);
        };
        push(&mut insert, "TAG1", tag, 0);
        push(&mut insert, "DESC1", "PLC module", 1);
        for (point, address) in addresses.iter().enumerate() {
            push(&mut insert, &format!("ADDR{:02}", point + 1), address, point + 2);
            // The pin's own coordinates, so a later pass can follow the wire from the point
            // instead of guessing where the generated geometry put it.
            let pin_y = y + (point as f64 + 0.5) * spacing;
            push(
                &mut insert,
                &format!("X1TERM{:02}", point + 1),
                &format!("{:.4},{:.4}", x - 0.15, pin_y),
                point + 2,
            );
        }

        self.push_undo_snapshot(i, "ELECPLC");
        self.tabs[i]
            .scene
            .add_entity(codec::EntityType::Insert(insert));
        self.tabs[i].dirty = true;

        let summary = format!(
            "ELECPLC: {tag} with {points} point(s), {}..{} at {x:.2},{y:.2}.",
            addresses.first().cloned().unwrap_or_default(),
            addresses.last().cloned().unwrap_or_default()
        );
        self.command_line.push_output(&summary);
        Task::none()
    }

    /// Addresses for `points` consecutive I/O points from a base like `I0.0`.
    ///
    /// The convention both products use groups eight points per byte: `I0.0`-`I0.7`, then
    /// `I1.0`. Returns `None` when the base does not look like an address.
    fn plc_addresses(base: &str, points: usize) -> Option<Vec<String>> {
        let base = base.trim();
        let split = base.rfind('.')?;
        let (head, last) = base.split_at(split);
        let last = &last[1..];
        if last.is_empty() || !last.chars().all(|c| c.is_ascii_digit()) {
            return None;
        }
        let bit_start: usize = last.parse().ok()?;
        if bit_start > 7 {
            return None;
        }
        // `I0` or `I0.` prefix stays; only the byte number changes on overflow.
        let byte_start: usize = head
            .chars()
            .rev()
            .take_while(|c| c.is_ascii_digit())
            .collect::<String>()
            .chars()
            .rev()
            .collect::<String>()
            .parse()
            .ok()?;
        let byte_prefix_len = head.len()
            - head
                .chars()
                .rev()
                .take_while(|c| c.is_ascii_digit())
                .count();
        let prefix = &head[..byte_prefix_len];

        let mut out = Vec::with_capacity(points);
        let mut bit = bit_start;
        let mut byte = byte_start;
        for _ in 0..points {
            out.push(format!("{prefix}{byte}.{bit}"));
            bit += 1;
            if bit > 7 {
                bit = 0;
                byte += 1;
            }
        }
        Some(out)
    }

    /// `ELECIO` lists each PLC point with its address and what it is wired to.
    ///
    /// An address on its own is a label; what an electrician needs is "I0.3 goes to the
    /// terminal strip on this sheet". `ELECPLC` records each pin's coordinates in
    /// `X1TERMnn`/`X2TERMnn` (the same idea as the reference products' terminal-position
    /// attributes), so this can follow the wire from the pin and name the devices on it.
    pub(super) fn elec_io(&mut self, i: usize) -> Task<Message> {
        const TOLERANCE: f64 = 0.05;

        let (wires, anchors) = self.elec_collect(i);
        if wires.is_empty() {
            self.command_line
                .push_output("ELECIO: no wires on this drawing, so no point can be wired.");
            return Task::none();
        }

        // Cluster wires by shared endpoints. A PLC pin is a connection point inside the
        // symbol, not the symbol's insertion point, so the anchor-based netlist cannot see
        // it; following the wire geometry finds the conductor the pin actually lands on.
        let near = |a: (f64, f64), b: (f64, f64)| {
            (a.0 - b.0).abs() <= TOLERANCE && (a.1 - b.1).abs() <= TOLERANCE
        };
        let mut cluster: Vec<usize> = (0..wires.len()).collect();
        fn find(parent: &mut Vec<usize>, mut index: usize) -> usize {
            while parent[index] != index {
                parent[index] = parent[parent[index]];
                index = parent[index];
            }
            index
        }
        for a in 0..wires.len() {
            for b in (a + 1)..wires.len() {
                let aw = &wires[a];
                let bw = &wires[b];
                let touches = near((aw.from.x, aw.from.y), (bw.from.x, bw.from.y))
                    || near((aw.from.x, aw.from.y), (bw.to.x, bw.to.y))
                    || near((aw.to.x, aw.to.y), (bw.from.x, bw.from.y))
                    || near((aw.to.x, aw.to.y), (bw.to.x, bw.to.y));
                if touches {
                    let ra = find(&mut cluster, a);
                    let rb = find(&mut cluster, b);
                    if ra != rb {
                        cluster[ra] = rb;
                    }
                }
            }
        }

        // Resolve roots once, so the lookups below only read them.
        let roots: Vec<usize> = (0..wires.len())
            .map(|index| find(&mut cluster, index))
            .collect();

        // Devices sitting on a cluster: an anchor whose position matches a wire endpoint.
        let devices_on = |root: usize| -> Vec<String> {
            let mut names: Vec<String> = Vec::new();
            for (index, wire) in wires.iter().enumerate() {
                if roots[index] != root {
                    continue;
                }
                for anchor in &anchors {
                    let point = (anchor.at.x, anchor.at.y);
                    if near(point, (wire.from.x, wire.from.y)) || near(point, (wire.to.x, wire.to.y)) {
                        if !names.contains(&anchor.label) {
                            names.push(anchor.label.clone());
                        }
                    }
                }
            }
            names
        };

        let net_at = |x: f64, y: f64| -> Option<usize> {
            (0..wires.len()).find(|index| {
                let wire = &wires[*index];
                near((x, y), (wire.from.x, wire.from.y)) || near((x, y), (wire.to.x, wire.to.y))
            })
        };

        let modules: Vec<(String, Vec<(String, Option<(f64, f64)>)>)> = {
            let doc = &self.tabs[i].scene.document;
            let _model_space = doc.header.model_space_block_handle;
            let spaces = Self::drawing_spaces(doc);
            doc.entities()
                .filter(|entity| spaces.contains(&entity.common().owner_handle))
                .filter_map(|entity| match entity {
                    codec::EntityType::Insert(insert)
                        if insert.block_name.to_ascii_uppercase().starts_with("PLC-") =>
                    {
                        let mut tag = String::new();
                        for attribute in &insert.attributes {
                            if is_tag_attribute(&attribute.tag) && tag.is_empty() {
                                tag = attribute.value.trim().to_string();
                            }
                        }
                        let mut points: Vec<(String, Option<(f64, f64)>)> = Vec::new();
                        for attribute in &insert.attributes {
                            let name = attribute.tag.trim().to_ascii_uppercase();
                            if !name.starts_with("ADDR") {
                                continue;
                            }
                            let suffix = &name[4..];
                            let address = attribute.value.trim().to_string();
                            let pin = insert
                                .attributes
                                .iter()
                                .find(|other| {
                                    other
                                        .tag
                                        .trim()
                                        .eq_ignore_ascii_case(&format!("X1TERM{suffix}"))
                                })
                                .and_then(|other| {
                                    let (x, y) = other.value.split_once(',')?;
                                    Some((
                                        x.trim().parse::<f64>().ok()?,
                                        y.trim().parse::<f64>().ok()?,
                                    ))
                                });
                            points.push((address, pin));
                        }
                        Some((tag, points))
                    }
                    _ => None,
                })
                .collect()
        };

        if modules.is_empty() {
            self.command_line
                .push_output("ELECIO: no PLC modules on this drawing (place one with ELECPLC).");
            return Task::none();
        }

        let mut total = 0usize;
        let mut wired = 0usize;
        for (tag, points) in &modules {
            let header = format!("  {tag} ({} point(s)):", points.len());
            self.command_line.push_output(&header);
            for (address, pin) in points {
                total += 1;
                let target = pin.and_then(|(x, y)| net_at(x, y));
                match target {
                    Some(index) => {
                        wired += 1;
                        let root = find(&mut cluster, index);
                        let peers = devices_on(root);
                        let line = if peers.is_empty() {
                            format!("    {address} -> wired, no device on that conductor")
                        } else {
                            format!("    {address} -> {}", peers.join(", "))
                        };
                        self.command_line.push_output(&line);
                    }
                    None => {
                        let line = format!("    {address} -> not wired");
                        self.command_line.push_output(&line);
                    }
                }
            }
        }

        let summary = format!(
            "ELECIO: {} module(s), {total} point(s), {wired} wired.",
            modules.len()
        );
        self.command_line.push_output(&summary);
        Task::none()
    }

    /// `ELECPLCNEXT <tag> <points> [prefix] [x,y]` places a module continuing the project's
    /// I/O sequence.
    ///
    /// On a real drawing set the second sheet continues where the first stopped - `I1.0`
    /// after `I0.7`, not another `I0.0`. That needs the other sheets, so this scans the
    /// project folder `ELECPROJECT` remembered and takes the highest address already used.
    pub(super) fn elec_plc_next(
        &mut self,
        i: usize,
        tag: &str,
        points: usize,
        prefix: &str,
        at: Option<(f64, f64)>,
    ) -> Task<Message> {
        let prefix = if prefix.trim().is_empty() {
            "I"
        } else {
            prefix.trim()
        };

        let mut highest: Option<(usize, usize)> = None;
        let mut note: Option<String> = None;

        // Addresses already used by the project: the open project file if there is one,
        // otherwise the folder the last ELECPROJECT scanned. One source of truth, so the
        // sequence cannot disagree with the reports.
        let project_sheets = Self::project_sheets();
        if !project_sheets.is_empty() {
            let mut scanned = 0usize;
            for path in &project_sheets {
                let Ok(document) = crate::io::load_file(path) else {
                    continue;
                };
                scanned += 1;
                for component in Self::components_of(&document) {
                    for (name, value) in component.attributes.iter() {
                        if !name.trim().to_ascii_uppercase().starts_with("ADDR") {
                            continue;
                        }
                        if let Some(position) = Self::plc_parse_address(value, prefix) {
                            highest = Some(match highest {
                                Some(current) if current >= position => current,
                                _ => position,
                            });
                        }
                    }
                }
            }
            if scanned > 0 {
                note = Some(format!("{scanned} sheet(s) in the project"));
            }
        }

        // Addresses already in this drawing.
        for component in Self::components_of(&self.tabs[i].scene.document) {
            for (name, value) in component.attributes.iter() {
                if !name.trim().to_ascii_uppercase().starts_with("ADDR") {
                    continue;
                }
                if let Some(position) = Self::plc_parse_address(value, prefix) {
                    highest = Some(match highest {
                        Some(current) if current >= position => current,
                        _ => position,
                    });
                }
            }
        }

        let base = match highest {
            Some((byte, bit)) => {
                let (next_byte, next_bit) = if bit >= 7 { (byte + 1, 0) } else { (byte, bit + 1) };
                format!("{prefix}{next_byte}.{next_bit}")
            }
            None => format!("{prefix}0.0"),
        };

        let detail = match note {
            Some(text) => format!("continuing after the highest address in the {text}"),
            None => "starting the sequence (no project folder remembered)".to_string(),
        };
        let message = format!("ELECPLCNEXT: {detail}; base {base}.");
        self.command_line.push_output(&message);
        self.elec_plc(i, tag, points, &base, at)
    }

    /// Parses `I1.3` into `(1, 3)` when the prefix matches.
    fn plc_parse_address(value: &str, prefix: &str) -> Option<(usize, usize)> {
        let value = value.trim();
        let rest = value.strip_prefix(prefix)?;
        let (byte, bit) = rest.split_once('.')?;
        let byte = byte.parse::<usize>().ok()?;
        let bit = bit.parse::<usize>().ok()?;
        if bit > 7 {
            return None;
        }
        Some((byte, bit))
    }

    /// `ELECPANEL <out.dxf>` lays the drawing's devices out as a panel.
    ///
    /// The schematic says what is connected; a panel drawing says what is bolted to the
    /// backplate and in what order. This is the smallest honest version: one box per device
    /// in a grid inside an enclosure, labelled with its tag and class, written to its own
    /// drawing so the schematic is not touched. Real footprints and dimensions come later.

    pub(super) fn elec_panel(&mut self, i: usize, path: &str) -> Task<Message> {
        const EDGE: f64 = 40.0;
        const RAIL_HEIGHT: f64 = 35.0;
        const DOOR_INSET: f64 = 20.0;

        let mut devices: Vec<(String, String)> = Vec::new();
        for component in Self::components_of(&self.tabs[i].scene.document) {
            let tag = component.tag.trim().to_string();
            if tag.is_empty() {
                continue;
            }
            if devices.iter().any(|(existing, _)| existing == &tag) {
                continue;
            }
            devices.push((tag, component.class.clone()));
        }
        if devices.is_empty() {
            self.command_line
                .push_output("ELECPANEL: no tagged devices on this drawing.");
            return Task::none();
        }

        let table = read_panel_footprints();
        let layout = panel_layout(&devices, &table);
        let mut document = codec::Document::new();
        for layer in [
            "PANEL-ENCLOSURE",
            "PANEL-DOOR",
            "PANEL-RAIL",
            "PANEL-DEVICE",
            "PANEL-CUTOUT",
            "PANEL-TEXT",
        ] {
            let _ = document.layers.add(codec::tables::Layer::new(layer));
        }

        let mut add_line = |document: &mut codec::Document,
                            ax: f64,
                            ay: f64,
                            bx: f64,
                            by: f64,
                            layer: &str| {
            let mut line = codec::entities::Line::from_points(
                codec::types::Vector3::new(ax, ay, 0.0),
                codec::types::Vector3::new(bx, by, 0.0),
            );
            line.common.layer = layer.to_string();
            let _ = document.add_entity(codec::EntityType::Line(line));
        };
        let mut add_rect = |document: &mut codec::Document,
                            x: f64,
                            y: f64,
                            width: f64,
                            height: f64,
                            layer: &str| {
            add_line(document, x, y, x + width, y, layer);
            add_line(document, x + width, y, x + width, y + height, layer);
            add_line(document, x + width, y + height, x, y + height, layer);
            add_line(document, x, y + height, x, y, layer);
        };
        let mut add_text = |document: &mut codec::Document,
                            text: &str,
                            x: f64,
                            y: f64,
                            height: f64| {
            let mut entity = codec::entities::Text::new();
            entity.value = text.to_string();
            entity.height = height;
            entity.insertion_point = codec::types::Vector3::new(x, y, 0.0);
            entity.common.layer = "PANEL-TEXT".to_string();
            let _ = document.add_entity(codec::EntityType::Text(entity));
        };

        // Enclosure and door.
        add_rect(&mut document, 0.0, 0.0, layout.width, layout.height, "PANEL-ENCLOSURE");
        add_rect(
            &mut document,
            DOOR_INSET,
            DOOR_INSET,
            layout.width - DOOR_INSET * 2.0,
            layout.height - DOOR_INSET * 2.0,
            "PANEL-DOOR",
        );
        // Door handle, on the right of the door.
        add_rect(
            &mut document,
            layout.width - DOOR_INSET - 12.0,
            layout.height * 0.5 - 8.0,
            6.0,
            16.0,
            "PANEL-DOOR",
        );

        // DIN rails on the mounting plate.
        for rail in 0..layout.rails {
            let y = EDGE + rail as f64 * 110.0;
            add_line(&mut document, EDGE - 10.0, y, layout.width - EDGE + 10.0, y, "PANEL-RAIL");
            add_line(
                &mut document,
                EDGE - 10.0,
                y + RAIL_HEIGHT,
                layout.width - EDGE + 10.0,
                y + RAIL_HEIGHT,
                "PANEL-RAIL",
            );
        }

        // Footprints: a rectangle per device with its tag, class and size.
        for placement in &layout.placements {
            let footprint = placement.footprint;
            add_rect(
                &mut document,
                placement.x,
                placement.y,
                footprint.width,
                footprint.height,
                "PANEL-DEVICE",
            );
            add_text(
                &mut document,
                &placement.tag,
                placement.x + 1.5,
                placement.y + footprint.height * 0.55,
                3.5,
            );
            add_text(
                &mut document,
                &placement.class,
                placement.x + 1.5,
                placement.y + footprint.height * 0.25,
                2.2,
            );
            add_text(
                &mut document,
                &format!("{:.0}x{:.0}", footprint.width, footprint.height),
                placement.x + 1.5,
                placement.y + 2.0,
                1.8,
            );
            if footprint.mount == PanelMount::Door {
                let mut circle = codec::entities::Circle::new();
                circle.center = codec::types::Vector3::new(
                    placement.x + footprint.width * 0.5,
                    placement.y + footprint.height * 0.5,
                    0.0,
                );
                circle.radius = footprint.width.min(footprint.height) * 0.5;
                circle.common.layer = "PANEL-CUTOUT".to_string();
                let _ = document.add_entity(codec::EntityType::Circle(circle));
            }
        }

        add_text(
            &mut document,
            &format!(
                "PANEL LAYOUT - {} device(s) - enclosure {:.0}x{:.0} mm",
                layout.placements.len(),
                layout.width,
                layout.height
            ),
            10.0,
            layout.height + 10.0,
            6.0,
        );

        match crate::io::save(&document, std::path::Path::new(path)) {
            Ok(()) => {
                // Register only once the drawing exists: the check resolves it on disk.
                let registered = register_panel_sheet(path)
                    .map(|text| format!("; {text}"))
                    .unwrap_or_default();
                let summary = format!(
                    "ELECPANEL: {} device(s); enclosure {:.0}x{:.0} mm; {} rail(s), {} door cut-out(s); wrote {path}{registered}.",
                    layout.placements.len(),
                    layout.width,
                    layout.height,
                    layout.rails,
                    layout.door_count
                );
                self.command_line.push_output(&summary);
            }
            Err(error) => {
                let message = format!("ELECPANEL: cannot write {path}: {error}");
                self.command_line.push_output(&message);
            }
        }
        Task::none()
    }


    /// `ELECPROJNEW <folder> [file.eprj]` writes a project file listing the drawings.
    ///
    /// A folder of DXFs is not a project: the order matters (sheet 1 before sheet 2), and
    /// numbering, cross references and reports all need to agree on which sheets belong
    /// together. The file is plain text so it can be read, edited and diffed.
    /// `ELECBUILT [on|off]` reports or sets the project's "as built" flag.
    ///
    /// SkyCAD's built-project control: once a panel is assembled and wired, the references
    /// that exist are physical labels on real hardware. Flagging the project built makes
    /// automatic numbering leave them alone and only number what was added afterwards.
    pub(super) fn elec_built(&mut self, argument: Option<&str>) -> Task<Message> {
        let Some(file) = project_file().read().ok().and_then(|guard| guard.clone()) else {
            self.command_line
                .push_output("ELECBUILT: no project is loaded — ELECPROJNEW or ELECPROJOPEN first.");
            return Task::none();
        };
        let current = project_is_built();
        let Some(argument) = argument.map(str::trim).filter(|value| !value.is_empty()) else {
            let state = if current { "built" } else { "in design" };
            self.command_line.push_output(&format!(
                "ELECBUILT: project is {state} — ELECBUILT ON|OFF changes it."
            ));
            return Task::none();
        };
        let wanted = match argument.to_ascii_uppercase().as_str() {
            "ON" | "YES" | "TRUE" | "1" => true,
            "OFF" | "NO" | "FALSE" | "0" => false,
            _ => {
                self.command_line
                    .push_output("ELECBUILT: use ELECBUILT ON or ELECBUILT OFF.");
                return Task::none();
            }
        };
        let text = match std::fs::read_to_string(&file) {
            Ok(text) => text,
            Err(error) => {
                let message = format!("ELECBUILT: cannot read {}: {error}", file.display());
                self.command_line.push_output(&message);
                return Task::none();
            }
        };
        let mut lines: Vec<String> = text
            .lines()
            .filter(|line| !line.trim_start().to_ascii_lowercase().starts_with("built "))
            .map(|line| line.to_string())
            .collect();
        if wanted {
            lines.push("built yes".to_string());
        }
        let mut output = lines.join("\n");
        output.push('\n');
        match std::fs::write(&file, output) {
            Ok(()) => {
                let state = if wanted { "built" } else { "in design" };
                let message = format!(
                    "ELECBUILT: project marked {state} ({}). Existing references are {}. ",
                    file.display(),
                    if wanted { "now fixed" } else { "free again" }
                );
                self.command_line.push_output(message.trim_end());
            }
            Err(error) => {
                let message = format!("ELECBUILT: cannot write {}: {error}", file.display());
                self.command_line.push_output(&message);
            }
        }
        Task::none()
    }

    pub(super) fn elec_proj_new(&mut self, folder: &str, file: Option<&str>) -> Task<Message> {
        let directory = std::path::Path::new(folder);
        if !directory.is_dir() {
            let message = format!("ELECPROJNEW: {folder} is not a folder.");
            self.command_line.push_output(&message);
            return Task::none();
        }
        let mut sheets: Vec<String> = Vec::new();
        if let Ok(entries) = std::fs::read_dir(directory) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("dxf"))
                {
                    continue;
                }
                if let Some(name) = path.file_name().and_then(|name| name.to_str()) {
                    sheets.push(name.to_string());
                }
            }
        }
        sheets.sort();
        if sheets.is_empty() {
            let message = format!("ELECPROJNEW: no .dxf drawings in {folder}.");
            self.command_line.push_output(&message);
            return Task::none();
        }

        let path = match file {
            Some(path) => std::path::PathBuf::from(path),
            None => directory.join("project.eprj"),
        };
        let mut text = String::from(
            "# AryaCAD electrical project\n# one sheet per line, in drawing order\n",
        );
        for sheet in &sheets {
            text.push_str(&format!("sheet {sheet}\n"));
        }
        match std::fs::write(&path, text) {
            Ok(()) => {
                if let Ok(mut guard) = project_file().write() {
                    *guard = Some(path.clone());
                }
                // A project is a folder as much as a file: remember the folder too, so the
                // project-wide commands (ELECPROJNUM, ELECREV, reports) work immediately
                // instead of answering "no project is loaded".
                if let Ok(mut guard) = project_dir().write() {
                    *guard = Some(directory.to_path_buf());
                }
                let message = format!(
                    "ELECPROJNEW: {} sheet(s) written to {}.",
                    sheets.len(),
                    path.display()
                );
                self.command_line.push_output(&message);
            }
            Err(error) => {
                let message = format!("ELECPROJNEW: cannot write {}: {error}", path.display());
                self.command_line.push_output(&message);
            }
        }
        Task::none()
    }

    /// `ELECPROJOPEN <file.eprj>` opens a project file.
    pub(super) fn elec_proj_open(&mut self, path: &str) -> Task<Message> {
        match Self::read_project(path) {
            Ok(sheets) => {
                if let Ok(mut guard) = project_file().write() {
                    *guard = Some(std::path::PathBuf::from(path));
                }
                if let Some(folder) = std::path::Path::new(path).parent() {
                    if let Ok(mut guard) = project_dir().write() {
                        *guard = Some(folder.to_path_buf());
                    }
                }
                let message = format!(
                    "ELECPROJOPEN: {path} - {} sheet(s).",
                    sheets.len()
                );
                self.command_line.push_output(&message);
                for (index, sheet) in sheets.iter().enumerate().take(10) {
                    let label = if sheet.number.is_empty() {
                        format!("{}", index + 1)
                    } else {
                        sheet.number.clone()
                    };
                    let title = if sheet.title.is_empty() { String::new() } else { format!("  {}", sheet.title) };
                    let line = format!("  {label:>3}  {}{title}", sheet.path.display());
                    self.command_line.push_output(&line);
                }
                if sheets.len() > 10 {
                    let line = format!("  ... and {} more", sheets.len() - 10);
                    self.command_line.push_output(&line);
                }
            }
            Err(error) => {
                let message = format!("ELECPROJOPEN: {error}");
                self.command_line.push_output(&message);
            }
        }
        Task::none()
    }

    /// `ELECPROJLIST` shows the open project's sheets.
    pub(super) fn elec_proj_list(&mut self) -> Task<Message> {
        let Some(path) = project_file().read().ok().and_then(|guard| guard.clone()) else {
            self.command_line
                .push_output("ELECPROJLIST: no project open (ELECPROJNEW / ELECPROJOPEN).");
            return Task::none();
        };
        match Self::read_project(&path.display().to_string()) {
            Ok(sheets) => {
                let message = format!("ELECPROJLIST: {} - {} sheet(s).", path.display(), sheets.len());
                self.command_line.push_output(&message);
                for (index, sheet) in sheets.iter().enumerate().take(20) {
                    let label = if sheet.number.is_empty() {
                        format!("{}", index + 1)
                    } else {
                        sheet.number.clone()
                    };
                    let title = if sheet.title.is_empty() { String::new() } else { format!("  {}", sheet.title) };
                    let line = format!("  {label:>3}  {}{title}", sheet.path.display());
                    self.command_line.push_output(&line);
                }
            }
            Err(error) => {
                let message = format!("ELECPROJLIST: {error}");
                self.command_line.push_output(&message);
            }
        }
        Task::none()
    }

    /// The sheets of the current project.
    ///
    /// The open project file wins; falling back to the last scanned folder keeps the older
    /// `ELECPROJECT <folder>` workflow working. Numbering and reports both go through here,
    /// so they cannot disagree about which sheets belong to the project.
    fn project_sheets() -> Vec<std::path::PathBuf> {
        if let Some(file) = project_file().read().ok().and_then(|guard| guard.clone()) {
            if let Ok(sheets) = Self::read_project(&file.display().to_string()) {
                return sheets.into_iter().map(|sheet| sheet.path).collect();
            }
        }
        let Some(folder) = project_dir().read().ok().and_then(|guard| guard.clone()) else {
            return Vec::new();
        };
        let mut sheets: Vec<std::path::PathBuf> = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&folder) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("dxf"))
                {
                    sheets.push(path);
                }
            }
        }
        sheets.sort();
        sheets
    }

    /// Reads a project file and resolves its sheets to paths.
    fn read_project(path: &str) -> Result<Vec<ProjectSheet>, String> {
        let text = std::fs::read_to_string(path)
            .map_err(|error| format!("cannot read {path}: {error}"))?;
        let base = std::path::Path::new(path)
            .parent()
            .map(|parent| parent.to_path_buf())
            .unwrap_or_default();
        let mut sheets: Vec<ProjectSheet> = Vec::new();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            // `number` and `title` belong to the sheet above them.
            if let Some(value) = line.strip_prefix("number ") {
                if let Some(current) = sheets.last_mut() {
                    current.number = value.trim().to_string();
                }
                continue;
            }
            if let Some(value) = line.strip_prefix("title ") {
                if let Some(current) = sheets.last_mut() {
                    current.title = value.trim().to_string();
                }
                continue;
            }
            let Some(name) = line.strip_prefix("sheet ") else {
                continue;
            };
            let name = name.trim();
            if name.is_empty() {
                continue;
            }
            let candidate = std::path::Path::new(name);
            let resolved = if candidate.is_absolute() {
                candidate.to_path_buf()
            } else {
                base.join(candidate)
            };
            if resolved.is_file() {
                sheets.push(ProjectSheet {
                    path: resolved,
                    number: String::new(),
                    title: String::new(),
                });
            }
        }
        if sheets.is_empty() {
            return Err(format!("{path} lists no sheets that exist on disk"));
        }
        Ok(sheets)
    }

    /// `ELECPROJNUM [apply]` numbers devices across the whole project, sheet by sheet.
    ///
    /// The per-drawing `ELECNUMBER` cannot do this: a device with parts on two sheets must
    /// get one number, and the sequence must stay continuous across the set. This walks the
    /// project in order, assigns one number per device, and (only with `apply`) writes the
    /// tags back, copying each sheet to `.bak` first.
    pub(super) fn elec_proj_number(&mut self, dry_run: bool) -> Task<Message> {
        // A built project has already been assembled and wired: its references are labels
        // on real hardware. Refuse to renumber it until the flag is cleared.
        if project_is_built() && !dry_run {
            self.command_line.push_output(
                "ELECPROJNUM: the project is flagged built — refusing to renumber it. \
                 ELECBUILT OFF first if this is a design change.",
            );
            return Task::none();
        }
        use ::elec::{default_prefix, infer_tag_prefix, NumberingFormat};

        let sheets = Self::project_sheet_list();
        if sheets.is_empty() {
            self.command_line.push_output(
                "ELECPROJNUM: no project open (ELECPROJNEW/ELECPROJOPEN) and no folder scanned.",
            );
            return Task::none();
        }

        let class_of = |block: &str| -> String {
            library_index()
                .read()
                .ok()
                .and_then(|index| {
                    index
                        .get(&block.to_ascii_uppercase())
                        .map(|entry| entry.class.clone())
                })
                .unwrap_or_else(|| block.to_string())
        };

        // Pass 1: every device, in sheet order then reading order. A tag seen again on a
        // later sheet is the same device and keeps its identity.
        struct Device {
            prefix: String,
            tag: String,
            sheets: Vec<String>,
            /// The drawing's `WD_M` says its references are already fixed.
            frozen: bool,
        }
        let mut devices: Vec<Device> = Vec::new();
        let mut seen: BTreeMap<(String, String), usize> = BTreeMap::new();
        let mut duplicates: Vec<String> = Vec::new();
        let mut tag_start = 1u32;

        for sheet in &sheets {
            let Ok(document) = crate::io::load_file(&sheet.path) else {
                continue;
            };
            let label = if sheet.number.is_empty() {
                sheet
                    .path
                    .file_stem()
                    .and_then(|name| name.to_str())
                    .unwrap_or("?")
                    .to_string()
            } else {
                sheet.number.clone()
            };
            let _model_space = document.header.model_space_block_handle;
            let spaces = Self::drawing_spaces(&document);
            // `TAGMODE=R` freezes the references already on the sheet: a released drawing
            // keeps its tags. `TAG-START` seeds the sequence for anything genuinely new.
            let setup = read_setup(&document);
            let frozen_sheet = setup.tag_mode.trim().eq_ignore_ascii_case("R");
            if setup.present && setup.tag_start > 0 {
                tag_start = tag_start.max(setup.tag_start);
            }
            let mut on_sheet: Vec<(String, String, f64, f64)> = Vec::new();
            for entity in document.entities() {
                if !spaces.contains(&entity.common().owner_handle) {
                    continue;
                }
                let codec::EntityType::Insert(insert) = entity else {
                    continue;
                };
                if insert.attributes.is_empty() || is_setup_block(&insert.block_name) {
                    continue;
                }
                let tag = insert
                    .attributes
                    .iter()
                    .find(|attribute| is_tag_attribute(&attribute.tag))
                    .map(|attribute| attribute.value.trim().to_string())
                    .unwrap_or_default();
                if tag.is_empty() {
                    continue;
                }
                let class = class_of(&insert.block_name);
                let prefix = infer_tag_prefix(&[tag.as_str()]).unwrap_or_else(|| default_prefix(&class));
                on_sheet.push((prefix, tag, insert.insert_point.x, insert.insert_point.y));
            }
            on_sheet.sort_by_key(|(_, _, x, y)| ::elec::reading_order_key(*x, *y));
            for (prefix, tag, _, _) in on_sheet {
                let key = (prefix.clone(), tag.to_ascii_uppercase());
                match seen.get(&key) {
                    Some(index) => {
                        let device: &mut Device = &mut devices[*index];
                        if !device.sheets.contains(&label) {
                            device.sheets.push(label.clone());
                        }
                        device.frozen |= frozen_sheet;
                    }
                    None => {
                        seen.insert(key, devices.len());
                        devices.push(Device {
                            prefix,
                            tag,
                            sheets: vec![label.clone()],
                            frozen: frozen_sheet,
                        });
                    }
                }
            }
        }

        if devices.is_empty() {
            self.command_line
                .push_output("ELECPROJNUM: no tagged devices across the project.");
            return Task::none();
        }

        // Pass 2: assign numbers per prefix, in encounter order.
        //
        // A device on a sheet whose `WD_M` says `TAGMODE=R` keeps its reference: a released
        // drawing must not be silently renumbered. Its number is still treated as taken, so a
        // genuinely new device gets the next free number instead of a duplicate.
        let mut counters: BTreeMap<String, u32> = BTreeMap::new();
        let mut taken: BTreeMap<String, BTreeSet<u32>> = BTreeMap::new();
        for device in &devices {
            if device.frozen {
                if let Some(number) = trailing_number(&device.tag) {
                    taken
                        .entry(device.prefix.clone())
                        .or_default()
                        .insert(number);
                }
            }
        }
        let mut mapping: BTreeMap<String, String> = BTreeMap::new();
        let mut plan: Vec<String> = Vec::new();
        let mut frozen_count = 0usize;
        for device in &devices {
            if device.frozen {
                frozen_count += 1;
                continue;
            }
            let format = NumberingFormat::tag(&device.prefix);
            let taken_for_prefix = taken.get(&device.prefix);
            // `value` is 0-based with a start of 1; TAG-START seeds the first ordinal.
            let ordinal = counters
                .entry(device.prefix.clone())
                .or_insert(tag_start.saturating_sub(1));
            loop {
                let number = *ordinal + 1;
                if taken_for_prefix
                    .map(|set| set.contains(&number))
                    .unwrap_or(false)
                {
                    *ordinal += 1;
                    continue;
                }
                break;
            }
            let value = format.value(*ordinal);
            *ordinal += 1;
            mapping.insert(device.tag.clone(), value.clone());
            if value != device.tag {
                plan.push(format!(
                    "    {} -> {value}  (on {})",
                    device.tag,
                    device.sheets.join(", ")
                ));
            }
            if device.sheets.len() > 1 {
                duplicates.push(device.tag.clone());
            }
        }

        let changed = plan.len();
        let verb = if dry_run { "would renumber" } else { "renumbered" };
        let start_note = if tag_start > 1 {
            format!(" TAG-START {tag_start}.")
        } else {
            String::new()
        };
        let frozen_note = if frozen_count > 0 {
            format!(
                " {frozen_count} device(s) frozen by WD_M (TAGMODE=R); existing references are left alone."
            )
        } else {
            String::new()
        };
        let summary = format!(
            "ELECPROJNUM: {} sheet(s), {} device(s); {verb} {changed}.{start_note}{frozen_note}",
            sheets.len(),
            devices.len()
        );
        self.command_line.push_output(&summary);
        for line in plan.iter().take(40) {
            self.command_line.push_output(line);
        }
        if plan.len() > 40 {
            let line = format!("    ... and {} more", plan.len() - 40);
            self.command_line.push_output(&line);
        }
        if !duplicates.is_empty() {
            let line = format!("  devices spanning sheets: {}", duplicates.join(", "));
            self.command_line.push_output(&line);
        }

        if dry_run || changed == 0 {
            if dry_run {
                self.command_line
                    .push_output("  (preview only; run ELECPROJNUM apply to write)");
            } else if changed == 0 && frozen_count > 0 {
                self.command_line.push_output(
                    "  (nothing to renumber: every device on a frozen sheet already has its reference)",
                );
            }
            return Task::none();
        }

        // Pass 3: write the tags back, one sheet at a time, keeping a .bak beside each.
        let mut written = 0usize;
        let mut failed = 0usize;
        for sheet in &sheets {
            let Ok(mut document) = crate::io::load_file(&sheet.path) else {
                failed += 1;
                continue;
            };
            let _model_space = document.header.model_space_block_handle;
            let spaces = Self::drawing_spaces(&document);
            let handles: Vec<codec::Handle> = document
                .entities()
                .filter(|entity| spaces.contains(&entity.common().owner_handle))
                .filter_map(|entity| match entity {
                    codec::EntityType::Insert(insert)
                        if !insert.attributes.is_empty()
                            && !is_setup_block(&insert.block_name) =>
                    {
                        Some(insert.common.handle)
                    }
                    _ => None,
                })
                .collect();
            let mut touched = 0usize;
            for handle in handles {
                let Some(entity) = document.get_entity_mut(handle) else {
                    continue;
                };
                let codec::EntityType::Insert(insert) = entity else {
                    continue;
                };
                let current = insert
                    .attributes
                    .iter()
                    .find(|attribute| is_tag_attribute(&attribute.tag))
                    .map(|attribute| attribute.value.trim().to_string())
                    .unwrap_or_default();
                let Some(next) = mapping.get(&current) else {
                    continue;
                };
                if next == &current {
                    continue;
                }
                let next = next.clone();
                for attribute in insert.attributes.iter_mut() {
                    if is_tag_attribute(&attribute.tag) {
                        attribute.value = next.clone();
                    }
                }
                touched += 1;
            }
            if touched == 0 {
                continue;
            }
            let backup = sheet.path.with_extension("dxf.bak");
            let _ = std::fs::copy(&sheet.path, &backup);
            match crate::io::save(&document, &sheet.path) {
                Ok(()) => written += touched,
                Err(_) => failed += 1,
            }
        }

        let message = format!(
            "  wrote {written} tag(s) across {} sheet(s); {failed} failed. Sheets are backed up as .dxf.bak; reopen any drawing you have open.",
            sheets.len()
        );
        self.command_line.push_output(&message);
        Task::none()
    }

    /// The project's sheets with their metadata: the open project file, else the folder.
    fn project_sheet_list() -> Vec<ProjectSheet> {
        if let Some(file) = project_file().read().ok().and_then(|guard| guard.clone()) {
            if let Ok(sheets) = Self::read_project(&file.display().to_string()) {
                return sheets;
            }
        }
        Self::project_sheets()
            .into_iter()
            .map(|path| ProjectSheet {
                path,
                number: String::new(),
                title: String::new(),
            })
            .collect()
    }

    pub(super) fn elec_sheet(&mut self, i: usize, size: &str, title: &str) -> Task<Message> {
        let (paper_w, paper_h) = match size.trim().to_ascii_uppercase().as_str() {
            "A3" => (420.0_f64, 297.0_f64),
            "A2" => (594.0, 420.0),
            "A1" => (841.0, 594.0),
            "A0" => (1189.0, 841.0),
            _ => (297.0, 210.0),
        };

        // The frame has to fit the drawing, not the other way round: the imported sheets hold
        // roughly 35x25 units of content, and a frame drawn at 297x210 mm in those units is
        // eight times too big. Scale the paper to the content's own extent.
        let mut min_x = f64::MAX;
        let mut min_y = f64::MAX;
        let mut max_x = f64::MIN;
        let mut max_y = f64::MIN;
        {
            let doc = &self.tabs[i].scene.document;
            let _model_space = doc.header.model_space_block_handle;
            let spaces = Self::drawing_spaces(doc);
            let mut take = |x: f64, y: f64| {
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x);
                max_y = max_y.max(y);
            };
            for entity in doc.entities() {
                if !spaces.contains(&entity.common().owner_handle) {
                    continue;
                }
                // Ignore an existing frame, so running ELECSHEET twice refits rather than
                // measuring its own borders.
                if entity.common().layer.as_str() == "ELEC-FRAME" {
                    continue;
                }
                match entity {
                    codec::EntityType::Line(line) => {
                        take(line.start.x, line.start.y);
                        take(line.end.x, line.end.y);
                    }
                    codec::EntityType::Circle(circle) => {
                        let r = circle.radius.abs();
                        take(circle.center.x - r, circle.center.y - r);
                        take(circle.center.x + r, circle.center.y + r);
                    }
                    codec::EntityType::Arc(arc) => {
                        let r = arc.radius.abs();
                        take(arc.center.x - r, arc.center.y - r);
                        take(arc.center.x + r, arc.center.y + r);
                    }
                    codec::EntityType::Text(text) => {
                        take(text.insertion_point.x, text.insertion_point.y);
                    }
                    codec::EntityType::Insert(insert) => {
                        take(insert.insert_point.x, insert.insert_point.y);
                    }
                    codec::EntityType::LwPolyline(polyline) => {
                        for vertex in &polyline.vertices {
                            take(vertex.location.x, vertex.location.y);
                        }
                    }
                    _ => {}
                }
            }
        }

        let (centre_x, centre_y, scale) = if min_x < max_x && min_y < max_y {
            let width = (max_x - min_x).max(1e-9);
            let height = (max_y - min_y).max(1e-9);
            // 15% breathing room around the content.
            let scale = ((width * 1.15) / paper_w).max((height * 1.15) / paper_h);
            (
                (min_x + max_x) * 0.5,
                (min_y + max_y) * 0.5,
                scale,
            )
        } else {
            // Empty drawing: draw a real sheet at 1:1 around the origin.
            (paper_w * 0.5, paper_h * 0.5, 1.0)
        };

        let frame_w = paper_w * scale;
        let frame_h = paper_h * scale;
        let origin_x = centre_x - frame_w * 0.5;
        let origin_y = centre_y - frame_h * 0.5;
        let mm = |value: f64| value * scale;

        // What the title block says about this sheet: project metadata if this file is in
        // the open project, otherwise whatever the caller supplied.
        let current = self.tabs[i]
            .current_path
            .clone()
            .map(|path| std::path::PathBuf::from(path));
        let mut sheet_number = String::new();
        let mut sheet_title = title.trim().to_string();
        let mut project_name = String::new();
        if let Some(file) = project_file().read().ok().and_then(|guard| guard.clone()) {
            project_name = file
                .file_stem()
                .and_then(|name| name.to_str())
                .unwrap_or("project")
                .to_string();
            if let Ok(sheets) = Self::read_project(&file.display().to_string()) {
                if let Some(current) = &current {
                    for (index, sheet) in sheets.iter().enumerate() {
                        let same = sheet.path.file_name() == current.file_name();
                        if !same {
                            continue;
                        }
                        sheet_number = if sheet.number.is_empty() {
                            (index + 1).to_string()
                        } else {
                            sheet.number.clone()
                        };
                        if sheet_title.is_empty() {
                            sheet_title = sheet.title.clone();
                        }
                        break;
                    }
                }
            }
        }

        let mut lines: Vec<(f64, f64, f64, f64)> = Vec::new();
        let mut texts: Vec<(String, f64, f64, f64)> = Vec::new();

        // Paper edge and inner frame, in drawing units.
        let (left_edge, bottom_edge) = (origin_x, origin_y);
        let (right_edge, top_edge) = (origin_x + frame_w, origin_y + frame_h);
        lines.push((left_edge, bottom_edge, right_edge, bottom_edge));
        lines.push((right_edge, bottom_edge, right_edge, top_edge));
        lines.push((right_edge, top_edge, left_edge, top_edge));
        lines.push((left_edge, top_edge, left_edge, bottom_edge));
        let inset = mm(10.0);
        lines.push((left_edge + inset, bottom_edge + inset, right_edge - inset, bottom_edge + inset));
        lines.push((right_edge - inset, bottom_edge + inset, right_edge - inset, top_edge - inset));
        lines.push((right_edge - inset, top_edge - inset, left_edge + inset, top_edge - inset));
        lines.push((left_edge + inset, top_edge - inset, left_edge + inset, bottom_edge + inset));

        // Title block, bottom right inside the frame.
        let block_w = mm(120.0);
        let block_h = mm(32.0);
        let block_left = right_edge - inset - block_w;
        let block_bottom = bottom_edge + inset;
        lines.push((block_left, block_bottom, block_left + block_w, block_bottom));
        lines.push((block_left + block_w, block_bottom, block_left + block_w, block_bottom + block_h));
        lines.push((block_left + block_w, block_bottom + block_h, block_left, block_bottom + block_h));
        lines.push((block_left, block_bottom + block_h, block_left, block_bottom));
        let row1 = block_bottom + mm(20.0);
        let row2 = block_bottom + mm(12.0);
        let row3 = block_bottom + mm(6.0);
        lines.push((block_left, row1, block_left + block_w, row1));
        lines.push((block_left, row2, block_left + block_w, row2));
        lines.push((block_left, row3, block_left + block_w, row3));
        let split = block_left + block_w * 0.65;
        lines.push((split, block_bottom, split, row3));

        let number_text = if sheet_number.is_empty() {
            "SHEET".to_string()
        } else {
            format!("SHEET {sheet_number}")
        };
        texts.push((number_text, block_left + mm(2.0), block_bottom + mm(23.0), mm(6.0)));
        texts.push((
            if sheet_title.is_empty() {
                "UNTITLED".to_string()
            } else {
                sheet_title.to_uppercase()
            },
            block_left + mm(2.0),
            block_bottom + mm(14.0),
            mm(4.0),
        ));
        texts.push((
            if project_name.is_empty() {
                "ARYACAD ELECTRICAL".to_string()
            } else {
                project_name.to_uppercase()
            },
            block_left + mm(2.0),
            block_bottom + mm(8.0),
            mm(3.0),
        ));
        let size_label = format!(
            "{}  {} x {}",
            size.trim().to_ascii_uppercase(),
            paper_w as i64,
            paper_h as i64
        );
        texts.push((size_label, split + mm(2.0), block_bottom + mm(2.0), mm(2.5)));
        texts.push((
            format!("1:{}", if scale > 0.0 { (1.0 / scale).round() as i64 } else { 1 }),
            block_left + mm(2.0),
            block_bottom + mm(2.0),
            mm(2.5),
        ));

        self.push_undo_snapshot(i, "ELECSHEET");
        for (ax, ay, bx, by) in lines {
            let mut line = codec::entities::Line::from_points(
                codec::types::Vector3::new(ax, ay, 0.0),
                codec::types::Vector3::new(bx, by, 0.0),
            );
            line.common.layer = "ELEC-FRAME".to_string();
            self.tabs[i].scene.add_entity(codec::EntityType::Line(line));
        }
        for (value, x, y, height) in texts {
            let mut text = codec::entities::Text::new();
            text.value = value;
            text.height = height;
            text.insertion_point = codec::types::Vector3::new(x, y, 0.0);
            text.common.layer = "ELEC-FRAME".to_string();
            self.tabs[i].scene.add_entity(codec::EntityType::Text(text));
        }
        self.tabs[i].dirty = true;

        let summary = format!(
            "ELECSHEET: {} fitted to the drawing at 1:{} ({frame_w:.2}x{frame_h:.2} units); title block shows sheet {} / {}.",
            size.trim().to_ascii_uppercase(),
            if scale > 0.0 { (1.0 / scale).round() as i64 } else { 1 },
            if sheet_number.is_empty() { "-" } else { &sheet_number },
            if sheet_title.is_empty() { "-" } else { &sheet_title }
        );
        self.command_line.push_output(&summary);
        Task::none()
    }


    /// Command entry point. Returns `None` when the command is not ours so the shared
    /// dispatcher can try the next family.
    pub(super) fn dispatch_elec(&mut self, cmd: &str, i: usize) -> Option<Task<Message>> {
        // Command families that live in their own modules. Each returns `None` when the
        // command is not one of its own, so the shared dispatcher keeps looking.
        if let Some(task) = self.dispatch_elec_parts(cmd, i) {
            return Some(task);
        }
        if let Some(task) = self.dispatch_elec_ioimport(cmd, i) {
            return Some(task);
        }
        if let Some(task) = self.dispatch_elec_revision(cmd, i) {
            return Some(task);
        }
        if let Some(task) = self.dispatch_elec_cables(cmd, i) {
            return Some(task);
        }
        if let Some(task) = self.dispatch_elec_systems(cmd, i) {
            return Some(task);
        }
        match cmd {
            "ELECBOM" => Some(self.elec_bom(i, None, None)),
            "ELECLIST" => Some(self.elec_list(i)),
            "ELECSETUP" => Some(self.elec_setup(i)),
            "ELECSETUPEDIT" => Some(self.elec_setup_edit(i)),
            "ELECHELP" => Some(self.elec_help()),
            "ELECWIRELABELS" => Some(self.elec_wire_labels(i, None)),
            "ELECBUILT" => Some(self.elec_built(None)),
            "ELECWIRELIST" => Some(self.elec_wirelist(i, None)),
            "ELECHIGHLIGHT" => {
                self.command_line
                    .push_output("ELECHIGHLIGHT: ELECHIGHLIGHT <tag>  (selects every part of one device)");
                Some(Task::none())
            }
            "ELECGOTO" => {
                self.command_line
                    .push_output("ELECGOTO: ELECGOTO <tag>  (centres the view on a device)");
                Some(Task::none())
            }
            "ELECWIRENUMBER" => Some(self.elec_wirenumber(i)),
            "ELECTERMINALS" => Some(self.elec_terminals(i)),
            // The registry used to advertise only `ELECSIDES`, so the typed name failed
            // with "Unknown command"; accept both spellings and keep one implementation.
            "ELECTSIDES" | "ELECSIDES" => Some(self.elec_sides(i)),
            "ELECIO" => Some(self.elec_io(i)),
            // The palette is UI, not a drawing edit: it opens the modal that lists the
            // loaded library, and a click in it runs this same ELECINSERT command.
            "ELECMAKESYMBOL" => {
                self.command_line.push_output("ELECMAKESYMBOL: select geometry, then ELECMAKESYMBOL <name>");
                Some(Task::none())
            }
            "ELECLIBMANAGER" => {
                self.open_library_manager();
                Some(Task::none())
            }
            "ELECNEW" => Some(self.elec_new(i, None)),
            // Reopen the drafting-or-electrical start dialog after it has been
            // dismissed once (the ribbon Help menu also reaches it).
            "ELECSTART" => Some(self.open_start_hub()),
            "ELECWIRE" => Some(self.elec_wire(i)),
            "ELECJUNCTIONS" => Some(self.elec_junctions(i)),
            "ELECATTR" => {
                self.command_line
                    .push_output("ELECATTR: use ELECSETATTR <tag> NAME=value");
                Some(Task::none())
            }
            // A ribbon button dispatches the bare verb, but these commands only exist with
            // an argument, so a click used to answer "Unknown command" — a button that looks
            // broken. Answer with the usage line instead.
            "ELECLIB" | "ELECPART" | "ELECSETATTR" | "ELECPROJNEW" | "ELECPROJOPEN"
            | "ELECPLC" | "ELECPLCNEXT" | "ELECPANEL" => {
                let usage = match cmd {
                    "ELECLIB" => "ELECLIB <folder>",
                    "ELECPART" => "ELECPART <parts.csv>",
                    "ELECSETATTR" => "ELECSETATTR <tag> NAME=value,...",
                    "ELECPROJNEW" => "ELECPROJNEW <folder> [project.eprj]",
                    "ELECPROJOPEN" => "ELECPROJOPEN <project.eprj>",
                    "ELECPLC" => "ELECPLC <tag> <points> <base> [x,y]",
                    "ELECPLCNEXT" => "ELECPLCNEXT <tag> <points> [prefix] [x,y]",
                    "ELECPANEL" => "ELECPANEL <out.dxf>",
                    _ => "",
                };
                self.command_line
                    .push_output(&format!("{cmd}: use {usage}."));
                Some(Task::none())
            }
            "ELECPLACE" => {
                // Click-to-place: the palette routes here so the symbol follows the cursor.
                let loaded = library_index().read().map(|i| !i.is_empty()).unwrap_or(false);
                if !loaded {
                    if let Some(directory) = remembered_library() {
                        let path = directory.display().to_string();
                        let _ = self.elec_lib(i, &path);
                    }
                }
                None
            }
            // Bare ELECINSERT opens the palette; the argument form `ELECINSERT <block>`
            // is handled at the bottom of this match. The ribbon's "Insert Symbol" button
            // sends the bare form, which previously matched nothing at all.
            "ELECPALETTE" | "ELECINSERT" => {
                // The palette is useless without symbols, and the folder is remembered, so
                // open it with the library already loaded when this session has not loaded
                // one yet.
                let loaded = library_index()
                    .read()
                    .map(|index| !index.is_empty())
                    .unwrap_or(false);
                if !loaded {
                    if let Some(directory) = remembered_library() {
                        let path = directory.display().to_string();
                        let _ = self.elec_lib(i, &path);
                    }
                }
                self.active_modal = Some(crate::app::ModalKind::ElectricalLibrary);
                self.reset_modal_geometry();
                Some(Task::none())
            }
            "ELECXREF" => Some(self.elec_xref(i)),
            "ELECXREFWRITE" => Some(self.elec_xref_write(i)),
            "ELECNUMBER" => Some(self.elec_number(i, false)),
            // A separate command, not an argument form: the command dispatcher can run a
            // line twice when it carries a space-separated argument, and a preview that
            // mutates the drawing is worse than no preview at all.
            "ELECNUMBERPREVIEW" => Some(self.elec_number(i, true)),
            _ => {
                if let Some(argument) = cmd.strip_prefix("ELECWIRELIST ") {
                    let path = argument.trim();
                    if !path.is_empty() {
                        return Some(self.elec_wirelist(i, Some(path)));
                    }
                }
                if let Some(argument) = cmd.strip_prefix("ELECHIGHLIGHT ") {
                    let tag = argument.trim();
                    if !tag.is_empty() {
                        return Some(self.elec_highlight(i, tag));
                    }
                }
                if let Some(argument) = cmd.strip_prefix("ELECGOTO ") {
                    let tag = argument.trim();
                    if !tag.is_empty() {
                        return Some(self.elec_goto(i, tag));
                    }
                }
                if let Some(argument) = cmd.strip_prefix("ELECWIRELABELS ") {
                    return Some(self.elec_wire_labels(i, Some(argument.trim())));
                }
                if let Some(argument) = cmd.strip_prefix("ELECSETUP ") {
                    return Some(self.elec_setup_assign(i, argument.trim()));
                }
                if let Some(argument) = cmd.strip_prefix("ELECBUILT ") {
                    return Some(self.elec_built(Some(argument)));
                }
                if let Some(argument) = cmd.strip_prefix("ELECBOM ") {
                    // `ELECBOM [out.csv] [OPTION=<code>]` in either order.
                    let mut path: Option<&str> = None;
                    let mut option: Option<&str> = None;
                    for word in argument.split_whitespace() {
                        if let Some(code) = word
                            .strip_prefix("OPTION=")
                            .or_else(|| word.strip_prefix("option="))
                        {
                            option = Some(code);
                        } else if path.is_none() {
                            path = Some(word);
                        }
                    }
                    if path.is_some() || option.is_some() {
                        return Some(self.elec_bom(i, path, option));
                    }
                }
                if let Some(argument) = cmd.strip_prefix("ELECPART ") {
                    let path = argument.trim();
                    if !path.is_empty() {
                        return Some(self.elec_parts(path));
                    }
                }
                if let Some(argument) = cmd.strip_prefix("ELECREPORT ") {
                    let path = argument.trim();
                    if !path.is_empty() {
                        return Some(self.elec_report(i, path));
                    }
                }
                if let Some(argument) = cmd.strip_prefix("ELECMAKESYMBOL ") {
                    let mut words = Vec::new();
                    let mut class = None;
                    let mut pins = None;
                    for word in argument.split_whitespace() {
                        if let Some(value) = word
                            .strip_prefix("CLASS=")
                            .or_else(|| word.strip_prefix("class="))
                        {
                            class = Some(value);
                        } else if let Some(value) = word
                            .strip_prefix("PINS=")
                            .or_else(|| word.strip_prefix("pins="))
                        {
                            pins = value.parse::<usize>().ok();
                        } else {
                            words.push(word);
                        }
                    }
                    let name = words.join(" ");
                    if !name.is_empty() {
                        return Some(self.elec_make_symbol(i, &name, class, pins));
                    }
                }
                if let Some(argument) = cmd.strip_prefix("ELECSETATTR ") {
                    let mut words = argument.split_whitespace();
                    if let Some(tag) = words.next() {
                        let rest = words.collect::<Vec<_>>().join(" ");
                        return Some(self.elec_set_attr(i, tag, &rest));
                    }
                }
                if let Some(argument) = cmd.strip_prefix("ELECPLACE ") {
                    let name = argument.trim();
                    if !name.is_empty() {
                        return Some(self.elec_place(i, name));
                    }
                }
                if cmd == "ELECNEW" {
                    return Some(self.elec_new(i, None));
                }
                if cmd == "ELECPROJLIST" {
                    return Some(self.elec_proj_list());
                }
                if let Some(argument) = cmd.strip_prefix("ELECPROJOPEN ") {
                    let path = argument.trim();
                    if !path.is_empty() {
                        return Some(self.elec_proj_open(path));
                    }
                }
                if let Some(argument) = cmd.strip_prefix("ELECPROJNEW ") {
                    let mut words = argument.split_whitespace();
                    if let Some(folder) = words.next() {
                        let file = words.next();
                        return Some(self.elec_proj_new(folder, file));
                    }
                }
                if let Some(argument) = cmd.strip_prefix("ELECPANEL ") {
                    let path = argument.trim();
                    if !path.is_empty() {
                        return Some(self.elec_panel(i, path));
                    }
                }
                if let Some(argument) = cmd.strip_prefix("ELECPLCNEXT ") {
                    // ELECPLCNEXT <tag> <points> [prefix] [x,y]
                    let mut words = argument.split_whitespace();
                    let tag = words.next().unwrap_or("");
                    let points = words
                        .next()
                        .and_then(|value| value.parse::<usize>().ok())
                        .unwrap_or(0);
                    let mut prefix = "I";
                    let mut at = None;
                    for word in words {
                        if let Some((x, y)) = word.split_once(',') {
                            at = Some((
                                x.trim().parse::<f64>().unwrap_or(0.0),
                                y.trim().parse::<f64>().unwrap_or(0.0),
                            ));
                        } else if word.len() == 1 {
                            prefix = word;
                        }
                    }
                    return Some(self.elec_plc_next(i, tag, points, prefix, at));
                }
                if let Some(argument) = cmd.strip_prefix("ELECPLC ") {
                    // ELECPLC <tag> <points> <base-address> [x,y]
                    let mut words = argument.split_whitespace();
                    let tag = words.next().unwrap_or("");
                    let points = words
                        .next()
                        .and_then(|value| value.parse::<usize>().ok())
                        .unwrap_or(0);
                    let base = words.next().unwrap_or("");
                    let at = words.next().and_then(|text| {
                        let (x, y) = text.split_once(',')?;
                        Some((
                            x.trim().parse::<f64>().ok()?,
                            y.trim().parse::<f64>().ok()?,
                        ))
                    });
                    return Some(self.elec_plc(i, tag, points, base, at));
                }
                if let Some(argument) = cmd.strip_prefix("ELECSHEET ") {
                    let mut words = argument.split_whitespace();
                    let size = words.next().unwrap_or("A4");
                    let title = argument.trim().trim_start_matches(size).trim();
                    return Some(self.elec_sheet(i, size, title));
                }
                if cmd == "ELECSHEET" {
                    return Some(self.elec_sheet(i, "A4", ""));
                }
                if cmd == "ELECPROJNUM" {
                    return Some(self.elec_proj_number(true));
                }
                if let Some(argument) = cmd.strip_prefix("ELECPROJNUM ") {
                    let apply = argument.trim().eq_ignore_ascii_case("apply");
                    return Some(self.elec_proj_number(!apply));
                }
                if cmd == "ELECPROJECT" {
                    // No folder: read the open project file.
                    return Some(self.elec_project(None, None));
                }
                if let Some(argument) = cmd.strip_prefix("ELECPROJECT ") {
                    // `ELECPROJECT <folder> [out.xlsx]`: the optional path writes the
                    // project BOM, the per-sheet summary and the class breakdown.
                    let mut words = argument.split_whitespace();
                    if let Some(first) = words.next() {
                        let out = words.next();
                        // A single .xlsx argument means "the open project, written here".
                        if out.is_none()
                            && first.to_ascii_lowercase().ends_with(".xlsx")
                            && project_file().read().map(|guard| guard.is_some()).unwrap_or(false)
                        {
                            return Some(self.elec_project(None, Some(first)));
                        }
                        return Some(self.elec_project(Some(first), out));
                    }
                }
                if let Some(argument) = cmd.strip_prefix("ELECNEW ") {
                    let path = argument.trim();
                    if !path.is_empty() {
                        return Some(self.elec_new(i, Some(path)));
                    }
                }
                if let Some(argument) = cmd.strip_prefix("ELECLIB ") {
                    let path = argument.trim();
                    if !path.is_empty() {
                        return Some(self.elec_lib(i, path));
                    }
                }
                if let Some(argument) = cmd.strip_prefix("ELECINSERT ") {
                    let mut words = argument.split_whitespace();
                    if let Some(block) = words.next() {
                        let part = words.next();
                        return Some(self.elec_insert(i, block, part));
                    }
                }
                None
            }
        }
    }
}

// ---------------------------------------------------------------------------------------
// Panel layout: footprints, rails, door cut-outs
// ---------------------------------------------------------------------------------------

/// Where a device mounts on the panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PanelMount {
    Rail,
    Door,
    Plate,
}

impl PanelMount {
    fn parse(text: &str) -> Self {
        match text.trim().to_ascii_lowercase().as_str() {
            "door" => Self::Door,
            "plate" => Self::Plate,
            _ => Self::Rail,
        }
    }

    fn rank(self) -> u8 {
        match self {
            Self::Rail => 0,
            Self::Plate => 1,
            Self::Door => 2,
        }
    }
}

/// The mounting-plate space one device class needs, in millimetres.
#[derive(Debug, Clone, Copy)]
struct PanelFootprint {
    width: f64,
    height: f64,
    mount: PanelMount,
}

/// One device placed on the panel, lower-left corner in plate millimetres.
struct PanelPlacement {
    tag: String,
    class: String,
    footprint: PanelFootprint,
    x: f64,
    y: f64,
}

/// The finished panel: placements, enclosure size and the counts the summary reports.
struct PanelLayout {
    placements: Vec<PanelPlacement>,
    width: f64,
    height: f64,
    rails: usize,
    door_count: usize,
}

/// Read the footprint table: a project's own `footprints.tsv` wins over the library's.
fn read_panel_footprints() -> BTreeMap<String, PanelFootprint> {
    const LIBRARY_TABLE: &str = "panel/footprints.tsv";
    let mut candidates: Vec<std::path::PathBuf> = Vec::new();
    if let Some(folder) = project_dir_for_revisions() {
        candidates.push(folder.join("footprints.tsv"));
    }
    if let Some(library) = library_dir().read().ok().and_then(|guard| guard.clone()) {
        candidates.push(library.join(LIBRARY_TABLE));
        if let Some(parent) = library.parent() {
            candidates.push(parent.join(LIBRARY_TABLE));
        }
    }
    candidates.push(std::path::PathBuf::from("library").join(LIBRARY_TABLE));

    let mut map: BTreeMap<String, PanelFootprint> = BTreeMap::new();
    for candidate in candidates {
        let Ok(text) = std::fs::read_to_string(&candidate) else {
            continue;
        };
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let mut fields = line.split('\t');
            let Some(key) = fields.next() else {
                continue;
            };
            let Some(width) = fields
                .next()
                .and_then(|value| value.trim().parse::<f64>().ok())
            else {
                continue;
            };
            let Some(height) = fields
                .next()
                .and_then(|value| value.trim().parse::<f64>().ok())
            else {
                continue;
            };
            let mount = fields.next().map(PanelMount::parse).unwrap_or(PanelMount::Rail);
            if width <= 0.0 || height <= 0.0 {
                continue;
            }
            let key = key.trim().to_ascii_uppercase();
            let key = key.strip_prefix("CLASS:").unwrap_or(&key).to_string();
            map.insert(
                key,
                PanelFootprint {
                    width,
                    height,
                    mount,
                },
            );
        }
    }
    map
}

fn panel_footprint(class: &str, table: &BTreeMap<String, PanelFootprint>) -> PanelFootprint {
    let key = class.trim().to_ascii_uppercase();
    if let Some(footprint) = table.get(&key) {
        return *footprint;
    }
    // A PLC module's class carries its point count (`PLC-8`, `PLC Digital Input`); the
    // table's generic PLC row is the fallback for the family.
    if key.starts_with("PLC") {
        if let Some(footprint) = table.get("PLC") {
            return *footprint;
        }
    }
    table.get("DEFAULT").copied().unwrap_or(PanelFootprint {
        width: 45.0,
        height: 90.0,
        mount: PanelMount::Rail,
    })
}

/// Place the devices: rail rows first, then plate rows, then the door band.
fn panel_layout(devices: &[(String, String)], table: &BTreeMap<String, PanelFootprint>) -> PanelLayout {
    const GAP: f64 = 5.0;
    const ROW_WIDTH: f64 = 560.0;
    const RAIL_PITCH: f64 = 110.0;
    const PLATE_PITCH: f64 = 70.0;
    const DOOR_BAND: f64 = 80.0;
    const EDGE: f64 = 40.0;

    let mut items: Vec<(String, String, PanelFootprint)> = devices
        .iter()
        .map(|(tag, class)| (tag.clone(), class.clone(), panel_footprint(class, table)))
        .collect();
    items.sort_by(|left, right| {
        left.2
            .mount
            .rank()
            .cmp(&right.2.mount.rank())
            .then(left.1.cmp(&right.1))
            .then(left.0.cmp(&right.0))
    });

    let mut placements: Vec<PanelPlacement> = Vec::new();
    let mut max_row = 0.0f64;

    // Rail devices, placed left to right on 35 mm DIN rails.
    let rail_items: Vec<&(String, String, PanelFootprint)> = items
        .iter()
        .filter(|item| item.2.mount == PanelMount::Rail)
        .collect();
    let mut rails = if rail_items.is_empty() { 0 } else { 1 };
    let mut x = EDGE;
    let mut y = EDGE;
    for (tag, class, footprint) in &rail_items {
        if x + footprint.width > EDGE + ROW_WIDTH && x > EDGE {
            rails += 1;
            x = EDGE;
            y += RAIL_PITCH;
        }
        placements.push(PanelPlacement {
            tag: tag.clone(),
            class: class.clone(),
            footprint: *footprint,
            x,
            y,
        });
        x += footprint.width + GAP;
        max_row = max_row.max(x - GAP - EDGE);
    }

    // Plate devices: screwed straight to the plate, below the rails.
    let plate_items: Vec<&(String, String, PanelFootprint)> = items
        .iter()
        .filter(|item| item.2.mount == PanelMount::Plate)
        .collect();
    let plate_top = EDGE + rails as f64 * RAIL_PITCH + 20.0;
    let mut plate_rows = if plate_items.is_empty() { 0 } else { 1 };
    let mut px = EDGE;
    let mut py = plate_top;
    for (tag, class, footprint) in &plate_items {
        if px + footprint.width > EDGE + ROW_WIDTH && px > EDGE {
            plate_rows += 1;
            px = EDGE;
            py += PLATE_PITCH;
        }
        placements.push(PanelPlacement {
            tag: tag.clone(),
            class: class.clone(),
            footprint: *footprint,
            x: px,
            y: py,
        });
        px += footprint.width + GAP;
        max_row = max_row.max(px - GAP - EDGE);
    }

    // Door devices: along the door band at the top, each with a round cut-out.
    let door_items: Vec<&(String, String, PanelFootprint)> = items
        .iter()
        .filter(|item| item.2.mount == PanelMount::Door)
        .collect();
    let door_count = door_items.len();
    let door_base = EDGE + rails as f64 * RAIL_PITCH + plate_rows as f64 * PLATE_PITCH + 40.0;
    let mut dx = EDGE + 20.0;
    for (tag, class, footprint) in &door_items {
        placements.push(PanelPlacement {
            tag: tag.clone(),
            class: class.clone(),
            footprint: *footprint,
            x: dx,
            y: door_base + 25.0,
        });
        dx += footprint.width.max(30.0) + 20.0;
        max_row = max_row.max(dx - 20.0 - EDGE);
    }

    let width = (max_row + EDGE * 2.0).max(420.0);
    let height = door_base + DOOR_BAND + EDGE;
    PanelLayout {
        placements,
        width,
        height,
        rails,
        door_count,
    }
}

/// Add the panel drawing to the open project file, once. Returns what was done.
fn register_panel_sheet(path: &str) -> Option<String> {
    let file = project_file().read().ok().and_then(|guard| guard.clone())?;
    let folder = file.parent()?.to_path_buf();
    let canonical = std::path::Path::new(path).canonicalize().ok()?;
    let existing = std::fs::read_to_string(&file).unwrap_or_default();
    let already = existing.lines().any(|line| {
        let Some(name) = line.strip_prefix("sheet ") else {
            return false;
        };
        let candidate = std::path::Path::new(name.trim());
        let resolved = if candidate.is_absolute() {
            candidate.to_path_buf()
        } else {
            folder.join(candidate)
        };
        resolved.canonicalize().ok().as_deref() == Some(canonical.as_path())
    });
    if already {
        return Some(format!("already a project sheet in {}", file.display()));
    }
    // The project's own sheets are written relative to it; `canonicalize` returns a
    // verbatim (`\\?\`) path on Windows, which does not compare with the plain folder,
    // so the relative name is computed from the requested path instead.
    let requested = std::path::PathBuf::from(path);
    let entry = requested
        .strip_prefix(&folder)
        .map(|relative| relative.to_path_buf())
        .unwrap_or_else(|_| {
            requested
                .file_name()
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| requested.clone())
        });
    let sheet_count = existing.lines().filter(|line| line.starts_with("sheet ")).count();
    let mut text = existing;
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    text.push_str(&format!(
        "sheet {}\nnumber P{}\ntitle Panel layout\n",
        entry.display(),
        sheet_count + 1
    ));
    std::fs::write(&file, text).ok()?;
    Some(format!("registered in {}", file.display()))
}

/// The trailing number of a reference (`M326` -> 326), or `None` for a structured tag.
fn trailing_number(tag: &str) -> Option<u32> {
    let digits: String = tag
        .chars()
        .rev()
        .take_while(|character| character.is_ascii_digit())
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    if digits.is_empty() {
        None
    } else {
        digits.parse().ok()
    }
}

/// Attribute tags the author already put in the selected geometry, in drawing order.
///
/// `ELECMAKESYMBOL` writes this list into `library.tsv`, so pin declarations such as
/// `X1TERM01` survive the round trip through the library. Duplicates are folded
/// case-insensitively; the four standard component attributes are added by the caller.
fn authored_attribute_tags(entities: &[codec::EntityType]) -> Vec<String> {
    let mut tags: Vec<String> = Vec::new();
    for entity in entities {
        let codec::EntityType::AttributeDefinition(definition) = entity else {
            continue;
        };
        let tag = definition.tag.trim();
        if tag.is_empty() || tags.iter().any(|existing| existing.eq_ignore_ascii_case(tag)) {
            continue;
        }
        tags.push(tag.to_string());
    }
    tags
}

/// True when an attribute carries the device tag.
fn is_tag_attribute(tag: &str) -> bool {
    matches!(
        tag.trim().to_ascii_uppercase().as_str(),
        "TAG" | "TAG1" | "TAG2" | "TAGSTR" | "WORKID"
    )
}

/// Write bill-of-material rows as CSV, using the column names the report templates use.
fn write_bom_csv(path: &str, rows: &[::elec::BomRow]) -> std::io::Result<()> {
    let mut out = String::from("Class name,Part number,Manufacturer,Description,Qty,Where used\n");
    for row in rows {
        out.push_str(&csv_field(&row.class));
        out.push(',');
        out.push_str(&csv_field(row.part.as_deref().unwrap_or("")));
        out.push(',');
        out.push_str(&csv_field(row.manufacturer.as_deref().unwrap_or("")));
        out.push(',');
        out.push_str(&csv_field(row.description.as_deref().unwrap_or("")));
        out.push(',');
        out.push_str(&row.quantity.to_string());
        out.push(',');
        out.push_str(&csv_field(&row.where_used.join(", ")));
        out.push('\n');
    }
    std::fs::write(path, out)
}

/// Quote a CSV field when it contains a delimiter, quote or newline.
fn csv_field(value: &str) -> String {
    if value.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

/// Shorten a string for fixed-width console output.
fn truncate(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        return text.to_string();
    }
    let mut out: String = text.chars().take(width.saturating_sub(3)).collect();
    out.push_str("...");
    out
}

/// True for a drawing-setup block rather than a device.
///
/// AutoCAD Electrical stores the sheet's electrical settings in the `WD_M` block, which
/// carries dozens of attributes. Treating it as a component put a fake `WD_M` line in the
/// bill of material and numbered it as a device on the production sheets before this filter.
fn is_setup_block(block_name: &str) -> bool {
    let name = block_name.trim().to_ascii_uppercase();
    // AutoCAD Electrical keeps its per-drawing settings on the `WD_M` block. Treating
    // *every* `WD_`-prefixed block as setup swallowed third-party symbols that happen to
    // follow that naming, so only the settings block is excluded here; other `WD_*`
    // inserts are still filtered in practice because a component needs attributes.
    name == "WD_M"
}


/// Write sheets as a minimal xlsx workbook.
///
/// The workbook is built directly rather than through a spreadsheet crate: three parts are
/// needed (content types, workbook, worksheets), strings are written inline so there is no
/// shared-string table to maintain, and the `zip` crate already in the tree packages it.
/// Cells are values, not formulas, which is what a report wants.
fn write_xlsx(path: &str, sheets: &[::elec::Sheet]) -> Result<(), String> {
    use std::io::Write;

    let file = std::fs::File::create(path).map_err(|error| error.to_string())?;
    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    let put = |zip: &mut zip::ZipWriter<std::fs::File>, name: &str, data: &str| -> Result<(), String> {
        zip.start_file(name, options)
            .map_err(|error| error.to_string())?;
        zip.write_all(data.as_bytes())
            .map_err(|error| error.to_string())
    };

    let mut content_types = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
         <Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">\
         <Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/>\
         <Default Extension=\"xml\" ContentType=\"application/xml\"/>\
         <Override PartName=\"/xl/workbook.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml\"/>",
    );
    for index in 1..=sheets.len() {
        content_types.push_str(&format!(
            "<Override PartName=\"/xl/worksheets/sheet{index}.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml\"/>"
        ));
    }
    content_types.push_str("</Types>");
    put(&mut zip, "[Content_Types].xml", &content_types)?;

    put(
        &mut zip,
        "_rels/.rels",
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
         <Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">\
         <Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"xl/workbook.xml\"/>\
         </Relationships>",
    )?;

    let mut workbook = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
         <workbook xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" \
         xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"><sheets>",
    );
    for (index, sheet) in sheets.iter().enumerate() {
        workbook.push_str(&format!(
            "<sheet name=\"{}\" sheetId=\"{}\" r:id=\"rId{}\"/>",
            xml_escape(&sheet.name),
            index + 1,
            index + 1
        ));
    }
    workbook.push_str("</sheets></workbook>");
    put(&mut zip, "xl/workbook.xml", &workbook)?;

    let mut workbook_rels = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
         <Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">",
    );
    for index in 1..=sheets.len() {
        workbook_rels.push_str(&format!(
            "<Relationship Id=\"rId{index}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet\" Target=\"worksheets/sheet{index}.xml\"/>"
        ));
    }
    workbook_rels.push_str("</Relationships>");
    put(&mut zip, "xl/_rels/workbook.xml.rels", &workbook_rels)?;

    for (index, sheet) in sheets.iter().enumerate() {
        let mut xml = String::from(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
             <worksheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"><sheetData>",
        );
        for (row_index, row) in sheet.rows.iter().enumerate() {
            let number = row_index + 1;
            xml.push_str(&format!("<row r=\"{number}\">"));
            for (column_index, cell) in row.iter().enumerate() {
                let reference = format!("{}{number}", column_name(column_index));
                match cell {
                    ::elec::Cell::Text(text) => xml.push_str(&format!(
                        "<c r=\"{reference}\" t=\"inlineStr\"><is><t xml:space=\"preserve\">{}</t></is></c>",
                        xml_escape(text)
                    )),
                    ::elec::Cell::Number(value) => {
                        if value.fract() == 0.0 && value.abs() < 1e15 {
                            xml.push_str(&format!("<c r=\"{reference}\"><v>{}</v></c>", *value as i64));
                        } else {
                            xml.push_str(&format!("<c r=\"{reference}\"><v>{value}</v></c>"));
                        }
                    }
                }
            }
            xml.push_str("</row>");
        }
        xml.push_str("</sheetData></worksheet>");
        put(&mut zip, &format!("xl/worksheets/sheet{}.xml", index + 1), &xml)?;
    }

    zip.finish().map_err(|error| error.to_string())?;
    Ok(())
}

/// Spreadsheet column name for a zero-based index: 0 -> A, 25 -> Z, 26 -> AA.
fn column_name(index: usize) -> String {
    let mut name = String::new();
    let mut n = index + 1;
    while n > 0 {
        let remainder = (n - 1) % 26;
        name.insert(0, (b'A' + remainder as u8) as char);
        n = (n - 1) / 26;
    }
    name
}

/// Escape text for XML content and attributes.
fn xml_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            // Control characters are illegal in XML; drop them rather than emit a broken file.
            c if (c as u32) < 0x20 && c != '\t' && c != '\n' && c != '\r' => {}
            other => out.push(other),
        }
    }
    out
}





























#[cfg(test)]
mod elec_make_symbol_tests {
    use super::*;

    fn attribute(tag: &str) -> codec::EntityType {
        codec::EntityType::AttributeDefinition(codec::entities::AttributeDefinition::new(
            tag.to_string(),
            "Attribute".to_string(),
            String::new(),
        ))
    }

    #[test]
    fn authored_attribute_tags_keep_pin_attributes_in_drawing_order() {
        let tags = authored_attribute_tags(&[attribute("TAG1"), attribute("X1TERM01")]);
        assert_eq!(tags, vec!["TAG1".to_string(), "X1TERM01".to_string()]);
    }

    #[test]
    fn authored_attribute_tags_fold_duplicates_and_blank_tags() {
        let tags = authored_attribute_tags(&[attribute("tag1"), attribute("TAG1"), attribute("  ")]);
        assert_eq!(tags, vec!["tag1".to_string()]);
    }
}

#[cfg(test)]
mod library_manifest_tests {
    use super::*;

    #[test]
    fn manifest_round_trips_class_attributes_and_scale() {
        let folder = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("tmp")
            .join("manifest_roundtrip");
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).expect("create test folder");

        let mut index = std::collections::BTreeMap::new();
        index.insert(
            "IO_MODULE".to_string(),
            LibraryEntry {
                name: "IO_MODULE".to_string(),
                class: "IO Module".to_string(),
                attributes: vec!["TAG1".to_string(), "X1TERM01".to_string()],
                label_height: 0.0,
                scale: 25.4,
                attribute_offsets: Vec::new(),
            },
        );
        write_library_manifest(&folder, &index).expect("write manifest");

        let read = read_library_manifest(&folder);
        let entry = read.get("IO_MODULE").expect("symbol survives the round trip");
        assert_eq!(entry.class, "IO Module");
        assert_eq!(entry.attributes, vec!["TAG1", "X1TERM01"]);
        assert!((entry.scale - 25.4).abs() < 1e-9);
        let _ = std::fs::remove_dir_all(&folder);
    }
}

#[cfg(test)]
mod elec_setup_tests {
    use super::*;

    #[test]
    fn a_drawing_without_wd_m_reads_the_setup_properties() {
        let mut document = codec::Document::new();
        document.summary_info.custom_properties = vec![
            ("ELEC_TAGFMT".to_string(), "%N".to_string()),
            ("ELEC_TAGMODE".to_string(), "R".to_string()),
            ("ELEC_TAG_START".to_string(), "7".to_string()),
            ("ELEC_WIREFMT".to_string(), "W%N".to_string()),
            ("ELEC_WIRE_START".to_string(), "42".to_string()),
            ("ELEC_TAGINC".to_string(), "5".to_string()),
            ("ELEC_TAGFALL".to_string(), "No".to_string()),
            ("ELEC_WIREINC".to_string(), "2".to_string()),
            ("ELEC_WIREKIND".to_string(), "Alphabetical".to_string()),
            ("ELEC_WIREEMPTY".to_string(), "Yes".to_string()),
            ("ELEC_TAGUNIQ".to_string(), "Local".to_string()),
        ];
        let setup = read_setup(&document);
        assert_eq!(setup.tag_format, "%N");
        assert_eq!(setup.tag_mode, "R");
        assert_eq!(setup.tag_start, 7);
        assert!(setup.references_are_fixed());
        assert_eq!(setup.wire_format, "W%N");
        assert_eq!(setup.wire_start, 42);
        // The SkyCAD numbering extras round-trip through the same properties.
        assert_eq!(setup.tag_increment, 5);
        assert!(!setup.tag_fallthrough);
        assert_eq!(setup.wire_increment, 2);
        assert!(setup.wire_alphabetical);
        assert!(setup.wire_empty_first);
        assert_eq!(setup.tag_unique, "Local");
        assert!(!setup.uniqueness_is_global());
    }

    #[test]
    fn the_dialog_stores_what_elecnumber_reads() {
        let mut app = OpenCADStudio::new_for_test();
        let _ = app.automation_op(r#"{"op":"new"}"#);
        let i = app.active_tab;
        let _ = app.update(Message::Command("ELECSETUPEDIT".into()));
        assert!(app.elec_setup_dialog.is_some());
        let _ = app.update(Message::ElecSetup(
            crate::ui::window::elec_setup::SetupMsg::TagFormat("%F%N%S".into()),
        ));
        let _ = app.update(Message::ElecSetup(
            crate::ui::window::elec_setup::SetupMsg::TagStart("12".into()),
        ));
        let _ = app.update(Message::ElecSetup(
            crate::ui::window::elec_setup::SetupMsg::WireStart("5".into()),
        ));
        let _ = app.update(Message::ElecSetup(
            crate::ui::window::elec_setup::SetupMsg::TagIncrement("3".into()),
        ));
        let _ = app.update(Message::ElecSetup(
            crate::ui::window::elec_setup::SetupMsg::TagFallthrough("No".into()),
        ));
        let _ = app.update(Message::ElecSetup(
            crate::ui::window::elec_setup::SetupMsg::TagUnique("None".into()),
        ));
        let _ = app.update(Message::ElecSetup(
            crate::ui::window::elec_setup::SetupMsg::WireKind("Alphabetical".into()),
        ));
        let _ = app.update(Message::ElecSetup(
            crate::ui::window::elec_setup::SetupMsg::WireEmptyFirst("Yes".into()),
        ));
        let _ = app.update(Message::ElecSetup(crate::ui::window::elec_setup::SetupMsg::Ok));
        assert!(app.elec_setup_dialog.is_none());
        let setup = read_setup(&app.tabs[i].scene.document);
        assert_eq!(setup.tag_format, "%F%N%S");
        assert_eq!(setup.tag_start, 12);
        assert_eq!(setup.wire_start, 5);
        assert_eq!(setup.tag_increment, 3);
        assert!(!setup.tag_fallthrough);
        assert!(setup.wire_alphabetical);
        assert!(setup.wire_empty_first);
        assert_eq!(setup.tag_unique, "None");
        assert!(app.tabs[i].dirty);
    }

    #[test]
    fn a_zero_increment_is_refused() {
        let mut app = OpenCADStudio::new_for_test();
        let _ = app.automation_op(r#"{"op":"new"}"#);
        let _ = app.update(Message::Command("ELECSETUPEDIT".into()));
        let _ = app.update(Message::ElecSetup(
            crate::ui::window::elec_setup::SetupMsg::WireIncrement("0".into()),
        ));
        let _ = app.update(Message::ElecSetup(crate::ui::window::elec_setup::SetupMsg::Ok));
        let state = app.elec_setup_dialog.as_ref().expect("dialog stays open");
        assert_eq!(
            state.error.as_deref(),
            Some("The wire increment must be 1 or more.")
        );
    }

    #[test]
    fn a_bad_counter_keeps_the_dialog_open_and_says_why() {
        let mut app = OpenCADStudio::new_for_test();
        let _ = app.automation_op(r#"{"op":"new"}"#);
        let _ = app.update(Message::Command("ELECSETUPEDIT".into()));
        let _ = app.update(Message::ElecSetup(
            crate::ui::window::elec_setup::SetupMsg::TagStart("twelve".into()),
        ));
        let _ = app.update(Message::ElecSetup(crate::ui::window::elec_setup::SetupMsg::Ok));
        let state = app.elec_setup_dialog.as_ref().expect("dialog stays open");
        assert_eq!(state.error.as_deref(), Some("Tag start must be a whole number."));
    }
}

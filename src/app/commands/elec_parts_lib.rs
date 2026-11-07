//! Local parts catalogue — the offline replacement for a vendor's online parts library.
//!
//! SkyCAD ships an online parts catalogue. This layer keeps the same idea on the machine:
//! a parts folder next to the drawing project, so a project is self-contained and works
//! with no account and no network. Parts carry the data a bill of material needs (part
//! number, manufacturer, description, class, rating, terminals) and point at the symbol
//! that draws them.
//!
//! A parts library is a folder holding one tab-separated `parts.tsv`; the first row is the
//! header. The canonical columns, in order, are:
//!
//! ```text
//! part_number  manufacturer  description  class  symbol  rating  terminals  footprint  datasheet
//! ```
//!
//! `symbol` names a symbol the loaded symbol library already draws (`ELECLIB`), so a part
//! and its geometry stay two separate pieces of data: the same `FUSE` symbol serves every
//! fuse, and the catalogue says which fuse this is. Only `part_number` is required; a
//! header may spell the columns in any order, and a few obvious synonyms are accepted.
//!
//! Commands, all case-insensitive:
//!
//! * `ELECPARTS [folder]` — load a parts folder (default `library\parts`; the last folder
//!   used is remembered in `%APPDATA%\AryaCAD\elec_parts.txt`, like `ELECLIB` remembers the
//!   symbol library).
//! * `ELECPARTLIST [filter]` — list the loaded catalogue as a table, optionally filtered.
//! * `ELECASSIGN <tag> <part-number>` — write `CAT`/`MFG`/`DESC1` (`RATING1` when the part
//!   has a rating) onto every insert carrying that tag.
//! * `ELECASSIGN <tag>` — print the part currently assigned to that tag.

use super::*;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Canonical `parts.tsv` column names, used in the usage and error messages.
const PART_COLUMNS: [&str; 9] = [
    "part_number",
    "manufacturer",
    "description",
    "class",
    "symbol",
    "rating",
    "terminals",
    "footprint",
    "datasheet",
];

/// Header synonyms for the part-number column.
const PART_ALIASES: &[&str] = &[
    "pn",
    "part number",
    "part",
    "partnumber",
    "part no",
    "cat",
    "catalogue id",
];
/// Header synonyms for the manufacturer column.
const MANUFACTURER_ALIASES: &[&str] = &["manufacturer", "mfg", "mfr", "make", "brand"];
/// Header synonyms for the description column.
const DESCRIPTION_ALIASES: &[&str] = &["description", "desc", "details"];
/// Header synonyms for the class column.
const CLASS_ALIASES: &[&str] = &["class", "category", "part class", "type", "cat"];
/// Header synonyms for the symbol column.
const SYMBOL_ALIASES: &[&str] = &["symbol", "symbol name", "block", "drawing symbol"];
/// Header synonyms for the rating column.
const RATING_ALIASES: &[&str] = &["rating", "ratings", "spec", "specification"];
/// Header synonyms for the terminal-count column.
const TERMINALS_ALIASES: &[&str] = &[
    "terminals",
    "terminal",
    "terminal count",
    "poles",
    "connections",
    "ways",
];
/// Header synonyms for the footprint column.
const FOOTPRINT_ALIASES: &[&str] = &["footprint", "mounting", "package"];
/// Header synonyms for the datasheet column.
const DATASHEET_ALIASES: &[&str] = &[
    "datasheet",
    "datasheet url",
    "datasheet reference",
    "documentation",
    "doc",
];

/// One catalogue row: what a part *is*, next to the symbol that draws it.
#[derive(Debug, Clone, Default)]
struct PartRow {
    /// The order code, as the manufacturer writes it (`LPJ-100SP`).
    part_number: String,
    manufacturer: String,
    description: String,
    /// Bill-of-material class (`Fuse`, `Contactor`, `PLC Digital Input`...).
    class: String,
    /// Symbol name the loaded symbol library draws this part with, if any.
    symbol: String,
    rating: String,
    /// Number of terminals; zero means "not a terminal-count part" (a module, a drive).
    terminals: u32,
    footprint: String,
    datasheet: String,
}

impl PartRow {
    /// True when any displayed field contains `needle` (already lower-case).
    fn matches(&self, needle: &str) -> bool {
        self.part_number.to_ascii_lowercase().contains(needle)
            || self.manufacturer.to_ascii_lowercase().contains(needle)
            || self.description.to_ascii_lowercase().contains(needle)
            || self.class.to_ascii_lowercase().contains(needle)
            || self.rating.to_ascii_lowercase().contains(needle)
            || self.symbol.to_ascii_lowercase().contains(needle)
    }
}

/// A loaded parts catalogue.
#[derive(Debug, Clone, Default)]
struct PartsCatalogue {
    /// The `parts.tsv` actually read, for the "loaded from" line.
    file: PathBuf,
    /// Rows in part-number order, so two loads list the same way.
    rows: Vec<PartRow>,
    /// Upper-case part number to row index.
    by_part: BTreeMap<String, usize>,
}

impl PartsCatalogue {
    /// Look a part number up, ignoring case and padding.
    fn lookup(&self, part: &str) -> Option<&PartRow> {
        self.by_part
            .get(&normalise_part(part))
            .map(|index| &self.rows[*index])
    }

    fn len(&self) -> usize {
        self.rows.len()
    }

    /// Part count per BOM class, alphabetically, for the load summary.
    fn class_counts(&self) -> BTreeMap<String, usize> {
        let mut counts = BTreeMap::new();
        for row in &self.rows {
            let class = if row.class.trim().is_empty() {
                "(no class)"
            } else {
                row.class.trim()
            };
            *counts.entry(class.to_string()).or_insert(0) += 1;
        }
        counts
    }
}

/// Outcome of reading a catalogue: the rows plus the data-quality notes.
struct ParsedCatalogue {
    catalogue: PartsCatalogue,
    warnings: Vec<String>,
}

/// The catalogue loaded for this session, if any.
///
/// A catalogue belongs to the machine, not to one drawing, so it lives beside the app
/// instead of inside a document. It is loaded with `ELECPARTS` and stays until replaced;
/// `ELECASSIGN` writes values from it into the drawing.
static PARTS_CATALOGUE: std::sync::OnceLock<std::sync::RwLock<Option<PartsCatalogue>>> =
    std::sync::OnceLock::new();

fn parts_catalogue() -> &'static std::sync::RwLock<Option<PartsCatalogue>> {
    PARTS_CATALOGUE.get_or_init(|| std::sync::RwLock::new(None))
}

/// Where the last parts folder is remembered between runs.
fn parts_config_path() -> Option<PathBuf> {
    let base = std::env::var_os("APPDATA").map(PathBuf::from)?;
    Some(base.join("AryaCAD").join("elec_parts.txt"))
}

fn remember_parts_folder(directory: &Path) {
    let Some(path) = parts_config_path() else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(path, directory.display().to_string());
}

fn remembered_parts_folder() -> Option<PathBuf> {
    let text = std::fs::read_to_string(parts_config_path()?).ok()?;
    let directory = PathBuf::from(text.trim());
    if directory.is_dir() {
        Some(directory)
    } else {
        None
    }
}

/// The folder a bare `ELECPARTS` looks in when nothing is remembered.
fn default_parts_folder() -> PathBuf {
    PathBuf::from("library").join("parts")
}

/// Strip surrounding quotes from a typed path (`ELECPARTS "D:\parts"`).
fn unquote(value: &str) -> String {
    let trimmed = value.trim();
    let bytes = trimmed.as_bytes();
    if trimmed.len() >= 2 {
        let first = bytes[0];
        let last = bytes[trimmed.len() - 1];
        if (first == b'"' && last == b'"') || (first == b'\'' && last == b'\'') {
            return trimmed[1..trimmed.len() - 1].trim().to_string();
        }
    }
    trimmed.to_string()
}

/// Resolve what `ELECPARTS` was asked to load to (the `parts.tsv` path, its folder).
fn resolve_parts_path(argument: &str) -> Result<(PathBuf, PathBuf), String> {
    if argument.is_empty() {
        if let Some(folder) = remembered_parts_folder() {
            return Ok((folder.join("parts.tsv"), folder));
        }
        let folder = default_parts_folder();
        return Ok((folder.join("parts.tsv"), folder));
    }
    let path = PathBuf::from(argument);
    if path.is_dir() {
        return Ok((path.join("parts.tsv"), path));
    }
    if path.is_file() {
        let folder = path.parent().map(Path::to_path_buf).unwrap_or_default();
        return Ok((path, folder));
    }
    Err(format!("{argument} is not a folder or a parts file"))
}

/// Normalise a header cell so `Part Number`, `part_number` and `part  number` all match.
fn normalise_header(value: &str) -> String {
    value
        .trim()
        .to_ascii_lowercase()
        .replace(['_', '-'], " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Index of the first header cell matching any of `names`.
fn find_column(header: &[String], names: &[&str]) -> Option<usize> {
    header.iter().position(|cell| {
        let cell = cell.as_str();
        names.iter().any(|name| cell == *name)
    })
}

/// Part-number key: trimmed and upper-cased, so lookup ignores case.
fn normalise_part(part: &str) -> String {
    part.trim().to_ascii_uppercase()
}

/// Read `parts.tsv`.
///
/// Blank lines and `#` comment lines are skipped. A duplicate part number keeps the later
/// row, the way appending a correction to a spreadsheet works, and is reported as a warning
/// rather than silently swallowing one of the two rows.
fn parse_parts_tsv(text: &str, file: PathBuf) -> Result<ParsedCatalogue, String> {
    // Excel writes a byte-order mark; left in place it would hide the first header cell.
    let text = text.trim_start_matches('\u{feff}');
    let mut warnings: Vec<String> = Vec::new();

    // Collect the numbered, non-blank, non-comment lines so warnings can name their line.
    let mut lines: Vec<(usize, &str)> = Vec::new();
    for (index, raw) in text.lines().enumerate() {
        let line = raw.trim_end_matches('\r');
        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            continue;
        }
        lines.push((index + 1, line));
    }
    let Some((header_line, header_text)) = lines.first().copied() else {
        return Err("the file has no header row".to_string());
    };
    let header: Vec<String> = header_text.split('\t').map(normalise_header).collect();

    let Some(part_column) = find_column(&header, PART_ALIASES) else {
        return Err(format!(
            "line {header_line} is not a header with a part-number column; expected one of {} (found {})",
            PART_ALIASES.join(", "),
            if header.is_empty() {
                "an empty row".to_string()
            } else {
                header.join(", ")
            }
        ));
    };
    let manufacturer_column = find_column(&header, MANUFACTURER_ALIASES);
    let description_column = find_column(&header, DESCRIPTION_ALIASES);
    let class_column = find_column(&header, CLASS_ALIASES);
    let symbol_column = find_column(&header, SYMBOL_ALIASES);
    let rating_column = find_column(&header, RATING_ALIASES);
    let terminals_column = find_column(&header, TERMINALS_ALIASES);
    let footprint_column = find_column(&header, FOOTPRINT_ALIASES);
    let datasheet_column = find_column(&header, DATASHEET_ALIASES);

    let mut rows: Vec<PartRow> = Vec::new();
    let mut by_part: BTreeMap<String, usize> = BTreeMap::new();
    for (number, line) in lines.iter().skip(1) {
        let cells: Vec<&str> = line.split('\t').collect();
        let cell = |column: Option<usize>| -> &str {
            column
                .and_then(|index| cells.get(index))
                .map(|value| value.trim())
                .unwrap_or("")
        };
        let part_number = cell(Some(part_column)).to_string();
        if part_number.is_empty() {
            warnings.push(format!("line {number}: no part number — row skipped"));
            continue;
        }
        let terminals_text = cell(terminals_column);
        let terminals = if terminals_text.is_empty() {
            0
        } else {
            match terminals_text.parse::<u32>() {
                Ok(value) => value,
                Err(_) => {
                    warnings.push(format!(
                        "line {number}: terminals \"{terminals_text}\" is not a count — using 0"
                    ));
                    0
                }
            }
        };
        let row = PartRow {
            part_number,
            manufacturer: cell(manufacturer_column).to_string(),
            description: cell(description_column).to_string(),
            class: cell(class_column).to_string(),
            symbol: cell(symbol_column).to_string(),
            rating: cell(rating_column).to_string(),
            terminals,
            footprint: cell(footprint_column).to_string(),
            datasheet: cell(datasheet_column).to_string(),
        };
        let key = normalise_part(&row.part_number);
        if let Some(index) = by_part.get(&key).copied() {
            warnings.push(format!(
                "line {number}: part {} appears earlier — the later row wins",
                row.part_number
            ));
            rows[index] = row;
        } else {
            by_part.insert(key, rows.len());
            rows.push(row);
        }
    }
    if rows.is_empty() {
        return Err("the file has a header but no part rows".to_string());
    }

    // Present the catalogue in part-number order so two loads of the same data agree.
    rows.sort_by(|left, right| {
        left.part_number
            .to_ascii_uppercase()
            .cmp(&right.part_number.to_ascii_uppercase())
    });
    let by_part: BTreeMap<String, usize> = rows
        .iter()
        .enumerate()
        .map(|(index, row)| (normalise_part(&row.part_number), index))
        .collect();

    Ok(ParsedCatalogue {
        catalogue: PartsCatalogue {
            file,
            rows,
            by_part,
        },
        warnings,
    })
}

/// Clip a cell to `width` characters so columns in the listing stay aligned.
fn clip(value: &str, width: usize) -> String {
    let value = value.trim();
    if value.chars().count() <= width {
        return value.to_string();
    }
    let mut out: String = value.chars().take(width.saturating_sub(2)).collect();
    out.push_str("..");
    out
}

/// True when an attribute name carries the device tag.
fn is_tag_attribute(tag: &str) -> bool {
    matches!(
        tag.trim().to_ascii_uppercase().as_str(),
        "TAG" | "TAG1" | "TAG2" | "TAGSTR" | "WORKID"
    )
}

/// True when this insert carries `tag` in one of its tag attributes.
fn insert_carries_tag(insert: &codec::entities::Insert, tag: &str) -> bool {
    insert.attributes.iter().any(|attribute| {
        is_tag_attribute(&attribute.tag) && attribute.value.trim().eq_ignore_ascii_case(tag)
    })
}

/// First non-empty value among the named attributes.
fn attribute_value(insert: &codec::entities::Insert, names: &[&str]) -> Option<String> {
    insert
        .attributes
        .iter()
        .find(|attribute| {
            let tag = attribute.tag.trim().to_ascii_uppercase();
            names.iter().any(|name| tag == *name)
        })
        .map(|attribute| attribute.value.trim().to_string())
        .filter(|value| !value.is_empty())
}

impl OpenCADStudio {
    /// Dispatch the local-parts-library commands.
    ///
    /// Returns `None` for anything that is not one of ours so the shared dispatcher keeps
    /// looking; `elec::dispatch_elec` calls this before its own match.
    pub(super) fn dispatch_elec_parts(&mut self, cmd: &str, i: usize) -> Option<Task<Message>> {
        let command = cmd.trim();
        let upper = command.to_ascii_uppercase();
        // `ELECPART` is a different, older command (a plain CSV catalogue); compare whole
        // words so `ELECPARTS` never swallows it and `ELECPARTLIST` never swallows both.
        let argument_of = |verb: &str| -> Option<String> {
            if upper == verb {
                Some(String::new())
            } else if upper.starts_with(&format!("{verb} ")) {
                Some(command[verb.len()..].trim().to_string())
            } else {
                None
            }
        };
        if let Some(argument) = argument_of("ELECPARTS") {
            return Some(self.elec_parts_load(&argument));
        }
        if let Some(argument) = argument_of("ELECPARTLIST") {
            return Some(self.elec_parts_list(&argument));
        }
        if let Some(argument) = argument_of("ELECASSIGN") {
            return Some(self.elec_parts_assign(i, &argument));
        }
        None
    }

    /// `ELECPARTS [folder]` loads a local parts library.
    fn elec_parts_load(&mut self, argument: &str) -> Task<Message> {
        let argument = unquote(argument);
        let (file, folder) = match resolve_parts_path(&argument) {
            Ok(resolved) => resolved,
            Err(message) => {
                self.command_line
                    .push_output(&format!("ELECPARTS: {message}."));
                return Task::none();
            }
        };
        let text = match std::fs::read_to_string(&file) {
            Ok(text) => text,
            Err(error) => {
                let message = if argument.is_empty() {
                    format!(
                        "ELECPARTS: no parts catalogue at {} ({error}) — run ELECPARTS <folder> once to remember one.",
                        file.display()
                    )
                } else {
                    format!("ELECPARTS: cannot read {}: {error}", file.display())
                };
                self.command_line.push_output(&message);
                return Task::none();
            }
        };
        let parsed = match parse_parts_tsv(&text, file.clone()) {
            Ok(parsed) => parsed,
            Err(message) => {
                let message = format!(
                    "ELECPARTS: {}: {message} (columns: {})",
                    file.display(),
                    PART_COLUMNS.join(", ")
                );
                self.command_line.push_output(&message);
                return Task::none();
            }
        };

        let count = parsed.catalogue.len();
        let class_counts = parsed.catalogue.class_counts();
        let classes: Vec<String> = class_counts
            .iter()
            .map(|(class, number)| format!("{class} {number}"))
            .collect();
        let with_symbol = parsed
            .catalogue
            .rows
            .iter()
            .filter(|row| !row.symbol.trim().is_empty())
            .count();
        let without_symbol = count - with_symbol;
        let with_terminals = parsed
            .catalogue
            .rows
            .iter()
            .filter(|row| row.terminals > 0)
            .count();
        let with_footprint = parsed
            .catalogue
            .rows
            .iter()
            .filter(|row| !row.footprint.trim().is_empty())
            .count();
        let with_datasheet = parsed
            .catalogue
            .rows
            .iter()
            .filter(|row| !row.datasheet.trim().is_empty())
            .count();
        let warnings = parsed.warnings;
        if let Ok(mut guard) = parts_catalogue().write() {
            *guard = Some(parsed.catalogue);
        }
        remember_parts_folder(&folder);

        self.command_line.push_output(&format!(
            "ELECPARTS: loaded {count} part(s) from {}",
            file.display()
        ));
        self.command_line
            .push_output(&format!("  {} class(es): {}", class_counts.len(), classes.join(", ")));
        self.command_line.push_output(&format!(
            "  {with_symbol} name a symbol, {without_symbol} do not; {with_terminals} carry a terminal count, {with_footprint} a footprint, {with_datasheet} a datasheet reference."
        ));
        for warning in warnings.iter().take(5) {
            self.command_line
                .push_output(&format!("  warning: {warning}"));
        }
        if warnings.len() > 5 {
            self.command_line.push_output(&format!(
                "  ... {} more warning(s)",
                warnings.len() - 5
            ));
        }
        Task::none()
    }

    /// Lazily load the remembered catalogue (or `library\parts`) for a read command.
    ///
    /// `ELECLIB` is auto-loaded by `ELECPLACE` from the remembered folder; a parts library
    /// the user has loaded once should come back the same way instead of being remembered
    /// and then ignored.
    fn ensure_parts_catalogue(&mut self) -> bool {
        if let Ok(guard) = parts_catalogue().read() {
            if guard.is_some() {
                return true;
            }
        }
        let Ok((file, folder)) = resolve_parts_path("") else {
            return false;
        };
        let Ok(text) = std::fs::read_to_string(&file) else {
            return false;
        };
        let Ok(parsed) = parse_parts_tsv(&text, file.clone()) else {
            return false;
        };
        let count = parsed.catalogue.len();
        if let Ok(mut guard) = parts_catalogue().write() {
            *guard = Some(parsed.catalogue);
        }
        remember_parts_folder(&folder);
        self.command_line
            .push_output(&format!("  parts catalogue: {count} part(s) from {}", file.display()));
        true
    }

    /// `ELECPARTLIST [filter]` prints the catalogue as a table.
    fn elec_parts_list(&mut self, filter: &str) -> Task<Message> {
        if !self.ensure_parts_catalogue() {
            self.command_line.push_output(
                "ELECPARTLIST: no parts catalogue loaded — ELECPARTS <folder> first.",
            );
            return Task::none();
        }
        let needle = filter.trim().to_ascii_lowercase();
        let guard = match parts_catalogue().read() {
            Ok(guard) => guard,
            Err(_) => return Task::none(),
        };
        let Some(catalogue) = guard.as_ref() else {
            return Task::none();
        };
        let rows: Vec<&PartRow> = catalogue
            .rows
            .iter()
            .filter(|row| needle.is_empty() || row.matches(&needle))
            .collect();
        if needle.is_empty() {
            self.command_line.push_output(&format!(
                "ELECPARTLIST: {} part(s) from {}",
                catalogue.len(),
                catalogue.file.display()
            ));
        } else if rows.is_empty() {
            self.command_line.push_output(&format!(
                "ELECPARTLIST: no part matches \"{}\" ({} loaded).",
                filter.trim(),
                catalogue.len()
            ));
            return Task::none();
        } else {
            self.command_line.push_output(&format!(
                "ELECPARTLIST: {} of {} part(s) match \"{}\".",
                rows.len(),
                catalogue.len(),
                filter.trim()
            ));
        }
        self.command_line.push_output(&format!(
            "{:<22} {:<18} {:<20} {:<46} {}",
            "PART NUMBER", "MANUFACTURER", "CLASS", "DESCRIPTION", "RATING"
        ));
        for row in rows {
            self.command_line.push_output(&format!(
                "{:<22} {:<18} {:<20} {:<46} {}",
                clip(&row.part_number, 22),
                clip(&row.manufacturer, 18),
                clip(&row.class, 20),
                clip(&row.description, 46),
                clip(&row.rating, 22),
            ));
        }
        Task::none()
    }

    /// `ELECASSIGN <tag> [<part-number>]`: read or write one device's catalogue values.
    fn elec_parts_assign(&mut self, i: usize, arguments: &str) -> Task<Message> {
        let mut words = arguments.split_whitespace();
        let Some(tag) = words.next() else {
            self.command_line.push_output(
                "ELECASSIGN: use ELECASSIGN <tag> <part-number> (or ELECASSIGN <tag> to read it).",
            );
            return Task::none();
        };
        // A part number may contain spaces (`USLKG 5`), so everything after the tag is one.
        let part = words.collect::<Vec<_>>().join(" ");
        if part.is_empty() {
            self.elec_parts_show(i, tag);
        } else {
            self.elec_parts_write(i, tag, &part);
        }
        Task::none()
    }

    /// `ELECASSIGN <tag>`: print the part currently assigned to that tag.
    fn elec_parts_show(&mut self, i: usize, tag: &str) {
        let mut parts: Vec<String> = Vec::new();
        let mut inserts = 0usize;
        {
            let document = &self.tabs[i].scene.document;
            for entity in document.entities() {
                let codec::EntityType::Insert(insert) = entity else {
                    continue;
                };
                if !insert_carries_tag(insert, tag) {
                    continue;
                }
                inserts += 1;
                if let Some(part) = attribute_value(
                    insert,
                    &["CAT", "PART", "PARTNUMBER", "CATALOGUEID", "CATALOG"],
                ) {
                    if !parts.iter().any(|known| known.eq_ignore_ascii_case(&part)) {
                        parts.push(part);
                    }
                }
            }
        }
        if inserts == 0 {
            self.report_no_insert(i, tag, "ELECASSIGN");
            return;
        }
        self.ensure_parts_catalogue();
        if parts.is_empty() {
            self.command_line.push_output(&format!(
                "ELECASSIGN: {tag} has no part assigned yet (CAT is empty on {inserts} insert(s))."
            ));
            return;
        }
        let guard = match parts_catalogue().read() {
            Ok(guard) => guard,
            Err(_) => return,
        };
        let catalogue = guard.as_ref();
        for part in &parts {
            match catalogue.and_then(|catalogue| catalogue.lookup(part)) {
                Some(row) => self.command_line.push_output(&format!(
                    "ELECASSIGN: {tag} -> {}  {}  {}",
                    row.part_number, row.manufacturer, row.description
                )),
                None => self.command_line.push_output(&format!(
                    "ELECASSIGN: {tag} -> {part} (not in the loaded parts catalogue)"
                )),
            }
        }
        if inserts > 1 {
            self.command_line.push_output(&format!(
                "  carried by {inserts} insert(s) sharing the tag"
            ));
        }
    }

    /// `ELECASSIGN <tag> <part-number>`: write the part's fields onto the device.
    ///
    /// The values go on the insert's attributes, which is where `ELECLIST`, `ELECBOM` and
    /// the report templates read a device from — a catalogue lookup that only lived in
    /// memory would vanish the moment the drawing was saved. A field the catalogue leaves
    /// empty is not written, so a partly-described part cannot blank a value the drawing
    /// already carries.
    fn elec_parts_write(&mut self, i: usize, tag: &str, part: &str) {
        if !self.ensure_parts_catalogue() {
            self.command_line.push_output(
                "ELECASSIGN: no parts catalogue loaded — ELECPARTS <folder> first.",
            );
            return;
        }
        let row = {
            let guard = match parts_catalogue().read() {
                Ok(guard) => guard,
                Err(_) => return,
            };
            match guard
                .as_ref()
                .and_then(|catalogue| catalogue.lookup(part))
                .cloned()
            {
                Some(row) => row,
                None => {
                    let loaded = guard.as_ref().map(|catalogue| catalogue.len()).unwrap_or(0);
                    self.command_line.push_output(&format!(
                        "ELECASSIGN: no part \"{part}\" in the parts catalogue ({loaded} loaded) — ELECPARTLIST to search."
                    ));
                    return;
                }
            }
        };

        let targets: Vec<codec::Handle> = {
            let document = &self.tabs[i].scene.document;
            document
                .entities()
                .filter_map(|entity| match entity {
                    codec::EntityType::Insert(insert) if insert_carries_tag(insert, tag) => {
                        Some(insert.common.handle)
                    }
                    _ => None,
                })
                .collect()
        };
        if targets.is_empty() {
            self.report_no_insert(i, tag, "ELECASSIGN");
            return;
        }

        let mut assignments: Vec<(&str, &str)> = vec![
            ("CAT", row.part_number.as_str()),
            ("MFG", row.manufacturer.as_str()),
            ("DESC1", row.description.as_str()),
        ];
        if !row.rating.trim().is_empty() {
            assignments.push(("RATING1", row.rating.as_str()));
        }

        self.push_undo_snapshot(i, "ELECASSIGN");
        let mut changed = 0usize;
        let mut added = 0usize;
        let mut unchanged = 0usize;
        for handle in &targets {
            let document = &mut self.tabs[i].scene.document;
            let Some(codec::EntityType::Insert(insert)) = document.get_entity_mut(*handle) else {
                continue;
            };
            for (name, value) in &assignments {
                if value.is_empty() {
                    continue;
                }
                let existing = insert
                    .attributes
                    .iter()
                    .position(|attribute| attribute.tag.trim().eq_ignore_ascii_case(name));
                match existing {
                    Some(index) => {
                        let attribute = &mut insert.attributes[index];
                        if attribute.value != *value {
                            attribute.value = (*value).to_string();
                            changed += 1;
                        } else {
                            unchanged += 1;
                        }
                    }
                    None => {
                        // The symbol did not carry this attribute. Add it below the last
                        // one, at the text height this symbol's own attributes use, so the
                        // value is visible and stays inside the symbol's scale.
                        let height = insert
                            .attributes
                            .iter()
                            .map(|attribute| attribute.height)
                            .find(|height| *height > 0.0)
                            .unwrap_or(0.125);
                        let lowest = insert
                            .attributes
                            .iter()
                            .map(|attribute| attribute.insertion_point.y)
                            .fold(insert.insert_point.y, f64::min);
                        let mut attribute = codec::entities::AttributeEntity::new(
                            (*name).to_string(),
                            (*value).to_string(),
                        );
                        attribute.height = height;
                        attribute.insertion_point = codec::types::Vector3::new(
                            insert.insert_point.x,
                            lowest - height * 1.25,
                            0.0,
                        );
                        attribute.alignment_point = attribute.insertion_point;
                        insert.attributes.push(attribute);
                        added += 1;
                    }
                }
            }
        }
        self.tabs[i].dirty = true;

        let names: Vec<&str> = assignments.iter().map(|(name, _)| *name).collect();
        self.command_line.push_output(&format!(
            "ELECASSIGN: {tag} <- {}  {}  {}",
            row.part_number, row.manufacturer, row.description
        ));
        self.command_line.push_output(&format!(
            "  set {} on {} insert(s): {changed} value(s) changed, {added} attribute(s) added, {unchanged} already correct.",
            names.join(", "),
            targets.len()
        ));
    }

    /// Say why a tag could not be written, distinguishing a lost symbol from a lost tag.
    ///
    /// On a sheet that was exploded before it reached us the symbol inserts are gone and
    /// the reference designator survives only as plain `TEXT` (round 2's finding on the
    /// customer sheets). Such a device still shows up in `ELECLIST`, but it has no
    /// attributes to write, so the honest answer names that reason instead of claiming the
    /// device does not exist.
    fn report_no_insert(&mut self, i: usize, tag: &str, verb: &str) {
        let text_only = {
            let document = &self.tabs[i].scene.document;
            document.entities().any(|entity| match entity {
                codec::EntityType::Text(text) => text.value.trim().eq_ignore_ascii_case(tag),
                _ => false,
            })
        };
        if text_only {
            self.command_line.push_output(&format!(
                "{verb}: {tag} is plain TEXT on this drawing, not a block insert — there are no attributes to write."
            ));
            self.command_line.push_output(
                "  Re-place the symbol (ELECINSERT) to give the tag attributes, or edit the drawing's part list directly.",
            );
        } else {
            self.command_line.push_output(&format!(
                "{verb}: no component tagged {tag} on this drawing."
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> PartsCatalogue {
        parse_parts_tsv(text, PathBuf::from("parts.tsv")).expect("catalogue parses").catalogue
    }

    const HEADER: &str = "part_number\tmanufacturer\tdescription\tclass\tsymbol\trating\tterminals\tfootprint\tdatasheet\n";

    #[test]
    fn reads_the_canonical_columns() {
        let text = format!(
            "{HEADER}LPJ-100SP\tBussmann\tFuse, Class J\tFuse\tFUSE\t100 A / 600 V\t2\tClass J holder\t\n"
        );
        let catalogue = parse(&text);
        assert_eq!(catalogue.len(), 1);
        let row = catalogue.lookup("lpj-100sp").expect("case-insensitive lookup");
        assert_eq!(row.manufacturer, "Bussmann");
        assert_eq!(row.class, "Fuse");
        assert_eq!(row.symbol, "FUSE");
        assert_eq!(row.terminals, 2);
        assert_eq!(row.footprint, "Class J holder");
    }

    #[test]
    fn header_aliases_are_accepted_in_any_order() {
        let text = "PN\tMfr\tCat\tDesc\tSpec\nX1\tACME\tFuse\tA fuse\t10 A\n";
        let catalogue = parse(text);
        let row = catalogue.lookup("X1").expect("row");
        assert_eq!(row.manufacturer, "ACME");
        assert_eq!(row.class, "Fuse");
        assert_eq!(row.description, "A fuse");
        assert_eq!(row.rating, "10 A");
    }

    #[test]
    fn a_duplicate_part_number_keeps_the_later_row() {
        let text = format!("{HEADER}A1\tOld\tfirst\tFuse\t\t\t0\t\t\nA1\tNew\tsecond\tFuse\t\t\t0\t\t\n");
        let catalogue = parse(&text);
        assert_eq!(catalogue.len(), 1);
        assert_eq!(catalogue.lookup("A1").unwrap().manufacturer, "New");
    }

    #[test]
    fn rows_without_a_part_number_are_skipped_and_reported() {
        let text = format!("{HEADER}\tNobody\tno part\tFuse\t\t\t0\t\t\nA1\tACME\tok\tFuse\t\t\t0\t\t\n");
        let parsed = parse_parts_tsv(&text, PathBuf::from("parts.tsv")).expect("parses");
        assert_eq!(parsed.catalogue.len(), 1);
        assert_eq!(parsed.warnings.len(), 1);
    }

    #[test]
    fn a_missing_part_column_is_an_error() {
        let text = "manufacturer\tdescription\nACME\tThing\n";
        assert!(parse_parts_tsv(text, PathBuf::from("x")).is_err());
    }

    #[test]
    fn listing_order_is_by_part_number_and_filter_reads_every_field() {
        let text = format!(
            "{HEADER}B2\tSiemens\tContactor\tContactor\tCONTACT_NO\t9 A\t6\t\t\nA1\tBussmann\tFuse\tFuse\tFUSE\t30 A\t2\t\t\n"
        );
        let catalogue = parse(&text);
        assert_eq!(catalogue.rows[0].part_number, "A1");
        assert!(catalogue.rows[0].matches("bussmann"));
        assert!(catalogue.rows[0].matches("fuse"));
        assert!(catalogue.rows[1].matches("contactor"));
        assert!(!catalogue.rows[1].matches("bussmann"));
    }
}

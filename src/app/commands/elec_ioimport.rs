//! Import an I/O list (Excel) and generate the PLC drawings it describes.
//!
//! This is the "automated PLC drawings from an I/O list" feature: a worksheet lists the I/O
//! points of a controller, and the command turns those rows into PLC module blocks on the
//! sheet, each carrying its address and description as attributes - the same representation
//! `ELECPLC` writes by hand.
//!
//! The command is `ELECIOIMPORT <file.xlsx> [x,y]`. It reads the *first* worksheet, finds the
//! header row, groups the points by card (the address prefix before the point number) and
//! places one `PLC-<n>` block per card in a grid starting at `x,y` (default `0,0`). The
//! attributes are the ones `ELECIO` and `ELECLIST` already read, so the generated drawing is
//! indistinguishable from one drafted with `ELECPLC`: `TAG1`, `DESC1`, `ADDR01..ADDRnn` and
//! the invisible `X1TERM01..X1TERMnn` pin coordinates.
//!
//! Everything the importer learned is reported on the command line: rows read, cards and
//! points generated, and every row it refused with the reason, so a list can be corrected
//! without opening the workbook again.

use super::*;

use calamine::{open_workbook_auto, Data, Reader};

/// Rows scanned looking for the header before the command gives up.
const HEADER_SCAN_ROWS: usize = 25;
/// Most worksheet rows read. A runaway sheet should not stall the UI thread.
const MAX_ROWS: usize = 20_000;
/// Most points one generated module carries; `ELECPLC` refuses more than this.
const MAX_POINTS_PER_MODULE: usize = 128;
/// Distance between two cards in the generated grid, in drawing units.
const CARD_SPACING_X: f64 = 8.0;
/// Smallest grid row height, in drawing units; taller cards push the next row down.
const CARD_MIN_HEIGHT: f64 = 3.0;
/// Pin spacing inside a generated module, matching `ELECPLC`.
const PIN_SPACING: f64 = 0.25;
/// Attribute text height inside a generated module, matching `ELECPLC`.
const LABEL_HEIGHT: f64 = 0.1;
/// Skip reasons printed before the rest are summarised as "and N more".
const MAX_SKIP_LINES: usize = 12;

/// One worksheet row that looked like an I/O point.
struct IoPoint {
    /// The address exactly as the worksheet spells it, e.g. `I0.3` or `%AI0.1`.
    address: String,
    /// Point number after the dot, used to order the points inside a card.
    point: u32,
    description: String,
    terminal: Option<String>,
}

/// One card: the points sharing an address prefix (`I0`, `Q0`, `AI0`, ...).
struct Card {
    prefix: String,
    /// `Module` column of the first row that had one, used as a label.
    module: Option<String>,
    /// Every distinct I/O type seen in the card, in first-seen order.
    kinds: Vec<&'static str>,
    points: Vec<IoPoint>,
}

impl Card {
    fn kind_label(&self) -> String {
        if self.kinds.is_empty() {
            "I/O".to_string()
        } else {
            self.kinds.join("+")
        }
    }

    fn label(&self) -> String {
        match &self.module {
            Some(module) if !module.is_empty() => {
                format!("{} card {} ({module})", self.kind_label(), self.prefix)
            }
            _ => format!("{} card {}", self.kind_label(), self.prefix),
        }
    }
}

/// Which worksheet column holds what. Only `address` is required.
struct IoColumns {
    address: usize,
    kind: Option<usize>,
    description: Option<usize>,
    terminal: Option<usize>,
    module: Option<usize>,
    /// Zero-based row index of the header inside the worksheet range.
    row: usize,
}

/// The role a header cell plays, if its text names a known column.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Role {
    Address,
    Kind,
    Description,
    Terminal,
    Module,
}

impl OpenCADStudio {
    /// Dispatch the I/O-list import commands. Returns `None` when the command is not ours.
    ///
    /// Registered in `src/app/commands/mod.rs` and listed in the command driver's
    /// early-argument table, so a line with arguments arrives here whole.
    pub(super) fn dispatch_elec_ioimport(&mut self, cmd: &str, i: usize) -> Option<Task<Message>> {
        const NAME: &str = "ELECIOIMPORT";
        let command = cmd.trim();
        // Dispatch is called for every command, so match without allocating: the first
        // bytes must be the name (ASCII, so `get` is a safe boundary test) and the rest must
        // be empty or start with whitespace.
        let head = command.get(..NAME.len())?;
        if !head.eq_ignore_ascii_case(NAME) {
            return None;
        }
        let rest = command.get(NAME.len()..).unwrap_or("");
        if !rest.is_empty() && !rest.starts_with(char::is_whitespace) {
            return None;
        }
        let arguments = rest.trim().to_string();
        Some(self.elec_ioimport(i, &arguments))
    }

    /// `ELECIOIMPORT <file.xlsx> [x,y]`.
    fn elec_ioimport(&mut self, i: usize, arguments: &str) -> Task<Message> {
        let (path, at) = split_arguments(arguments);
        let Some(path) = path else {
            self.command_line.push_output(
                "ELECIOIMPORT: use ELECIOIMPORT <file.xlsx> [x,y], e.g. ELECIOIMPORT C:\\io\\line1.xlsx 0,0",
            );
            return Task::none();
        };
        let file = std::path::Path::new(&path);
        if !file.is_file() {
            let message = format!("ELECIOIMPORT: {} is not a file.", file.display());
            self.command_line.push_error(&message);
            return Task::none();
        }

        let mut workbook = match open_workbook_auto(file) {
            Ok(workbook) => workbook,
            Err(error) => {
                let message = format!(
                    "ELECIOIMPORT: cannot read {}: {error} - an .xlsx, .xls, .xlsm or .ods file is expected.",
                    file.display()
                );
                self.command_line.push_error(&message);
                return Task::none();
            }
        };
        let sheet_names = workbook.sheet_names();
        let Some(sheet_name) = sheet_names.first().cloned() else {
            let message = format!(
                "ELECIOIMPORT: {} has no worksheets.",
                file.display()
            );
            self.command_line.push_error(&message);
            return Task::none();
        };
        let range = match workbook.worksheet_range(&sheet_name) {
            Ok(range) => range,
            Err(error) => {
                let message = format!(
                    "ELECIOIMPORT: cannot read worksheet \"{sheet_name}\" of {}: {error}",
                    file.display()
                );
                self.command_line.push_error(&message);
                return Task::none();
            }
        };

        // The worksheet as plain strings. A cell is text or a number; a number keeps a
        // plain integer form because an address typed as text usually is one.
        let start_row = range.start().map(|(row, _)| row as usize).unwrap_or(0);
        let total_rows = range.height();
        let rows: Vec<Vec<String>> = range
            .rows()
            .take(MAX_ROWS)
            .map(|row| row.iter().map(cell_text).collect())
            .collect();
        let truncated = total_rows > MAX_ROWS;

        if rows.is_empty() {
            let message = format!(
                "ELECIOIMPORT: worksheet \"{sheet_name}\" of {} is empty.",
                file.display()
            );
            self.command_line.push_output(&message);
            return Task::none();
        }

        let Some(header) = find_header(&rows) else {
            let message = format!(
                "ELECIOIMPORT: no header row found in the first {HEADER_SCAN_ROWS} row(s) of \"{sheet_name}\". \
                 The header needs an Address column and at least one of I/O Type, Description, Terminal or Module."
            );
            self.command_line.push_error(&message);
            return Task::none();
        };

        let mut cards: Vec<Card> = Vec::new();
        let mut skipped: Vec<String> = Vec::new();
        let mut blanks = 0usize;
        let mut data_rows = 0usize;

        for (index, cells) in rows.iter().enumerate().skip(header.row + 1) {
            if cells.iter().all(|cell| cell.is_empty()) {
                blanks += 1;
                continue;
            }
            data_rows += 1;
            let row_number = start_row + index + 1;
            let address = cell_at(cells, Some(header.address));
            if address.is_empty() {
                skipped.push(format!(
                    "row {row_number}: no address - {}",
                    row_preview(cells)
                ));
                continue;
            }
            // A header repeated at a page break is not an I/O point.
            if is_address_header(&normalize_header(address)) {
                skipped.push(format!("row {row_number}: repeated header row"));
                continue;
            }
            let Some((prefix, point)) = parse_point_address(address) else {
                skipped.push(format!(
                    "row {row_number}: \"{}\" is not an I/O address (want a prefix, a dot and a point number, e.g. I0.3)",
                    truncate(address, 40)
                ));
                continue;
            };
            let kind = match normalize_kind(cell_at(cells, header.kind)) {
                Some(kind) => kind,
                None => infer_kind(&prefix),
            };
            let description = cell_at(cells, header.description).to_string();
            let terminal = non_empty(cell_at(cells, header.terminal));
            let module = non_empty(cell_at(cells, header.module));

            let position = match cards.iter().position(|card| card.prefix == prefix) {
                Some(position) => position,
                None => {
                    cards.push(Card {
                        prefix: prefix.clone(),
                        module: module.clone(),
                        kinds: Vec::new(),
                        points: Vec::new(),
                    });
                    cards.len() - 1
                }
            };
            let card = &mut cards[position];
            if card.module.is_none() {
                card.module = module;
            }
            if !card.kinds.contains(&kind) {
                card.kinds.push(kind);
            }
            if card.points.iter().any(|existing| existing.point == point) {
                skipped.push(format!(
                    "row {row_number}: {address} repeats point {point} in card {}",
                    card.prefix
                ));
                continue;
            }
            card.points.push(IoPoint {
                address: address.to_string(),
                point,
                description,
                terminal,
            });
        }

        // Points inside a card follow the point number even when the sheet lists them out of
        // order; a card split at the module limit still reads top to bottom.
        for card in &mut cards {
            card.points.sort_by_key(|point| point.point);
        }

        if cards.is_empty() {
            let message = format!(
                "ELECIOIMPORT: no I/O points found in \"{sheet_name}\" - {data_rows} data row(s) read, \
                 {blanks} blank row(s) ignored, {} row(s) skipped.",
                skipped.len()
            );
            self.command_line.push_output(&message);
            print_skips(self, &skipped);
            return Task::none();
        }

        let (x0, y0) = at.unwrap_or((0.0, 0.0));
        let (generated, placed) = self.generate_plc_modules(i, &cards, x0, y0);

        let mut summary = format!(
            "ELECIOIMPORT: \"{sheet_name}\" from {} - {data_rows} row(s) read, {generated} card(s), \
             {placed} point(s) placed at {x0:.2},{y0:.2}; {} row(s) skipped, {blanks} blank row(s) ignored.",
            file.display(),
            skipped.len()
        );
        if truncated {
            summary.push_str(&format!(" The worksheet was cut at {MAX_ROWS} rows."));
        }
        self.command_line.push_output(&summary);
        print_skips(self, &skipped);
        Task::none()
    }

    /// Create one `PLC-<n>` module per card on the active tab and return
    /// `(cards generated, points placed)`.
    fn generate_plc_modules(
        &mut self,
        i: usize,
        cards: &[Card],
        x0: f64,
        y0: f64,
    ) -> (usize, usize) {
        // A card can exceed the per-module limit; each slice becomes its own module.
        let mut plans: Vec<(&Card, &[IoPoint])> = Vec::new();
        for card in cards {
            for slice in card.points.chunks(MAX_POINTS_PER_MODULE) {
                plans.push((card, slice));
            }
        }
        let columns = ((plans.len() as f64).sqrt().ceil() as usize).clamp(1, 4);
        let mut next_tag = next_module_tag(&self.tabs[i].scene.document);
        let mut generated = 0usize;
        let mut placed = 0usize;
        let mut top = 0.0_f64;
        let mut snapshot = false;
        let mut lines: Vec<String> = Vec::new();

        for chunk in plans.chunks(columns) {
            let tallest = chunk
                .iter()
                .map(|(_, points)| points.len())
                .max()
                .unwrap_or(1);
            let row_height = (tallest as f64 * PIN_SPACING + 1.6).max(CARD_MIN_HEIGHT);
            for (column, (card, points)) in chunk.iter().enumerate() {
                let x = x0 + column as f64 * CARD_SPACING_X;
                let y = y0 - top;
                let block_name = format!("PLC-{}", points.len());

                if self.tabs[i]
                    .scene
                    .document
                    .block_records
                    .get(&block_name)
                    .is_none()
                {
                    let entities = plc_block_entities(points.len());
                    if let Err(error) = self.tabs[i].scene.define_block_from_owned_entities(
                        entities,
                        &block_name,
                        glam::DVec3::ZERO,
                    ) {
                        lines.push(format!(
                            "  card {}: cannot build {block_name}: {error}",
                            card.prefix
                        ));
                        continue;
                    }
                }

                let tag = format!("-P{next_tag}");
                next_tag += 1;
                if !snapshot {
                    self.push_undo_snapshot(i, "ELECIOIMPORT");
                    snapshot = true;
                }
                let insert = build_plc_insert(&block_name, &tag, card, points, x, y);
                self.tabs[i]
                    .scene
                    .add_entity(codec::EntityType::Insert(insert));
                generated += 1;
                placed += points.len();
                let first = points.first().map(|p| p.address.as_str()).unwrap_or("");
                let last = points.last().map(|p| p.address.as_str()).unwrap_or("");
                lines.push(format!(
                    "  {tag}  {:<4} {:<6} {:>3} point(s)  {}..{}",
                    card.prefix,
                    card.kind_label(),
                    points.len(),
                    first,
                    last
                ));
            }
            top += row_height;
        }

        if generated > 0 {
            self.tabs[i].dirty = true;
        }
        for line in lines {
            self.command_line.push_output(&line);
        }
        if generated == 0 {
            self.command_line
                .push_error("ELECIOIMPORT: no module could be generated on this drawing.");
        }
        (generated, placed)
    }
}

/// Print the skip reasons, capped so a broken worksheet cannot flood the command line.
fn print_skips(app: &mut OpenCADStudio, skipped: &[String]) {
    for reason in skipped.iter().take(MAX_SKIP_LINES) {
        app.command_line.push_output(&format!("  {reason}"));
    }
    if skipped.len() > MAX_SKIP_LINES {
        app.command_line.push_output(&format!(
            "  ... and {} more skipped row(s)",
            skipped.len() - MAX_SKIP_LINES
        ));
    }
}

/// Split `ELECIOIMPORT`'s arguments into the workbook path and the optional insertion point.
///
/// The last word is the point when it parses as `x,y`; the rest is the path, so a path with
/// spaces works without quoting (the command line strips quotes from nothing).
fn split_arguments(arguments: &str) -> (Option<String>, Option<(f64, f64)>) {
    let trimmed = arguments.trim();
    if trimmed.is_empty() {
        return (None, None);
    }
    if let Some((head, last)) = trimmed.rsplit_once(' ') {
        if let Some(point) = parse_point(last) {
            let path = head.trim();
            if !path.is_empty() {
                return (Some(path.to_string()), Some(point));
            }
        }
    }
    (Some(trimmed.to_string()), None)
}

/// Parse a `x,y` pair.
fn parse_point(text: &str) -> Option<(f64, f64)> {
    let (x, y) = text.trim().split_once(',')?;
    let x = x.trim().parse::<f64>().ok()?;
    let y = y.trim().parse::<f64>().ok()?;
    if x.is_finite() && y.is_finite() {
        Some((x, y))
    } else {
        None
    }
}

/// Split an address into its card prefix and point number.
///
/// `I0.3` -> (`I0`, 3), `%AI0.1` -> (`AI0`, 1), `Q12.7` -> (`Q12`, 7). The prefix keeps its
/// letters and byte/digit part so `I0` and `I1` are different cards. The percent sign of a
/// Siemens-style address is accepted and ignored for the prefix.
fn parse_point_address(text: &str) -> Option<(String, u32)> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    let bare = text.strip_prefix('%').unwrap_or(text).trim();
    // Find the last dot: `I0.3` splits once; a description-like value has no dot and fails.
    let (head, tail) = bare.rsplit_once('.')?;
    let head = head.trim();
    let tail = tail.trim();
    if head.is_empty()
        || !head
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        || !head.chars().any(|c| c.is_ascii_alphabetic())
    {
        return None;
    }
    if tail.is_empty() || !tail.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let point: u32 = tail.parse().ok()?;
    // An I/O card addresses bits of a byte; 0..255 covers every convention used here.
    if point > 255 {
        return None;
    }
    Some((head.to_ascii_uppercase(), point))
}

/// Map an I/O-type cell to one of the four spellings the reports use.
fn normalize_kind(text: &str) -> Option<&'static str> {
    let value = text.trim().to_ascii_uppercase().replace(['/', '-', '_', ' '], "");
    match value.as_str() {
        "DI" | "DIGITALIN" | "DIGITALINPUT" | "INPUT" | "IN" | "DISCRETEIN" => Some("DI"),
        "DO" | "DIGITALOUT" | "DIGITALOUTPUT" | "OUTPUT" | "OUT" | "DISCRETEOUT" => Some("DO"),
        "AI" | "ANALOGIN" | "ANALOGINPUT" | "ANALOGUEIN" => Some("AI"),
        "AO" | "ANALOGOUT" | "ANALOGOUTPUT" | "ANALOGUEOUT" => Some("AO"),
        _ => None,
    }
}

/// Guess the I/O type when the list has no type column, from the address letters.
fn infer_kind(prefix: &str) -> &'static str {
    let upper = prefix.to_ascii_uppercase();
    if upper.starts_with("AI") {
        "AI"
    } else if upper.starts_with("AO") {
        "AO"
    } else if upper.starts_with('Q') || upper.starts_with('O') {
        "DO"
    } else {
        "DI"
    }
}

/// Read one worksheet cell as trimmed text.
fn cell_text(cell: &Data) -> String {
    match cell {
        Data::Empty => String::new(),
        Data::String(text) => text.trim().to_string(),
        Data::Int(value) => value.to_string(),
        Data::Float(value) => {
            if value.fract() == 0.0 && value.abs() < 1e15 {
                format!("{value:.0}")
            } else {
                value.to_string()
            }
        }
        Data::Bool(value) => value.to_string(),
        Data::DateTime(value) => value.as_f64().to_string(),
        Data::DateTimeIso(text) | Data::DurationIso(text) => text.trim().to_string(),
        Data::Error(_) => String::new(),
    }
}

/// Lower-case header text with everything but letters and digits removed.
///
/// `I/O Type`, `I-O type` and `io_type` all become `iotype`.
fn normalize_header(text: &str) -> String {
    text.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

/// The role a header names, if any.
fn role_of(name: &str) -> Option<Role> {
    if is_address_header(name) {
        return Some(Role::Address);
    }
    if matches!(
        name,
        "iotype" | "type" | "signaltype" | "iosignaltype" | "pointtype" | "kind"
    ) {
        return Some(Role::Kind);
    }
    if matches!(
        name,
        "description"
            | "desc"
            | "comment"
            | "comments"
            | "iodescription"
            | "pointdescription"
            | "signaldescription"
            | "text"
            | "label"
    ) {
        return Some(Role::Description);
    }
    if matches!(
        name,
        "terminal" | "term" | "terminals" | "terminalnumber" | "terminalno" | "terminalstrip"
    ) {
        return Some(Role::Terminal);
    }
    if matches!(
        name,
        "module" | "card" | "slot" | "rack" | "iomodule" | "modulecard"
    ) {
        return Some(Role::Module);
    }
    None
}

/// True for the header names that mean "the address column".
fn is_address_header(name: &str) -> bool {
    matches!(
        name,
        "address"
            | "addr"
            | "io"
            | "ioaddress"
            | "plcaddress"
            | "signaladdress"
            | "absoluteaddress"
            | "pointaddress"
    )
}

/// Find the header row among the first rows, requiring an address column.
fn find_header(rows: &[Vec<String>]) -> Option<IoColumns> {
    for (index, cells) in rows.iter().take(HEADER_SCAN_ROWS).enumerate() {
        let mut address: Option<usize> = None;
        let mut kind: Option<usize> = None;
        let mut description: Option<usize> = None;
        let mut terminal: Option<usize> = None;
        let mut module: Option<usize> = None;
        for (column, cell) in cells.iter().enumerate() {
            match role_of(&normalize_header(cell)) {
                Some(Role::Address) if address.is_none() => address = Some(column),
                Some(Role::Kind) if kind.is_none() => kind = Some(column),
                Some(Role::Description) if description.is_none() => description = Some(column),
                Some(Role::Terminal) if terminal.is_none() => terminal = Some(column),
                Some(Role::Module) if module.is_none() => module = Some(column),
                _ => {}
            }
        }
        if let Some(address) = address {
            if kind.is_some()
                || description.is_some()
                || terminal.is_some()
                || module.is_some()
            {
                return Some(IoColumns {
                    address,
                    kind,
                    description,
                    terminal,
                    module,
                    row: index,
                });
            }
        }
    }
    None
}

/// One cell as text, empty when the column is absent.
fn cell_at<'a>(cells: &'a [String], column: Option<usize>) -> &'a str {
    column
        .and_then(|column| cells.get(column))
        .map(|value| value.as_str())
        .unwrap_or("")
}

/// A trimmed string, or `None` when it is empty.
fn non_empty(text: &str) -> Option<String> {
    let text = text.trim();
    if text.is_empty() {
        None
    } else {
        Some(text.to_string())
    }
}

/// Shorten text for a one-line command output.
fn truncate(text: &str, width: usize) -> String {
    let text = text.trim();
    if text.chars().count() <= width {
        return text.to_string();
    }
    let mut out: String = text.chars().take(width.saturating_sub(3)).collect();
    out.push_str("...");
    out
}

/// The non-empty cells of a row, short enough to identify it in a skip reason.
fn row_preview(cells: &[String]) -> String {
    let mut parts: Vec<String> = cells
        .iter()
        .filter(|cell| !cell.is_empty())
        .map(|cell| truncate(cell, 24))
        .collect();
    if parts.is_empty() {
        return "blank".to_string();
    }
    if parts.len() > 3 {
        parts.truncate(3);
        parts.push("...".to_string());
    }
    format!("\"{}\"", parts.join(" | "))
}

/// The geometry of a `PLC-<n>` module block, identical to the one `ELECPLC` builds.
fn plc_block_entities(points: usize) -> Vec<codec::EntityType> {
    const WIDTH: f64 = 0.8;
    let mut entities: Vec<codec::EntityType> = Vec::new();
    let mut line = |ax: f64, ay: f64, bx: f64, by: f64| {
        let mut entity = codec::entities::Line::from_points(
            codec::types::Vector3::new(ax, ay, 0.0),
            codec::types::Vector3::new(bx, by, 0.0),
        );
        entity.common.layer = "ELEC-SYMBOLS".to_string();
        entities.push(codec::EntityType::Line(entity));
    };
    let top = points as f64 * PIN_SPACING;
    line(0.0, 0.0, WIDTH, 0.0);
    line(WIDTH, 0.0, WIDTH, top);
    line(WIDTH, top, 0.0, top);
    line(0.0, top, 0.0, 0.0);
    for point in 0..points {
        let y = (point as f64 + 0.5) * PIN_SPACING;
        line(-0.15, y, 0.0, y);
        line(WIDTH, y, WIDTH + 0.15, y);
    }
    entities
}

/// True when an owner handle names drawing space, not a block definition.
///
/// `add_entity` leaves the owner null until the drawing is written, so a module added
/// earlier in the same session is still part of the drawing; this is the same rule
/// `elec.rs` uses for component collection, kept local because that helper is private to
/// its module.
fn is_drawing_space(document: &codec::Document, owner: codec::Handle) -> bool {
    if owner.is_null() {
        return true;
    }
    document.block_records.iter().any(|record| {
        record.handle == owner
            && (record.name.eq_ignore_ascii_case("*Model_Space")
                || record.name.to_ascii_lowercase().starts_with("*paper_space"))
    })
}

/// The next free `-P<n>` tag on the drawing, so generated modules never collide.
fn next_module_tag(document: &codec::Document) -> u32 {
    let mut highest = 0u32;
    for entity in document.entities() {
        if !is_drawing_space(document, entity.common().owner_handle) {
            continue;
        }
        let codec::EntityType::Insert(insert) = entity else {
            continue;
        };
        for attribute in &insert.attributes {
            let name = attribute.tag.trim().to_ascii_uppercase();
            if !matches!(name.as_str(), "TAG" | "TAG1" | "TAG2" | "TAGSTR" | "WORKID") {
                continue;
            }
            let value = attribute.value.trim().to_ascii_uppercase();
            if let Some(rest) = value.strip_prefix("-P") {
                if let Ok(number) = rest.parse::<u32>() {
                    highest = highest.max(number);
                }
            }
        }
    }
    highest + 1
}

/// Push one attribute onto an insert, at a given point.
fn push_attribute(
    insert: &mut codec::entities::Insert,
    name: &str,
    value: &str,
    x: f64,
    y: f64,
    visible: bool,
) {
    let mut attribute =
        codec::entities::AttributeEntity::new(name.to_string(), value.to_string());
    attribute.height = LABEL_HEIGHT;
    attribute.insertion_point = codec::types::Vector3::new(x, y, 0.0);
    attribute.alignment_point = attribute.insertion_point;
    if !visible {
        attribute.flags.invisible = true;
    }
    insert.attributes.push(attribute);
}

/// Build the insert for one card: the module attributes `ELECIO` and `ELECLIST` already read,
/// plus the per-point description, terminal and pin-position attributes.
///
/// `ADDRnn` and the invisible `X1TERMnn` keep the exact names and coordinate format
/// `ELECPLC` writes, so `ELECIO` follows the pin to the conductor with no change.
fn build_plc_insert(
    block_name: &str,
    tag: &str,
    card: &Card,
    points: &[IoPoint],
    x: f64,
    y: f64,
) -> codec::entities::Insert {
    let top = points.len() as f64 * PIN_SPACING;
    let mut insert =
        codec::entities::Insert::new(block_name, codec::types::Vector3::new(x, y, 0.0));
    // Module header above the body, the way a drafted module is titled.
    push_attribute(&mut insert, "TAG1", tag, x + 0.9, y + top + 0.1, true);
    push_attribute(
        &mut insert,
        "DESC1",
        &card.label(),
        x + 0.9,
        y + top + 0.35,
        true,
    );
    push_attribute(
        &mut insert,
        "CARD",
        &card.prefix,
        x + 3.2,
        y + top + 0.1,
        true,
    );
    push_attribute(
        &mut insert,
        "IOTYPE",
        &card.kind_label(),
        x + 3.2,
        y + top + 0.35,
        true,
    );
    if let Some(module) = &card.module {
        push_attribute(
            &mut insert,
            "MODULE",
            module,
            x + 4.8,
            y + top + 0.1,
            true,
        );
    }
    for (index, point) in points.iter().enumerate() {
        let number = index + 1;
        let pin_y = y + (index as f64 + 0.5) * PIN_SPACING;
        push_attribute(
            &mut insert,
            &format!("ADDR{number:02}"),
            &point.address,
            x + 0.9,
            pin_y,
            true,
        );
        if !point.description.is_empty() {
            push_attribute(
                &mut insert,
                &format!("DESC{number:02}"),
                &point.description,
                x + 2.4,
                pin_y,
                true,
            );
        }
        if let Some(terminal) = &point.terminal {
            push_attribute(
                &mut insert,
                &format!("TERM{number:02}"),
                terminal,
                x + 6.2,
                pin_y,
                true,
            );
        }
        push_attribute(
            &mut insert,
            &format!("X1TERM{number:02}"),
            &format!("{:.4},{:.4}", x - 0.15, pin_y),
            x + 0.9,
            pin_y,
            false,
        );
    }
    insert
}

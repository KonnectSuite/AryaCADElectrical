//! Cable management and the cable schedule.
//!
//! A cable is a bundle of conductors that runs between two points of an installation; the
//! schedule is the document the wireman uses to pull it. SkyCAD keeps cables as first-class
//! objects and generates the schedule from the schematics; this module keeps the cable
//! records next to the project (`cables.tsv`) so they travel with it, can be edited in a
//! text editor, and generates the same schedule with one command.
//!
//! A cable can also be **drawn**: `ELECCABLEDRAW` puts its route on the `CABLES` layer, its
//! tag on the drawing and its shield bracket where `SHIELD=yes`, and stores the route in the
//! schedule record. The conductors come from the drawing's geometry: the length is measured
//! from the route and each end is resolved to the device pin it lands on.
//!
//! Commands:
//!   `ELECCABLEDRAW <tag> [CORES=<n>] [PART=<p>] [MFG=<m>] [LEN=<m>] [SHIELD=yes] [DESC=<text>]`
//!                               draw the cable route; Enter ends the polyline
//!   `ELECCABLE <tag> CORES=<n> [PART=<p>] [MFG=<m>] [LEN=<metres>] [DESC=<text>]`
//!                               schedule-only cable (no geometry)
//!   `ELECCABLELIST [out.csv]`   print the schedule, or write it as CSV
//!   `ELECCABLEDEL <tag>`        remove a cable from the schedule

use super::*;
use crate::command::{CadCommand, CmdResult};
use std::path::PathBuf;

/// One cable in the schedule.
#[derive(Debug, Clone, Default)]
struct Cable {
    tag: String,
    cores: String,
    part: String,
    manufacturer: String,
    length: String,
    description: String,
    /// The drawn route, in drawing units; empty for a schedule-only cable.
    points: Vec<(f64, f64)>,
    shield: bool,
}

impl Cable {
    fn to_line(&self) -> String {
        let route = self
            .points
            .iter()
            .map(|(x, y)| format!("{x:.4},{y:.4}"))
            .collect::<Vec<_>>()
            .join(" ");
        [
            self.tag.as_str(),
            self.cores.as_str(),
            self.part.as_str(),
            self.manufacturer.as_str(),
            self.length.as_str(),
            self.description.as_str(),
            route.as_str(),
            if self.shield { "yes" } else { "" },
        ]
        .join("\t")
    }

    fn from_line(line: &str) -> Option<Self> {
        let mut fields = line.split('\t');
        let tag = fields.next()?.trim();
        if tag.is_empty() || tag.starts_with('#') || tag.eq_ignore_ascii_case("tag") {
            return None;
        }
        let mut rest: Vec<String> = fields.map(|field| field.trim().to_string()).collect();
        // The route and the shield flag are the two trailing columns; both are empty for a
        // typed cable, so trailing empty fields are stripped before looking for them.
        for _ in 0..2 {
            if rest.last().map(String::is_empty).unwrap_or(false) {
                rest.pop();
            }
        }
        let mut shield = false;
        if let Some(last) = rest.last() {
            if last.eq_ignore_ascii_case("yes") || last.eq_ignore_ascii_case("no") {
                shield = last.eq_ignore_ascii_case("yes");
                rest.pop();
            }
        }
        let mut points = Vec::new();
        if let Some(last) = rest.last() {
            let route = parse_route(last);
            // A description may contain a comma; only a real route (two or more points) is
            // taken as geometry.
            if route.len() >= 2 {
                points = route;
                rest.pop();
            }
        }
        Some(Self {
            tag: tag.to_string(),
            cores: rest.first().cloned().unwrap_or_default(),
            part: rest.get(1).cloned().unwrap_or_default(),
            manufacturer: rest.get(2).cloned().unwrap_or_default(),
            length: rest.get(3).cloned().unwrap_or_default(),
            description: rest.get(4).cloned().unwrap_or_default(),
            points,
            shield,
        })
    }

    /// Measured length of the drawn route, when it has one.
    fn drawn_length(&self) -> Option<f64> {
        if self.points.len() < 2 {
            return None;
        }
        let mut total = 0.0;
        for pair in self.points.windows(2) {
            total += (pair[1].0 - pair[0].0).hypot(pair[1].1 - pair[0].1);
        }
        Some(total)
    }

    /// The length the schedule shows: measured from the drawing when drawn, else typed.
    fn schedule_length(&self) -> String {
        match self.drawn_length() {
            Some(value) => format!("{value:.2}"),
            None => self.length.clone(),
        }
    }
}

/// Parse `"x,y x,y ..."` into a route.
fn parse_route(text: &str) -> Vec<(f64, f64)> {
    text.split_whitespace()
        .filter_map(|pair| {
            let (x, y) = pair.split_once(',')?;
            Some((x.trim().parse::<f64>().ok()?, y.trim().parse::<f64>().ok()?))
        })
        .collect()
}


impl OpenCADStudio {
    /// Dispatch the cable commands. Returns `None` when the command is not ours.
    pub(super) fn dispatch_elec_cables(&mut self, cmd: &str, i: usize) -> Option<Task<Message>> {
        let command = cmd.trim();
        let upper = command.to_ascii_uppercase();
        let argument_of = |verb: &str| -> Option<String> {
            if upper == verb {
                Some(String::new())
            } else if upper.starts_with(&format!("{verb} ")) {
                Some(command[verb.len()..].trim().to_string())
            } else {
                None
            }
        };
        if let Some(argument) = argument_of("ELECCABLEDRAW") {
            return Some(self.elec_cable_draw(i, &argument));
        }
        if let Some(argument) = argument_of("ELECCABLELIST") {
            return Some(self.elec_cable_list(i, &argument));
        }
        if let Some(argument) = argument_of("ELECCABLEDEL") {
            return Some(self.elec_cable_delete(&argument));
        }
        if let Some(argument) = argument_of("ELECCABLE") {
            return Some(self.elec_cable_upsert(&argument));
        }
        None
    }

    /// `ELECCABLEDRAW <tag> [CORES=] [PART=] [MFG=] [LEN=] [SHIELD=yes] [DESC=]`
    ///
    /// Starts the cable tool: the next points are the cable's route (Enter ends it). The
    /// route lands on the `CABLES` layer, the tag beside its first point, and a shield
    /// bracket when requested. The schedule record is written as the route is drawn, so the
    /// schedule and the drawing cannot disagree.
    fn elec_cable_draw(&mut self, i: usize, arguments: &str) -> Task<Message> {
        let Some(path) = cable_file() else {
            self.command_line.push_output(
                "ELECCABLEDRAW: no project is loaded — ELECPROJNEW or ELECPROJOPEN first.",
            );
            return Task::none();
        };
        let mut cable = match parse_cable_arguments(arguments) {
            Ok(cable) => cable,
            Err(message) => {
                self.command_line.push_output(&message);
                return Task::none();
            }
        };
        if cable.cores.trim().is_empty() {
            cable.cores = "1".to_string();
        }
        let prompt = format!(
            "ELECCABLEDRAW {}  Specify start point:",
            cable.tag.to_ascii_uppercase()
        );
        self.command_line.push_output(&prompt);
        self.tabs[i].active_cmd = Some(Box::new(CableCommand::new(cable, path)));
        Task::none()
    }

    fn elec_cable_upsert(&mut self, arguments: &str) -> Task<Message> {
        let Some(path) = cable_file() else {
            self.command_line.push_output(
                "ELECCABLE: no project is loaded — ELECPROJNEW or ELECPROJOPEN first.",
            );
            return Task::none();
        };
        let cable = match parse_cable_arguments(arguments) {
            Ok(cable) => cable,
            Err(message) => {
                self.command_line.push_output(&message);
                return Task::none();
            }
        };
        if cable.cores.trim().is_empty() {
            self.command_line.push_output(
                "ELECCABLE: a cable needs a conductor count — add CORES=<n>.",
            );
            return Task::none();
        }

        let mut cables = read_cables(&path);
        let mut cable = cable;
        // A typed cable keeps whatever route was drawn earlier for the same tag.
        let existing = cables
            .iter()
            .position(|other| other.tag.eq_ignore_ascii_case(&cable.tag));
        let replaced = existing.is_some();
        match existing {
            Some(index) => {
                cable.points = cables[index].points.clone();
                cable.shield = cable.shield || cables[index].shield;
                cables[index] = cable.clone();
            }
            None => cables.push(cable.clone()),
        }
        match write_cables(&path, &cables) {
            Ok(()) => {
                let verb = if replaced { "updated" } else { "added" };
                self.command_line.push_output(&format!(
                    "ELECCABLE: {verb} {} — {} core(s){} ({} cable(s) in the schedule).",
                    cable.tag,
                    cable.cores,
                    if cable.part.is_empty() {
                        String::new()
                    } else {
                        format!(", {}", cable.part)
                    },
                    cables.len()
                ));
            }
            Err(error) => {
                let message = format!("ELECCABLE: cannot write {}: {error}", path.display());
                self.command_line.push_output(&message);
            }
        }
        Task::none()
    }

    fn elec_cable_delete(&mut self, arguments: &str) -> Task<Message> {
        let Some(path) = cable_file() else {
            self.command_line
                .push_output("ELECCABLEDEL: no project is loaded.");
            return Task::none();
        };
        let tag = arguments.trim();
        if tag.is_empty() {
            self.command_line
                .push_output("ELECCABLEDEL: use ELECCABLEDEL <tag>.");
            return Task::none();
        }
        let mut cables = read_cables(&path);
        let before = cables.len();
        cables.retain(|cable| !cable.tag.eq_ignore_ascii_case(tag));
        if cables.len() == before {
            self.command_line
                .push_output(&format!("ELECCABLEDEL: no cable {tag} in the schedule."));
            return Task::none();
        }
        match write_cables(&path, &cables) {
            Ok(()) => self.command_line.push_output(&format!(
                "ELECCABLEDEL: removed {tag} ({} cable(s) left).",
                cables.len()
            )),
            Err(error) => self.command_line.push_output(&format!(
                "ELECCABLEDEL: cannot write {}: {error}",
                path.display()
            )),
        }
        Task::none()
    }

    /// `ELECCABLELIST [out.csv]` prints the schedule or writes it as CSV.
    ///
    /// For a drawn cable the length is measured from its route and the two ends are resolved
    /// to the device pins they land on; `LENGTH`, `FROM` and `TO` therefore come from the
    /// drawing rather than from typing.
    fn elec_cable_list(&mut self, i: usize, arguments: &str) -> Task<Message> {
        let Some(path) = cable_file() else {
            self.command_line
                .push_output("ELECCABLELIST: no project is loaded.");
            return Task::none();
        };
        let cables = read_cables(&path);
        if cables.is_empty() {
            self.command_line.push_output(
                "ELECCABLELIST: no cables yet — ELECCABLEDRAW <tag> draws one, ELECCABLE <tag> CORES=<n> types one.",
            );
            return Task::none();
        }
        let anchors = {
            let document = &self.tabs[i].scene.document;
            super::elec::device_pin_points(document)
        };
        // The columns are SkyCAD's cable-schedule set, so an existing company template can
        // be filled from the same data; FROM and TO are added for a drawn cable.
        let header = format!(
            "{:<10} {:<6} {:<18} {:<16} {:>8} {:<7} {:<14} {:<14}  {:<28}",
            "CABLE", "CORES", "PART NUMBER", "MANUFACTURER", "LENGTH", "SHIELD", "FROM", "TO",
            "DESCRIPTION"
        );
        self.command_line.push_output(&header);
        let mut rows: Vec<(String, String, String)> = Vec::new();
        for cable in &cables {
            let (from, to) = cable_ends(cable, &anchors);
            let length = cable.schedule_length();
            let line = format!(
                "{:<10} {:<6} {:<18} {:<16} {:>8} {:<7} {:<14} {:<14}  {:<28}",
                cable.tag,
                cable.cores,
                if cable.part.is_empty() { "-" } else { &cable.part },
                if cable.manufacturer.is_empty() {
                    "-"
                } else {
                    &cable.manufacturer
                },
                if length.is_empty() { "-".to_string() } else { length.clone() },
                if cable.shield { "yes" } else { "-" },
                from,
                to,
                cable.description
            );
            self.command_line.push_output(&line);
            rows.push((from, to, length));
        }
        let out = arguments.trim();
        if !out.is_empty() {
            let mut text = String::from(
                "Cable,Description,Part Number,Manufacturer,Length,Conductor,Shield,From,To\n",
            );
            for (cable, (from, to, length)) in cables.iter().zip(rows.iter()) {
                text.push_str(&format!(
                    "{},{},{},{},{},{},{},{},{}\n",
                    csv_field(&cable.tag),
                    csv_field(&cable.description),
                    csv_field(&cable.part),
                    csv_field(&cable.manufacturer),
                    csv_field(length),
                    csv_field(&cable.cores),
                    if cable.shield { "yes" } else { "" },
                    csv_field(from),
                    csv_field(to)
                ));
            }
            match std::fs::write(out, text) {
                Ok(()) => self.command_line.push_output(&format!(
                    "ELECCABLELIST: wrote {} row(s) to {out}.",
                    cables.len()
                )),
                Err(error) => self
                    .command_line
                    .push_output(&format!("ELECCABLELIST: cannot write {out}: {error}")),
            }
        }
        Task::none()
    }
}

/// The project's cable schedule file.
fn cable_file() -> Option<PathBuf> {
    super::elec::project_dir_for_revisions().map(|folder| folder.join("cables.tsv"))
}

fn read_cables(path: &std::path::Path) -> Vec<Cable> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    text.lines().filter_map(Cable::from_line).collect()
}

fn write_cables(path: &std::path::Path, cables: &[Cable]) -> std::io::Result<()> {
    let mut text = String::from("# AryaCAD cable schedule - one cable per row\ntag\tcores\tpart\tmanufacturer\tlength\tdescription\troute\tshield\n");
    for cable in cables {
        text.push_str(&cable.to_line());
        text.push('\n');
    }
    std::fs::write(path, text)
}

/// CSV field quoting for a value that may hold a comma or a quote.
fn csv_field(value: &str) -> String {
    if value.contains(',') || value.contains('"') || value.contains('\n') {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

/// Parse the shared `ELECCABLE` / `ELECCABLEDRAW` argument grammar.
fn parse_cable_arguments(arguments: &str) -> Result<Cable, String> {
    let tokens: Vec<&str> = arguments.split_whitespace().collect();
    let tag = tokens
        .first()
        .copied()
        .map(str::trim)
        .filter(|tag| !tag.is_empty())
        .ok_or_else(|| {
            "ELECCABLE: use ELECCABLE <tag> CORES=<n> [PART=<p>] [MFG=<m>] [LEN=<m>] [SHIELD=yes] [DESC=<text>]."
                .to_string()
        })?;
    let mut cable = Cable {
        tag: tag.to_string(),
        ..Cable::default()
    };
    for (position, word) in tokens.iter().enumerate().skip(1) {
        let Some((key, value)) = word.split_once('=') else {
            continue;
        };
        let key = key.trim().to_ascii_uppercase();
        // A description is prose, so it takes the rest of the line rather than one word.
        if key == "DESC" || key == "DESCRIPTION" {
            cable.description = tokens[position..]
                .join(" ")
                .splitn(2, '=')
                .nth(1)
                .unwrap_or("")
                .trim()
                .to_string();
            break;
        }
        let value = value.trim().to_string();
        match key.as_str() {
            "CORES" | "CONDUCTORS" => cable.cores = value,
            "PART" | "PARTNUMBER" => cable.part = value,
            "MFG" | "MANUFACTURER" => cable.manufacturer = value,
            "LEN" | "LENGTH" => cable.length = value,
            "SHIELD" => {
                cable.shield = value.eq_ignore_ascii_case("yes")
                    || value.eq_ignore_ascii_case("on")
                    || value == "1"
            }
            _ => {}
        }
    }
    Ok(cable)
}

/// The device pin at each end of a drawn cable, as `TAG:pin` labels.
fn cable_ends(cable: &Cable, anchors: &[(f64, f64, String)]) -> (String, String) {
    let Some(first) = cable.points.first() else {
        return ("-".to_string(), "-".to_string());
    };
    let last = cable.points.last().unwrap_or(first);
    // A cable end lands on a device pin or a terminal a little away from the exact point;
    // 1.0 drawing unit is the measured association radius of the diagram.
    const END_ASSOC: f64 = 1.0;
    (
        nearest_anchor(anchors, *first, END_ASSOC),
        nearest_anchor(anchors, *last, END_ASSOC),
    )
}

fn nearest_anchor(anchors: &[(f64, f64, String)], point: (f64, f64), max: f64) -> String {
    let mut best: Option<(f64, &str)> = None;
    for (x, y, label) in anchors {
        let distance = (x - point.0).hypot(y - point.1);
        if distance <= max && best.as_ref().map(|(seen, _)| distance < *seen).unwrap_or(true) {
            best = Some((distance, label.as_str()));
        }
    }
    best.map(|(_, label)| label.to_string())
        .unwrap_or_else(|| "-".to_string())
}

/// Layer the cable route, tag and shield bracket are drawn on.
const CABLE_LAYER: &str = "CABLES";

/// The drawn cable tool.
///
/// Route segments go on the `CABLES` layer with the tag beside the first point and a shield
/// bracket when the cable is shielded. The schedule record is upserted as each segment is
/// committed, so a drawn cable never exists only as geometry.
pub(super) struct CableCommand {
    cable: Cable,
    path: PathBuf,
    last: Option<(f64, f64)>,
    committed: usize,
}

impl CableCommand {
    pub(super) fn new(cable: Cable, path: PathBuf) -> Self {
        Self {
            cable,
            path,
            last: None,
            committed: 0,
        }
    }

    fn segment(from: (f64, f64), to: (f64, f64)) -> codec::EntityType {
        let mut line = codec::entities::Line::from_points(
            codec::types::Vector3::new(from.0, from.1, 0.0),
            codec::types::Vector3::new(to.0, to.1, 0.0),
        );
        line.common.layer = CABLE_LAYER.to_string();
        codec::EntityType::Line(line)
    }

    fn tag_text(&self, at: (f64, f64)) -> codec::EntityType {
        let mut text = codec::entities::Text::new();
        text.value = self.cable.tag.to_ascii_uppercase();
        text.height = 2.5;
        text.insertion_point = codec::types::Vector3::new(at.0 + 1.0, at.1 + 1.5, 0.0);
        text.common.layer = CABLE_LAYER.to_string();
        codec::EntityType::Text(text)
    }

    /// The shield bracket: a semicircle over the first leg, the IEC drawing convention.
    fn shield_arc(from: (f64, f64), to: (f64, f64)) -> codec::EntityType {
        let mid = ((from.0 + to.0) / 2.0, (from.1 + to.1) / 2.0);
        let mut arc = codec::entities::Arc::new();
        arc.center = codec::types::Vector3::new(mid.0, mid.1, 0.0);
        arc.radius = 1.5;
        arc.start_angle = 0.0;
        arc.end_angle = std::f64::consts::PI;
        arc.common.layer = CABLE_LAYER.to_string();
        codec::EntityType::Arc(arc)
    }

    fn persist(&self) {
        let mut cables = read_cables(&self.path);
        match cables
            .iter()
            .position(|other| other.tag.eq_ignore_ascii_case(&self.cable.tag))
        {
            Some(index) => cables[index] = self.cable.clone(),
            None => cables.push(self.cable.clone()),
        }
        let _ = write_cables(&self.path, &cables);
    }
}

impl CadCommand for CableCommand {
    fn name(&self) -> &'static str {
        "ELECCABLEDRAW"
    }

    fn prompt(&self) -> String {
        match self.last {
            None => format!("ELECCABLEDRAW {}  Specify start point:", self.cable.tag),
            Some(_) => format!(
                "ELECCABLEDRAW {}  Specify next point  [Enter to finish]:",
                self.cable.tag
            ),
        }
    }

    fn on_point(&mut self, point: glam::DVec3) -> CmdResult {
        let here = (point.x, point.y);
        match self.last {
            None => {
                self.last = Some(here);
                self.cable.points.push(here);
                CmdResult::NeedPoint
            }
            Some(from) => {
                self.last = Some(here);
                self.cable.points.push(here);
                self.committed += 1;
                self.persist();
                let mut entities = Vec::new();
                if self.committed == 1 {
                    entities.push(self.tag_text(from));
                    if self.cable.shield {
                        entities.push(Self::shield_arc(from, here));
                    }
                }
                entities.push(Self::segment(from, here));
                CmdResult::CommitEntities(entities)
            }
        }
    }

    fn on_enter(&mut self) -> CmdResult {
        if self.committed > 0 {
            self.persist();
        }
        CmdResult::Done
    }
}

/// The cable schedule as a report sheet, for `ELECREPORT`.
pub(super) fn cable_sheet(document: &codec::Document) -> Option<::elec::Sheet> {
    let path = cable_file()?;
    let cables = read_cables(&path);
    if cables.is_empty() {
        return None;
    }
    let anchors = super::elec::device_pin_points(document);
    let mut sheet = ::elec::Sheet::new(
        "Cables",
        &[
            "Cable",
            "Description",
            "Part Number",
            "Manufacturer",
            "Length",
            "Conductor",
            "Shield",
            "From",
            "To",
        ],
    );
    for cable in &cables {
        let (from, to) = cable_ends(cable, &anchors);
        sheet.push(vec![
            ::elec::Cell::text(cable.tag.clone()),
            ::elec::Cell::text(cable.description.clone()),
            ::elec::Cell::text(cable.part.clone()),
            ::elec::Cell::text(cable.manufacturer.clone()),
            ::elec::Cell::text(cable.schedule_length()),
            ::elec::Cell::text(cable.cores.clone()),
            ::elec::Cell::text(if cable.shield { "yes" } else { "" }),
            ::elec::Cell::text(from),
            ::elec::Cell::text(to),
        ]);
    }
    Some(sheet)
}

//! Cable management and the cable schedule.
//!
//! A cable is a bundle of conductors that runs between two points of an installation; the
//! schedule is the document the wireman uses to pull it. SkyCAD keeps cables as first-class
//! objects and generates the schedule from the schematics; this module keeps the cable
//! records next to the project (`cables.tsv`) so they travel with it, can be edited in a
//! text editor, and generate the same schedule with one command.
//!
//! Commands:
//!   `ELECCABLE <tag> CORES=<n> [PART=<p>] [MFG=<m>] [LEN=<metres>] [DESC=<text>]`
//!   `ELECCABLELIST [out.csv]`   print the schedule, or write it as CSV
//!   `ELECCABLEDEL <tag>`        remove a cable from the schedule

use super::*;
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
}

impl Cable {
    fn to_line(&self) -> String {
        [
            self.tag.as_str(),
            self.cores.as_str(),
            self.part.as_str(),
            self.manufacturer.as_str(),
            self.length.as_str(),
            self.description.as_str(),
        ]
        .join("\t")
    }

    fn from_line(line: &str) -> Option<Self> {
        let mut fields = line.split('\t');
        let tag = fields.next()?.trim();
        if tag.is_empty() || tag.starts_with('#') || tag.eq_ignore_ascii_case("tag") {
            return None;
        }
        Some(Self {
            tag: tag.to_string(),
            cores: fields.next().unwrap_or("").trim().to_string(),
            part: fields.next().unwrap_or("").trim().to_string(),
            manufacturer: fields.next().unwrap_or("").trim().to_string(),
            length: fields.next().unwrap_or("").trim().to_string(),
            description: fields.next().unwrap_or("").trim().to_string(),
        })
    }
}

impl OpenCADStudio {
    /// Dispatch the cable commands. Returns `None` when the command is not ours.
    pub(super) fn dispatch_elec_cables(&mut self, cmd: &str, _i: usize) -> Option<Task<Message>> {
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
        if let Some(argument) = argument_of("ELECCABLELIST") {
            return Some(self.elec_cable_list(&argument));
        }
        if let Some(argument) = argument_of("ELECCABLEDEL") {
            return Some(self.elec_cable_delete(&argument));
        }
        if let Some(argument) = argument_of("ELECCABLE") {
            return Some(self.elec_cable_upsert(&argument));
        }
        None
    }

    fn elec_cable_upsert(&mut self, arguments: &str) -> Task<Message> {
        let Some(path) = cable_file() else {
            self.command_line.push_output(
                "ELECCABLE: no project is loaded — ELECPROJNEW or ELECPROJOPEN first.",
            );
            return Task::none();
        };
        let tokens: Vec<&str> = arguments.split_whitespace().collect();
        let Some(tag) = tokens.first().copied().map(str::trim).filter(|tag| !tag.is_empty()) else {
            self.command_line.push_output(
                "ELECCABLE: use ELECCABLE <tag> CORES=<n> [PART=<p>] [MFG=<m>] [LEN=<m>] [DESC=<text>].",
            );
            return Task::none();
        };
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
            // Every other value is a single token.
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
                _ => {}
            }
        }
        if cable.cores.is_empty() {
            self.command_line.push_output(
                "ELECCABLE: a cable needs a conductor count — add CORES=<n>.",
            );
            return Task::none();
        }

        let mut cables = read_cables(&path);
        let existing = cables
            .iter()
            .position(|other| other.tag.eq_ignore_ascii_case(&cable.tag));
        let replaced = existing.is_some();
        match existing {
            Some(index) => cables[index] = cable.clone(),
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

    fn elec_cable_list(&mut self, arguments: &str) -> Task<Message> {
        let Some(path) = cable_file() else {
            self.command_line
                .push_output("ELECCABLELIST: no project is loaded.");
            return Task::none();
        };
        let cables = read_cables(&path);
        if cables.is_empty() {
            self.command_line.push_output(
                "ELECCABLELIST: no cables yet — ELECCABLE <tag> CORES=<n> adds one.",
            );
            return Task::none();
        }
        // The columns are SkyCAD's cable-schedule set, so an existing company template can
        // be filled from the same data.
        let header = format!(
            "{:<10} {:<8} {:<18} {:<16} {:>8}  {:<30}",
            "CABLE", "CORES", "PART NUMBER", "MANUFACTURER", "LENGTH", "DESCRIPTION"
        );
        self.command_line.push_output(&header);
        for cable in &cables {
            let line = format!(
                "{:<10} {:<8} {:<18} {:<16} {:>8}  {:<30}",
                cable.tag,
                cable.cores,
                if cable.part.is_empty() { "-" } else { &cable.part },
                if cable.manufacturer.is_empty() {
                    "-"
                } else {
                    &cable.manufacturer
                },
                if cable.length.is_empty() {
                    "-".to_string()
                } else {
                    cable.length.clone()
                },
                cable.description
            );
            self.command_line.push_output(&line);
        }
        let out = arguments.trim();
        if !out.is_empty() {
            let mut text = String::from("Cable,Description,Part Number,Manufacturer,Length,Conductor\n");
            for cable in &cables {
                text.push_str(&format!(
                    "{},{},{},{},{},{}\n",
                    csv_field(&cable.tag),
                    csv_field(&cable.description),
                    csv_field(&cable.part),
                    csv_field(&cable.manufacturer),
                    csv_field(&cable.length),
                    csv_field(&cable.cores)
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
    let mut text = String::from("# AryaCAD cable schedule — one cable per row\ntag\tcores\tpart\tmanufacturer\tlength\tdescription\n");
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

//! Design by systems, project variants and option codes.
//!
//! Two configuration-management ideas from the products this project compares itself to:
//!
//! * a **system** is a reusable sub-circuit stored as a small text fragment under
//!   `library/systems/` — a DOL starter, a reversing starter. Inserting one places its
//!   symbols and wires at a point and retags the devices into the project's own numbering,
//!   so a starter drawn once can be dropped into every future project.
//! * a **variant** is an alternative sheet set for the same machine (pump set A or B). It
//!   lives as its own `.eprj` beside the master, and switching to it makes the project-wide
//!   commands (numbering, reports) work on that selection. An **option code** marks a device
//!   as belonging to an option (`ELECOPTION -K1 pump-set-B`).
//!
//! Commands:
//!   `ELECSYSTEM [LIST]`                 list the systems available
//!   `ELECSYSTEM INSERT <name> [x,y]`    place a system and retag its devices
//!   `ELECVARIANT NEW <name>`            copy the current project sheet set into a variant
//!   `ELECVARIANT LIST`                  list the variants beside the project
//!   `ELECVARIANT USE <name>`            make a variant the active project file
//!   `ELECOPTION <tag> <option>`         mark a device with an option code
//!   `ELECOPTION [LIST]`                 list the option codes
//!   `ELECOPTION CLEAR <tag>`            remove a device's option codes

use super::*;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// One line of a system fragment.
#[derive(Debug, Clone)]
enum SystemOp {
    Device { dx: f64, dy: f64, symbol: String },
    Wire { x1: f64, y1: f64, x2: f64, y2: f64 },
}

/// A named system: its operations, in file order.
struct System {
    name: String,
    ops: Vec<SystemOp>,
}

impl System {
    fn device_count(&self) -> usize {
        self.ops
            .iter()
            .filter(|op| matches!(op, SystemOp::Device { .. }))
            .count()
    }

    fn wire_count(&self) -> usize {
        self.ops
            .iter()
            .filter(|op| matches!(op, SystemOp::Wire { .. }))
            .count()
    }
}

impl OpenCADStudio {
    /// Dispatch the configuration-management commands. `None` when the command is not ours.
    pub(super) fn dispatch_elec_systems(&mut self, cmd: &str, i: usize) -> Option<Task<Message>> {
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
        if let Some(argument) = argument_of("ELECSYSTEM") {
            return Some(self.elec_system(i, &argument));
        }
        if let Some(argument) = argument_of("ELECVARIANT") {
            return Some(self.elec_variant(&argument));
        }
        if let Some(argument) = argument_of("ELECOPTION") {
            return Some(self.elec_option(&argument));
        }
        None
    }

    /// `ELECSYSTEM [LIST | INSERT <name> [x,y]]`
    fn elec_system(&mut self, i: usize, arguments: &str) -> Task<Message> {
        let systems = read_systems();
        let mut words = arguments.split_whitespace();
        let verb = words.next().map(|word| word.to_ascii_uppercase());
        match verb.as_deref() {
            None | Some("LIST") => {
                if systems.is_empty() {
                    self.command_line.push_output(
                        "ELECSYSTEM: no systems found — put <name>.tsv fragments in library/systems/.",
                    );
                    return Task::none();
                }
                let line = format!(
                    "ELECSYSTEM: {} system(s) available (library/systems).",
                    systems.len()
                );
                self.command_line.push_output(&line);
                for name in systems.keys() {
                    let system = &systems[name];
                    let detail = format!(
                        "  {:<20} {} device(s), {} wire(s)",
                        system.name,
                        system.device_count(),
                        system.wire_count()
                    );
                    self.command_line.push_output(&detail);
                }
                Task::none()
            }
            Some("INSERT") => {
                let Some(name) = words.next() else {
                    self.command_line
                        .push_output("ELECSYSTEM: use ELECSYSTEM INSERT <name> [x,y].");
                    return Task::none();
                };
                let system = match systems.get(&name.to_ascii_uppercase()) {
                    Some(system) => system,
                    None => {
                        let available = if systems.is_empty() {
                            "none".to_string()
                        } else {
                            systems.keys().cloned().collect::<Vec<_>>().join(", ")
                        };
                        let message = format!(
                            "ELECSYSTEM: no system named {name} (available: {available})."
                        );
                        self.command_line.push_output(&message);
                        return Task::none();
                    }
                };
                // `[x,y]` optionally names the insertion point; the default is the origin.
                let mut position = (0.0f64, 0.0f64);
                if let Some(point) = words.next() {
                    let mut parts = point.split(',');
                    if let (Some(x), Some(y)) = (parts.next(), parts.next()) {
                        if let (Ok(x), Ok(y)) = (x.trim().parse(), y.trim().parse()) {
                            position = (x, y);
                        }
                    }
                }
                self.insert_system(i, system, position)
            }
            Some(_) => {
                self.command_line.push_output(
                    "ELECSYSTEM: use ELECSYSTEM [LIST] or ELECSYSTEM INSERT <name> [x,y].",
                );
                Task::none()
            }
        }
    }

    /// Place a system's devices and wires, retagged into the drawing's own sequences.
    fn insert_system(&mut self, i: usize, system: &System, position: (f64, f64)) -> Task<Message> {
        // Existing tags decide the next free number, so two insertions do not collide.
        let mut existing: Vec<String> = Self::components_of(&self.tabs[i].scene.document)
            .iter()
            .map(|component| component.tag.trim().to_string())
            .filter(|tag| !tag.is_empty())
            .collect();
        let mut placed: Vec<String> = Vec::new();
        let mut wires = 0usize;
        let mut missing_blocks: Vec<String> = Vec::new();

        self.push_undo_snapshot(i, "ELECSYSTEM");
        for op in &system.ops {
            match op {
                SystemOp::Device { dx, dy, symbol } => {
                    let library = super::elec::library_symbol(symbol);
                    let class = library
                        .as_ref()
                        .map(|(class, _, _, _)| class.clone())
                        .unwrap_or_else(|| symbol.clone());
                    let prefix = ::elec::default_prefix(&class);
                    let refs: Vec<&str> = existing
                        .iter()
                        .chain(placed.iter())
                        .map(String::as_str)
                        .filter(|tag| tag.starts_with(prefix.as_str()))
                        .collect();
                    let tag = ::elec::next_tag(&refs, &class);
                    let at = codec::types::Vector3::new(
                        position.0 + dx,
                        position.1 + dy,
                        0.0,
                    );
                    match self.build_system_insert(i, symbol, at, &tag) {
                        Some(entity) => {
                            self.tabs[i].scene.add_entity(entity);
                            placed.push(tag);
                        }
                        None => {
                            if !missing_blocks.contains(symbol) {
                                missing_blocks.push(symbol.clone());
                            }
                        }
                    }
                }
                SystemOp::Wire { x1, y1, x2, y2 } => {
                    let mut line = codec::entities::Line::from_points(
                        codec::types::Vector3::new(position.0 + x1, position.1 + y1, 0.0),
                        codec::types::Vector3::new(position.0 + x2, position.1 + y2, 0.0),
                    );
                    line.common.layer = "WIRES".to_string();
                    self.tabs[i].scene.add_entity(codec::EntityType::Line(line));
                    wires += 1;
                }
            }
        }
        self.tabs[i].dirty = true;

        if placed.is_empty() && !missing_blocks.is_empty() {
            let message = format!(
                "ELECSYSTEM: cannot insert {} — symbols not in the drawing (load the library with ELECLIB): {}.",
                system.name,
                missing_blocks.join(", ")
            );
            self.command_line.push_output(&message);
            return Task::none();
        }
        let labels: Vec<String> = placed.iter().map(|tag| tag.clone()).collect();
        let mut summary = format!(
            "ELECSYSTEM: inserted {} at {:.2},{:.2} — {} device(s) ({}), {} wire(s).",
            system.name,
            position.0,
            position.1,
            labels.len(),
            labels.join(", "),
            wires
        );
        if !missing_blocks.is_empty() {
            summary.push_str(&format!(
                " Skipped {}: symbol not in the drawing.",
                missing_blocks.join(", ")
            ));
        }
        self.command_line.push_output(&summary);
        Task::none()
    }

    /// Build one system device insert, with its attributes and this run's tag.
    fn build_system_insert(
        &self,
        i: usize,
        symbol: &str,
        at: codec::types::Vector3,
        tag: &str,
    ) -> Option<codec::EntityType> {
        let block_exists = self.tabs[i]
            .scene
            .document
            .block_records
            .get(symbol)
            .is_some();
        if !block_exists {
            return None;
        }
        let definitions = self.elec_block_definitions(i, symbol);
        let mut insert = codec::entities::Insert::new(symbol, at);
        if definitions.is_empty() {
            // A library symbol may be plain geometry: use the attribute set the library
            // recorded for it, laid out below the symbol as ELECINSERT does.
            let attributes = super::elec::library_symbol(symbol)
                .map(|(_, attributes, _, _)| attributes)
                .unwrap_or_default();
            for (row, name) in attributes.iter().enumerate() {
                let mut attribute =
                    codec::entities::AttributeEntity::new(name.clone(), String::new());
                attribute.insertion_point = codec::types::Vector3::new(
                    at.x - 0.5,
                    at.y - 0.25 - row as f64 * 0.2,
                    0.0,
                );
                attribute.alignment_point = attribute.insertion_point;
                insert.attributes.push(attribute);
            }
        } else {
            for definition in definitions {
                let mut attribute =
                    codec::entities::AttributeEntity::from_definition(&definition, None);
                attribute.insertion_point.x += at.x;
                attribute.insertion_point.y += at.y;
                attribute.alignment_point.x += at.x;
                attribute.alignment_point.y += at.y;
                insert.attributes.push(attribute);
            }
        }
        for attribute in &mut insert.attributes {
            if is_device_tag(&attribute.tag) {
                attribute.value = tag.to_string();
            }
        }
        Some(codec::EntityType::Insert(insert))
    }

    /// `ELECVARIANT NEW <name> | LIST | USE <name>`
    fn elec_variant(&mut self, arguments: &str) -> Task<Message> {
        let Some(project) = super::elec::project_file()
            .read()
            .ok()
            .and_then(|guard| guard.clone())
        else {
            self.command_line.push_output(
                "ELECVARIANT: no project is loaded — ELECPROJNEW or ELECPROJOPEN first.",
            );
            return Task::none();
        };
        let Some(folder) = project.parent().map(Path::to_path_buf) else {
            self.command_line
                .push_output("ELECVARIANT: the project file has no folder.");
            return Task::none();
        };
        let mut words = arguments.split_whitespace();
        let verb = words.next().map(|word| word.to_ascii_uppercase());
        match verb.as_deref() {
            None | Some("LIST") => {
                let variants = list_variants(&folder);
                if variants.is_empty() {
                    self.command_line
                        .push_output("ELECVARIANT: no variants yet — ELECVARIANT NEW <name>.");
                    return Task::none();
                }
                let line = format!("ELECVARIANT: {} variant(s) beside the project.", variants.len());
                self.command_line.push_output(&line);
                for (name, path) in variants {
                    let sheets = std::fs::read_to_string(&path)
                        .map(|text| {
                            text.lines()
                                .filter(|line| line.starts_with("sheet "))
                                .count()
                        })
                        .unwrap_or(0);
                    let active = path == project;
                    let detail = format!(
                        "  {:<20} {} sheet(s){}",
                        name,
                        sheets,
                        if active { "  (active)" } else { "" }
                    );
                    self.command_line.push_output(&detail);
                }
                Task::none()
            }
            Some("NEW") => {
                let Some(name) = words.next() else {
                    self.command_line
                        .push_output("ELECVARIANT: use ELECVARIANT NEW <name>.");
                    return Task::none();
                };
                let path = variant_file(&folder, name);
                if path.exists() {
                    let message = format!(
                        "ELECVARIANT: {} already exists — edit it or pick another name.",
                        path.display()
                    );
                    self.command_line.push_output(&message);
                    return Task::none();
                }
                let source = std::fs::read_to_string(&project).unwrap_or_default();
                let mut text = format!(
                    "# AryaCAD electrical project variant {}\n# copied from {}\n",
                    name,
                    project
                        .file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or("project")
                );
                for line in source.lines() {
                    if line.starts_with('#') || line.trim().is_empty() {
                        continue;
                    }
                    text.push_str(line);
                    text.push('\n');
                }
                match std::fs::write(&path, text) {
                    Ok(()) => {
                        let sheets = source
                            .lines()
                            .filter(|line| line.starts_with("sheet "))
                            .count();
                        let message = format!(
                            "ELECVARIANT: created {} ({} sheet(s)); ELECVARIANT USE {name} switches to it.",
                            path.display(),
                            sheets
                        );
                        self.command_line.push_output(&message);
                    }
                    Err(error) => self.command_line.push_output(&format!(
                        "ELECVARIANT: cannot write {}: {error}",
                        path.display()
                    )),
                }
                Task::none()
            }
            Some("USE") => {
                let Some(name) = words.next() else {
                    self.command_line
                        .push_output("ELECVARIANT: use ELECVARIANT USE <name>.");
                    return Task::none();
                };
                let path = variant_file(&folder, name);
                if !path.is_file() {
                    let message = format!(
                        "ELECVARIANT: no variant {name} — ELECVARIANT LIST shows what exists."
                    );
                    self.command_line.push_output(&message);
                    return Task::none();
                }
                if let Ok(mut guard) = super::elec::project_file().write() {
                    *guard = Some(path.clone());
                }
                let message = format!(
                    "ELECVARIANT: switched the active project to {}.",
                    path.display()
                );
                self.command_line.push_output(&message);
                Task::none()
            }
            Some(_) => {
                self.command_line.push_output(
                    "ELECVARIANT: use ELECVARIANT NEW <name>, LIST or USE <name>.",
                );
                Task::none()
            }
        }
    }

    /// `ELECOPTION <tag> <option> | LIST | CLEAR <tag>`
    fn elec_option(&mut self, arguments: &str) -> Task<Message> {
        let Some(project) = super::elec::project_file()
            .read()
            .ok()
            .and_then(|guard| guard.clone())
        else {
            self.command_line
                .push_output("ELECOPTION: no project is loaded.");
            return Task::none();
        };
        let lines: Vec<String> = std::fs::read_to_string(&project)
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect();
        let mut words = arguments.split_whitespace();
        let first = words.next().map(str::to_string);
        let verb = first.as_deref().map(str::to_ascii_uppercase);
        match verb.as_deref() {
            None | Some("LIST") => {
                let options: Vec<&String> = lines
                    .iter()
                    .filter(|line| line.starts_with("option "))
                    .collect();
                if options.is_empty() {
                    self.command_line.push_output(
                        "ELECOPTION: no option codes yet — ELECOPTION <tag> <option>.",
                    );
                    return Task::none();
                }
                let line = format!("ELECOPTION: {} option code(s).", options.len());
                self.command_line.push_output(&line);
                for option in options {
                    let detail = format!("  {}", option.trim_start_matches("option "));
                    self.command_line.push_output(&detail);
                }
                Task::none()
            }
            Some("CLEAR") => {
                let Some(tag) = words.next() else {
                    self.command_line
                        .push_output("ELECOPTION: use ELECOPTION CLEAR <tag>.");
                    return Task::none();
                };
                let wanted = format!("option {}", tag);
                let before = lines.len();
                let kept: Vec<&String> = lines
                    .iter()
                    .filter(|line| !line.starts_with(&wanted))
                    .collect();
                let kept_count = kept.len();
                if kept.len() == before {
                    let message = format!("ELECOPTION: no option codes for {tag}.");
                    self.command_line.push_output(&message);
                    return Task::none();
                }
                let mut text = String::new();
                for line in kept {
                    text.push_str(line);
                    text.push('\n');
                }
                match std::fs::write(&project, text) {
                    Ok(()) => {
                        let message = format!(
                            "ELECOPTION: cleared {} option code(s) for {tag}.",
                            before - kept_count
                        );
                        self.command_line.push_output(&message);
                    }
                    Err(error) => self.command_line.push_output(&format!(
                        "ELECOPTION: cannot write {}: {error}",
                        project.display()
                    )),
                }
                Task::none()
            }
            _ => {
                let Some(tag) = first.as_deref() else {
                    self.command_line
                        .push_output("ELECOPTION: use ELECOPTION <tag> <option>.");
                    return Task::none();
                };
                let Some(option) = words.next() else {
                    self.command_line
                        .push_output("ELECOPTION: use ELECOPTION <tag> <option>.");
                    return Task::none();
                };
                let entry = format!("option {} {}", tag, option);
                if lines.iter().any(|line| line.trim() == entry) {
                    let message = format!("ELECOPTION: {tag} already has option {option}.");
                    self.command_line.push_output(&message);
                    return Task::none();
                }
                let mut text = String::new();
                for line in &lines {
                    text.push_str(line);
                    text.push('\n');
                }
                text.push_str(&entry);
                text.push('\n');
                match std::fs::write(&project, text) {
                    Ok(()) => {
                        let count = lines
                            .iter()
                            .filter(|line| line.starts_with(&format!("option {tag} ")))
                            .count()
                            + 1;
                        let message = format!(
                            "ELECOPTION: {tag} = {option} ({count} option code(s) on {tag})."
                        );
                        self.command_line.push_output(&message);
                    }
                    Err(error) => self.command_line.push_output(&format!(
                        "ELECOPTION: cannot write {}: {error}",
                        project.display()
                    )),
                }
                Task::none()
            }
        }
    }
}

/// True for the attributes that carry a device tag.
fn is_device_tag(tag: &str) -> bool {
    matches!(
        tag.trim().to_ascii_uppercase().as_str(),
        "TAG" | "TAG1" | "TAG2" | "TAGSTR" | "WORKID"
    )
}

/// Candidate folders a system may live in: the project's own first, then the library's.
fn system_folders() -> Vec<PathBuf> {
    let mut folders: Vec<PathBuf> = vec![PathBuf::from("library").join("systems")];
    if let Some(library) = super::elec::library_directory() {
        if let Some(parent) = library.parent() {
            folders.push(parent.join("systems"));
        }
        folders.push(library.join("systems"));
    }
    if let Some(project) = super::elec::project_dir_for_revisions() {
        // Last wins: a project's own systems override the library's.
        folders.push(project.join("systems"));
    }
    folders
}

/// Read every system fragment available, keyed by upper-case name.
fn read_systems() -> BTreeMap<String, System> {
    let mut systems: BTreeMap<String, System> = BTreeMap::new();
    for folder in system_folders() {
        let Ok(entries) = std::fs::read_dir(&folder) else {
            continue;
        };
        let mut paths: Vec<PathBuf> = entries
            .filter_map(|entry| entry.ok().map(|entry| entry.path()))
            .filter(|path| {
                path.extension()
                    .and_then(|extension| extension.to_str())
                    .map(|extension| extension.eq_ignore_ascii_case("tsv"))
                    .unwrap_or(false)
            })
            .collect();
        paths.sort();
        for path in paths {
            let Some(name) = path.file_stem().and_then(|stem| stem.to_str()) else {
                continue;
            };
            if let Ok(system) = read_system(&path) {
                // A project's own folder overrides the library's, so later folders win.
                systems.insert(name.to_ascii_uppercase(), system);
            }
        }
    }
    systems
}

fn read_system(path: &Path) -> Result<System, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    let name = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("SYSTEM")
        .to_ascii_uppercase();
    let mut ops = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut fields = line.split_whitespace();
        match fields.next() {
            Some("device") => {
                let numbers: Vec<f64> = fields
                    .by_ref()
                    .take(2)
                    .filter_map(|value| value.parse::<f64>().ok())
                    .collect();
                let Some(symbol) = fields.next() else {
                    continue;
                };
                if numbers.len() == 2 {
                    ops.push(SystemOp::Device {
                        dx: numbers[0],
                        dy: numbers[1],
                        symbol: symbol.to_string(),
                    });
                }
            }
            Some("wire") => {
                let numbers: Vec<f64> = fields
                    .filter_map(|value| value.parse::<f64>().ok())
                    .collect();
                if numbers.len() == 4 {
                    ops.push(SystemOp::Wire {
                        x1: numbers[0],
                        y1: numbers[1],
                        x2: numbers[2],
                        y2: numbers[3],
                    });
                }
            }
            _ => {}
        }
    }
    Ok(System { name, ops })
}

/// `variant-<name>.eprj` beside the master project, with a name safe for a file system.
fn variant_file(folder: &Path, name: &str) -> PathBuf {
    let safe: String = name
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '-' || character == '_' {
                character
            } else {
                '_'
            }
        })
        .collect();
    folder.join(format!("variant-{safe}.eprj"))
}

/// The variants beside a project, as (name, path) in name order.
fn list_variants(folder: &Path) -> Vec<(String, PathBuf)> {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut variants: Vec<(String, PathBuf)> = entries
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter_map(|path| {
            let stem = path.file_stem()?.to_str()?.to_string();
            let name = stem.strip_prefix("variant-")?.to_string();
            Some((name, path))
        })
        .collect();
    variants.sort();
    variants
}


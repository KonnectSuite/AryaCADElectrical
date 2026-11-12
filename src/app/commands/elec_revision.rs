//! Revision control for an electrical project: snapshot, list, compare, restore.
//!
//! SkyCAD locks a version of the whole project and lets you consult it later. This module
//! keeps that contract in the filesystem next to the project: a snapshot is a numbered
//! folder under `<project>\.rev\` holding a manifest and copies of every drawing and the
//! project file. Nothing is hidden in a database, so a snapshot can be copied to another
//! machine, inspected with a text editor, or restored by hand.
//!
//! Commands:
//!   `ELECREV`                     list snapshots (same as `ELECREV LIST`)
//!   `ELECREV SNAPSHOT [label]`    capture the project as it is now
//!   `ELECREV LIST`                list snapshots, newest last
//!   `ELECREV COMPARE <a> <b>`     what changed between two snapshots
//!   `ELECREV RESTORE <id>`        put a snapshot back (the current state is snapshotted first)

use super::*;
use std::path::{Path, PathBuf};

/// Extensions a revision captures: the drawings and the project file are the design; reports
/// and cache files are derived and are re-generated rather than versioned.
const CAPTURED_EXTENSIONS: [&str; 3] = ["dxf", "dwg", "eprj"];

impl OpenCADStudio {
    /// Dispatch the revision commands. Returns `None` when the command is not ours.
    pub(super) fn dispatch_elec_revision(&mut self, cmd: &str, _i: usize) -> Option<Task<Message>> {
        let command = cmd.trim();
        if !command.eq_ignore_ascii_case("ELECREV")
            && !command
                .to_ascii_uppercase()
                .starts_with("ELECREV ")
        {
            return None;
        }
        let rest = command
            .get("ELECREV".len()..)
            .unwrap_or("")
            .trim()
            .to_string();
        Some(self.elec_rev(&rest))
    }

    fn elec_rev(&mut self, arguments: &str) -> Task<Message> {
        let mut words = arguments.split_whitespace();
        let verb = words.next().unwrap_or("LIST").to_ascii_uppercase();
        let rest: Vec<&str> = words.collect();
        match verb.as_str() {
            "LIST" => self.elec_rev_list(),
            "SNAPSHOT" => {
                let label = rest.join(" ");
                self.elec_rev_snapshot(&label)
            }
            "COMPARE" => {
                if rest.len() < 2 {
                    self.rev_output("ELECREV: use ELECREV COMPARE <a> <b>.");
                    return Task::none();
                }
                self.elec_rev_compare(rest[0], rest[1])
            }
            "RESTORE" => {
                let Some(id) = rest.first() else {
                    self.rev_output("ELECREV: use ELECREV RESTORE <id>.");
                    return Task::none();
                };
                self.elec_rev_restore(id)
            }
            _ => {
                self.rev_output("ELECREV: use ELECREV [SNAPSHOT <label> | LIST | COMPARE <a> <b> | RESTORE <id>].");
                Task::none()
            }
        }
    }

    fn rev_output(&mut self, message: &str) {
        self.command_line.push_output(message);
    }
}

/// A revision snapshot on disk.
struct Revision {
    directory: PathBuf,
    /// Number parsed from the folder name, used for ordering.
    order: u32,
    name: String,
}

/// The project folder a revision lives in, or the reason there is none.
fn revision_root() -> Result<PathBuf, String> {
    let folder = super::elec::project_dir_for_revisions()
        .ok_or_else(|| "no project is loaded — ELECPROJNEW or ELECPROJOPEN first".to_string())?;
    Ok(folder.join(".rev"))
}

fn read_revisions(root: &Path) -> Vec<Revision> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(root) else {
        return out;
    };
    for entry in entries.flatten() {
        if !entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false) {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        let order = name
            .split('-')
            .next()
            .and_then(|prefix| prefix.parse::<u32>().ok())
            .unwrap_or(u32::MAX);
        out.push(Revision {
            directory: entry.path(),
            order,
            name,
        });
    }
    out.sort_by(|left, right| left.order.cmp(&right.order).then(left.name.cmp(&right.name)));
    out
}

/// Find a snapshot by the id `ELECREV LIST` prints or by the bare number people type.
///
/// A snapshot folder is named `<order><label>` (`002-A`), so `002`, `2` and `002-A` all mean
/// the same one. `COMPARE` and `RESTORE` share this rule so they never disagree.
fn find_revision<'a>(revisions: &'a [Revision], token: &str) -> Option<&'a Revision> {
    let token = token.trim();
    let wanted_order = token.parse::<u32>().ok();
    revisions
        .iter()
        .find(|revision| revision.name == token || wanted_order == Some(revision.order))
}

/// Snapshot files: `relative path \t bytes \t modified-unix`.
fn revision_manifest(directory: &Path) -> Vec<(String, u64, u64)> {
    let path = directory.join("manifest.tsv");
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    text.lines()
        .filter_map(|line| {
            let mut fields = line.split('\t');
            let name = fields.next()?.to_string();
            let bytes = fields.next()?.parse().ok()?;
            let modified = fields.next().unwrap_or("0").parse().unwrap_or(0);
            Some((name, bytes, modified))
        })
        .collect()
}

/// Files a snapshot of `folder` captures, sorted by name.
fn revision_files(folder: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = entries
        .flatten()
        .filter(|entry| entry.file_type().map(|kind| kind.is_file()).unwrap_or(false))
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .and_then(|extension| extension.to_str())
                .map(|extension| {
                    CAPTURED_EXTENSIONS
                        .iter()
                        .any(|known| known.eq_ignore_ascii_case(extension))
                })
                .unwrap_or(false)
        })
        .collect();
    files.sort();
    files
}

fn write_revision(
    root: &Path,
    folder: &Path,
    label: &str,
) -> Result<(String, usize, u64), String> {
    std::fs::create_dir_all(root).map_err(|error| format!("cannot create {}: {error}", root.display()))?;
    let existing = read_revisions(root);
    let next = existing.iter().map(|revision| revision.order).max().unwrap_or(0) + 1;
    let safe_label: String = label
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '-' || character == '_' {
                character
            } else {
                '_'
            }
        })
        .collect();
    let name = if safe_label.is_empty() {
        format!("{next:03}")
    } else {
        format!("{next:03}-{safe_label}")
    };
    let directory = root.join(&name);
    // A snapshot of nothing is not a revision: it burns the next order number and
    // leaves a `.rev` folder that `ELECREV RESTORE` would happily roll the project
    // back into. Say so before anything is written.
    let sources = revision_files(folder);
    if sources.is_empty() {
        return Err(format!(
            "no project files in {} to snapshot — save the sheets first",
            folder.display()
        ));
    }
    std::fs::create_dir_all(&directory)
        .map_err(|error| format!("cannot create {}: {error}", directory.display()))?;

    let mut lines = Vec::new();
    let mut total = 0u64;
    for file in sources {
        let Some(file_name) = file.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let bytes = std::fs::metadata(&file).map(|meta| meta.len()).unwrap_or(0);
        let modified = std::fs::metadata(&file)
            .and_then(|meta| meta.modified())
            .ok()
            .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|duration| duration.as_secs())
            .unwrap_or(0);
        std::fs::copy(&file, directory.join(file_name))
            .map_err(|error| format!("cannot copy {file_name}: {error}"))?;
        total += bytes;
        lines.push(format!("{file_name}\t{bytes}\t{modified}"));
    }
    std::fs::write(directory.join("manifest.tsv"), lines.join("\n"))
        .map_err(|error| format!("cannot write manifest: {error}"))?;
    Ok((name, lines.len(), total))
}

impl OpenCADStudio {
    fn elec_rev_snapshot(&mut self, label: &str) -> Task<Message> {
        let (root, folder) = match revision_root() {
            Ok(root) => {
                let folder = root
                    .parent()
                    .map(|parent| parent.to_path_buf())
                    .unwrap_or_else(|| PathBuf::from("."));
                (root, folder)
            }
            Err(reason) => {
                self.rev_output(&format!("ELECREV: {reason}."));
                return Task::none();
            }
        };
        match write_revision(&root, &folder, label) {
            Ok((name, files, bytes)) => {
                let message = format!(
                    "ELECREV: snapshot {name} — {files} file(s), {:.1} KB.",
                    bytes as f64 / 1024.0
                );
                self.command_line.push_output(&message);
            }
            Err(error) => {
                self.rev_output(&format!("ELECREV: {error}."));
            }
        }
        Task::none()
    }

    fn elec_rev_list(&mut self) -> Task<Message> {
        let root = match revision_root() {
            Ok(root) => root,
            Err(reason) => {
                self.rev_output(&format!("ELECREV: {reason}."));
                return Task::none();
            }
        };
        let revisions = read_revisions(&root);
        if revisions.is_empty() {
            self.rev_output("ELECREV: no snapshots yet — ELECREV SNAPSHOT <label>.");
            return Task::none();
        }
        self.rev_output(&format!("ELECREV: {} snapshot(s) in {}", revisions.len(), root.display()));
        for revision in &revisions {
            let manifest = revision_manifest(&revision.directory);
            let bytes: u64 = manifest.iter().map(|(_, size, _)| *size).sum();
            self.command_line.push_output(&format!(
                "  {}  {} file(s)  {:.1} KB",
                revision.name,
                manifest.len(),
                bytes as f64 / 1024.0
            ));
        }
        Task::none()
    }

    fn elec_rev_compare(&mut self, left: &str, right: &str) -> Task<Message> {
        let root = match revision_root() {
            Ok(root) => root,
            Err(reason) => {
                self.rev_output(&format!("ELECREV: {reason}."));
                return Task::none();
            }
        };
        let revisions = read_revisions(&root);
        let (Some(left_revision), Some(right_revision)) =
            (find_revision(&revisions, left), find_revision(&revisions, right))
        else {
            self.rev_output(&format!(
                "ELECREV: no snapshot {left} or {right} — ELECREV LIST shows the ids."
            ));
            return Task::none();
        };
        let left_manifest = revision_manifest(&left_revision.directory);
        let right_manifest = revision_manifest(&right_revision.directory);
        let mut changed = 0usize;
        let mut added = 0usize;
        let mut removed = 0usize;
        self.command_line.push_output(&format!(
            "ELECREV: {} -> {}",
            left_revision.name, right_revision.name
        ));
        for (name, size, _) in &right_manifest {
            match left_manifest.iter().find(|(other, _, _)| other == name) {
                Some((_, left_size, _)) if left_size == size => {}
                Some((_, left_size, _)) => {
                    changed += 1;
                    self.command_line
                        .push_output(&format!("  changed  {name}  {left_size} -> {size} bytes"));
                }
                None => {
                    added += 1;
                    self.command_line.push_output(&format!("  added    {name}  {size} bytes"));
                }
            }
        }
        for (name, size, _) in &left_manifest {
            if !right_manifest.iter().any(|(other, _, _)| other == name) {
                removed += 1;
                self.command_line
                    .push_output(&format!("  removed  {name}  was {size} bytes"));
            }
        }
        if changed + added + removed == 0 {
            self.command_line
                .push_output("  no differences (same file set and sizes)");
        }
        Task::none()
    }

    fn elec_rev_restore(&mut self, id: &str) -> Task<Message> {
        let root = match revision_root() {
            Ok(root) => root,
            Err(reason) => {
                self.rev_output(&format!("ELECREV: {reason}."));
                return Task::none();
            }
        };
        let folder = root
            .parent()
            .map(|parent| parent.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."));
        let revisions = read_revisions(&root);
        // `ELECREV LIST` shows the snapshot name with its label (`002-A`), and people also
        // type the bare number (`002` or `2`), so both spellings select the same snapshot.
        let Some(revision) = find_revision(&revisions, id) else {
            self.rev_output(&format!("ELECREV: no snapshot {id} — ELECREV LIST shows the ids."));
            return Task::none();
        };
        // Never destroy the current design on the way back: snapshot it first.
        let safety = write_revision(&root, &folder, "before-restore");
        let manifest = revision_manifest(&revision.directory);
        let mut restored = 0usize;
        let mut failed: Vec<String> = Vec::new();
        let mut reloaded: Vec<String> = Vec::new();
        for (name, _, _) in &manifest {
            let source = revision.directory.join(name);
            let target = folder.join(name);
            // Restore goes through the same staged-rename path as Save. A plain copy cannot
            // replace a drawing this session has open: the edit lease holds the file, and
            // reading it back leaves the old design on screen while the disk changed under
            // it, so an open drawing is also reloaded afterwards.
            match crate::io::replace_file_from_source(&source, &target) {
                Ok(_) => {
                    restored += 1;
                    if let Some(tab) = self.tab_showing(&target) {
                        match self.reload_tab_from_disk(tab) {
                            Ok(_) => reloaded.push(name.clone()),
                            Err(error) => self.command_line.push_output(&format!(
                                "ELECREV: restored {name} but could not reload it: {error}"
                            )),
                        }
                    }
                }
                Err(error) => {
                    failed.push(name.clone());
                    self.command_line
                        .push_output(&format!("ELECREV: could not replace {name}: {error}"));
                }
            }
        }
        // A restore that could not replace a file must not read as a success: reporting
        // "restored" would leave the user believing an older design is in place.
        if !failed.is_empty() {
            self.command_line.push_output(&format!(
                "ELECREV: restored {} of {} file(s) from {}; {} could not be replaced.",
                restored,
                manifest.len(),
                revision.name,
                failed.len()
            ));
            return Task::none();
        }
        let reloaded_text = if reloaded.is_empty() {
            String::new()
        } else {
            format!(" Reloaded {}.", reloaded.join(", "))
        };
        match safety {
            Ok((name, _, _)) => self.command_line.push_output(&format!(
                "ELECREV: restored {} file(s) from {} — the pre-restore state is snapshot {name}.{reloaded_text}",
                restored, revision.name
            )),
            Err(error) => self.command_line.push_output(&format!(
                "ELECREV: restored {restored} file(s) from {} but could not snapshot the previous state: {error}",
                revision.name
            )),
        }
        Task::none()
    }
}

#[cfg(test)]
mod revision_guard_tests {
    use super::*;

    /// A snapshot of a project folder that holds no sheets used to write
    /// `001\manifest.tsv` with nothing in it, burn the next order number, and leave
    /// a revision `ELECREV RESTORE` would roll the whole project back into.
    #[test]
    fn an_empty_project_folder_is_refused_before_anything_is_written() {
        let root = std::env::temp_dir().join(format!(
            "ocs-rev-guard-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|span| span.subsec_nanos())
                .unwrap_or(0)
        ));
        let folder = root.join("sheets");
        std::fs::create_dir_all(&folder).expect("temp folders");
        let revisions = root.join(".rev");

        let error = write_revision(&revisions, &folder, "qa")
            .expect_err("an empty folder has nothing to snapshot");
        assert!(error.contains("to snapshot"), "unexpected error: {error}");
        assert!(
            read_revisions(&revisions).is_empty(),
            "an empty snapshot must not burn an order number"
        );

        std::fs::remove_dir_all(&root).ok();
    }
}

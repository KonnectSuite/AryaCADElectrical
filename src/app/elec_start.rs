//! The front door: choose a drafting workspace or an electrical one, and, for
//! electrical, configure and create a new drawing set from the shipped IEC /
//! NEC templates.
//!
//! The dialog itself is drawn by [`crate::ui::window::start_hub`]; this module
//! owns the state it edits and the work behind `Create`: load each template,
//! fill its title block, save one DXF per sheet and write the project file.

use std::path::{Path, PathBuf};

use iced::Task;

use crate::app::Message;
use crate::app::OpenCADStudio;

/// Which step of the start dialog is showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HubStep {
    /// Drafting or electrical?
    #[default]
    ChooseMode,
    /// The electrical drawing-set wizard.
    Configure,
}

/// Which template family the new set uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EcadStandard {
    #[default]
    Iec,
    Nec,
}

impl EcadStandard {
    pub fn folder(self) -> &'static str {
        match self {
            Self::Iec => "iec",
            Self::Nec => "nec",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Iec => "IEC (ISO 5457, metric)",
            Self::Nec => "NEC (ASME Y14.1, ANSI)",
        }
    }

    pub fn sizes(self) -> &'static [TemplateSize] {
        match self {
            Self::Iec => &IEC_SIZES,
            Self::Nec => &NEC_SIZES,
        }
    }
}

/// One row of the size list.
#[derive(Debug, Clone, Copy)]
pub struct TemplateSize {
    /// Template file stem under `templates/<family>/`.
    pub file: &'static str,
    /// What the wizard and the title block call it.
    pub label: &'static str,
    pub width: f64,
    pub height: f64,
}

impl TemplateSize {
    pub fn dimensions(self) -> String {
        format!("{:.1} x {:.1} mm", self.width, self.height)
    }
}

const IEC_SIZES: [TemplateSize; 8] = [
    TemplateSize { file: "A4_landscape", label: "A4 (landscape)", width: 297.0, height: 210.0 },
    TemplateSize { file: "A4_portrait", label: "A4 (portrait)", width: 210.0, height: 297.0 },
    TemplateSize { file: "A3_landscape", label: "A3 (landscape)", width: 420.0, height: 297.0 },
    TemplateSize { file: "A3_portrait", label: "A3 (portrait)", width: 297.0, height: 420.0 },
    TemplateSize { file: "A2_landscape", label: "A2", width: 594.0, height: 420.0 },
    TemplateSize { file: "A1_landscape", label: "A1", width: 841.0, height: 594.0 },
    TemplateSize { file: "A0_landscape", label: "A0", width: 1189.0, height: 841.0 },
    TemplateSize { file: "B1_landscape", label: "B1", width: 1000.0, height: 707.0 },
];

const NEC_SIZES: [TemplateSize; 7] = [
    TemplateSize { file: "ANSI_A_letter", label: "ANSI A (Letter)", width: 279.4, height: 215.9 },
    TemplateSize { file: "ANSI_A_letter_portrait", label: "ANSI A (portrait)", width: 215.9, height: 279.4 },
    TemplateSize { file: "ANSI_B_tabloid", label: "ANSI B (ledger/tabloid)", width: 431.8, height: 279.4 },
    TemplateSize { file: "ANSI_C", label: "ANSI C", width: 558.8, height: 431.8 },
    TemplateSize { file: "ANSI_D", label: "ANSI D", width: 863.6, height: 558.8 },
    TemplateSize { file: "ANSI_E", label: "ANSI E", width: 1117.6, height: 863.6 },
    TemplateSize { file: "ANSI_Legal", label: "ANSI Legal", width: 355.6, height: 215.9 },
];

/// Which title-block field a text box edits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EcadField {
    Project,
    Title,
    Company,
    Drawing,
    DrawnBy,
    Sheets,
    Folder,
}

/// Everything the start dialog remembers between frames.
#[derive(Debug, Clone)]
pub struct StartHub {
    pub step: HubStep,
    pub standard: EcadStandard,
    pub size: usize,
    pub project: String,
    pub title: String,
    pub company: String,
    pub drawing: String,
    pub drawn_by: String,
    pub sheets: String,
    pub folder: String,
    pub error: Option<String>,
}

impl Default for StartHub {
    fn default() -> Self {
        Self {
            step: HubStep::ChooseMode,
            standard: EcadStandard::Iec,
            size: 2,
            project: String::new(),
            title: "Electrical schematic".to_string(),
            company: String::new(),
            drawing: "AR-0001".to_string(),
            drawn_by: String::new(),
            sheets: "1".to_string(),
            folder: default_folder(),
            error: None,
        }
    }
}

impl StartHub {
    pub fn size(&self) -> TemplateSize {
        let sizes = self.standard.sizes();
        sizes[self.size.min(sizes.len() - 1)]
    }

    pub fn set_standard(&mut self, standard: EcadStandard) {
        if self.standard != standard {
            self.standard = standard;
            self.size = 0;
        }
    }

    /// The template the wizard will copy, if it can be found beside the exe.
    pub fn template_path(&self) -> Option<PathBuf> {
        let root = template_root()?;
        let path = root.join(self.standard.folder()).join(format!("{}.dxf", self.size().file));
        path.is_file().then_some(path)
    }

    pub fn folder_path(&self) -> PathBuf {
        let text = self.folder.trim();
        if text.is_empty() {
            PathBuf::from(default_folder())
        } else {
            PathBuf::from(text)
        }
    }
}

/// Where a new set goes when the folder box is empty: the user's Documents
/// folder, else the per-user application folder. No probe here — `Create`
/// falls back to a writable place when Windows protects the chosen folder.
fn default_folder() -> String {
    if let Some(documents) = crate::sys::documents_dir() {
        return documents
            .join("AryaCAD")
            .join("Projects")
            .join("Project1")
            .to_string_lossy()
            .into_owned();
    }
    if let Some(config) = crate::config::config_dir() {
        return config.join("Projects").join("Project1").to_string_lossy().into_owned();
    }
    "Project1".to_string()
}

/// Fallback locations tried when the chosen folder is protected by Windows:
/// the per-user application folder, then the temp folder, then the current
/// directory. Duplicates of the requested folder are skipped.
fn fallback_folders(requested: &Path) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(config) = crate::config::config_dir() {
        candidates.push(config.join("Projects").join("Project1"));
    }
    candidates.push(std::env::temp_dir().join("AryaCAD").join("Project1"));
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join("Project1"));
    }
    candidates.retain(|path| path != requested);
    candidates
}

/// Create `requested`, or — if Windows denies access there — the first
/// writable fallback. Returns the folder actually used plus a note when the
/// requested folder could not be used.
fn prepare_folder(requested: &Path) -> Result<(PathBuf, Option<String>), String> {
    match std::fs::create_dir_all(requested) {
        Ok(()) => return Ok((requested.to_path_buf(), None)),
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
            let mut denied = error;
            for candidate in fallback_folders(requested) {
                match std::fs::create_dir_all(&candidate) {
                    Ok(()) => {
                        return Ok((
                            candidate.clone(),
                            Some(format!(
                                "{} is protected ({denied}); the set was created in {} instead.",
                                requested.display(),
                                candidate.display()
                            )),
                        ));
                    }
                    Err(error) => denied = error,
                }
            }
            Err(format!("cannot create {}: {denied}", requested.display()))
        }
        Err(error) => Err(format!("cannot create {}: {error}", requested.display())),
    }
}

/// Find the shipped `templates/` folder: beside the executable, a few levels up
/// from it (the build tree), or in / above the working directory (the repo
/// checkout). The portable folder and the repo both satisfy one of these.
pub fn template_root() -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            for ancestor in dir.ancestors().take(5) {
                candidates.push(ancestor.join("templates"));
            }
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        for ancestor in cwd.ancestors().take(5) {
            candidates.push(ancestor.join("templates"));
        }
    }
    candidates.into_iter().find(|path| path.join("iec").is_dir() && path.join("nec").is_dir())
}

/// A filename stem for the sheets: keep letters, digits, `-` and `_`.
fn stem(text: &str) -> String {
    let cleaned: String = text
        .trim()
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' { ch } else { '-' })
        .collect();
    let cleaned = cleaned.trim_matches('-').to_string();
    if cleaned.is_empty() { "sheet".to_string() } else { cleaned }
}

/// Create the drawing set described by `hub`: one DXF per sheet, titled, plus
/// the `project.eprj` that makes the folder an electrical project. The third
/// element is a note when Windows protected the requested folder and the set
/// went to a fallback location instead.
pub fn build_drawing_set(hub: &StartHub) -> Result<(PathBuf, Vec<PathBuf>, Option<String>), String> {
    let template = hub
        .template_path()
        .ok_or_else(|| format!("template {} / {} was not found beside the executable", hub.standard.label(), hub.size().label))?;
    let total: u32 = hub
        .sheets
        .trim()
        .parse()
        .map_err(|_| "sheet count must be a whole number".to_string())?;
    if !(1..=200).contains(&total) {
        return Err("sheet count must be between 1 and 200".to_string());
    }
    let (folder, note) = prepare_folder(&hub.folder_path())?;

    let stem = stem(&hub.drawing);
    let mut files = Vec::new();
    for number in 1..=total {
        let mut document = crate::io::load_file(&template)
            .map_err(|error| format!("cannot read {}: {error}", template.display()))?;
        patch_title_block(&mut document, hub, number, total);
        let path = folder.join(format!("{stem}-{number:02}.dxf"));
        match crate::io::save(&document, &path) {
            Ok(()) => files.push(path),
            // The folder was usable a moment ago but is not now (catalog policy,
            // AV, disk) — surface it rather than pretending the set is whole.
            Err(error) => return Err(format!("cannot write {}: {error}", path.display())),
        }
    }

    let project = folder.join("project.eprj");
    let mut text = String::from("# AryaCAD electrical project\n# one sheet per line, in drawing order\n");
    for file in &files {
        if let Some(name) = file.file_name().and_then(|name| name.to_str()) {
            text.push_str(&format!("sheet {name}\n"));
        }
    }
    std::fs::write(&project, text)
        .map_err(|error| format!("cannot write {}: {error}", project.display()))?;
    register_project(&project, &folder);
    Ok((project, files, note))
}

fn register_project(project: &Path, folder: &Path) {
    if let Ok(mut guard) = crate::app::commands::elec::project_file().write() {
        *guard = Some(project.to_path_buf());
    }
    if let Ok(mut guard) = crate::app::commands::elec::project_dir().write() {
        *guard = Some(folder.to_path_buf());
    }
}

/// Replace the placeholder strings the template generator wrote into the title
/// block, and add the free-text values beside their labels.
fn patch_title_block(
    document: &mut codec::Document,
    hub: &StartHub,
    sheet: u32,
    total: u32,
) {
    let mut edits: Vec<(codec::Handle, String)> = Vec::new();
    let mut extras: Vec<(String, f64, f64, f64)> = Vec::new();
    let company = hub.company.trim();
    let title = hub.title.trim();
    let project = hub.project.trim();
    let drawing = hub.drawing.trim();
    let drawn_by = hub.drawn_by.trim();

    for entity in document.entities() {
        let codec::EntityType::Text(text) = entity else {
            continue;
        };
        let value = text.value.trim().to_string();
        let point = text.insertion_point;
        let height = text.height;
        match value.as_str() {
            "COMPANY NAME" if !company.is_empty() => {
                edits.push((text.common.handle, company.to_string()));
            }
            "DRAWING TITLE" if !title.is_empty() => {
                edits.push((text.common.handle, title.to_string()));
            }
            "AR-0001" if !drawing.is_empty() => {
                edits.push((text.common.handle, drawing.to_string()));
            }
            "1 / 1" => {
                edits.push((text.common.handle, format!("{sheet} / {total}")));
            }
            "PROJECT" if !project.is_empty() => {
                // The caption sits at the top of its cell; the value goes under it with a
                // clear gap so the two never touch at any sheet size.
                extras.push((project.to_string(), point.x + 24.0, point.y - 3.6, 3.0));
            }
            "DRAWN" if !drawn_by.is_empty() => {
                extras.push((drawn_by.to_string(), point.x + 20.0, point.y - 3.0, 2.0));
            }
            _ => {}
        }
        // Keep the borrow checker honest about the fields we read above.
        let _ = height;
    }

    for (handle, value) in edits {
        if let Some(codec::EntityType::Text(text)) = document.get_entity_mut(handle) {
            text.value = value;
        }
    }
    for (value, x, y, height) in extras {
        let mut text = codec::entities::Text::new();
        text.value = value;
        text.height = height;
        text.insertion_point = codec::types::Vector3::new(x, y, 0.0);
        text.common.layer = "TITLE_BLOCK".to_string();
        let _ = document.add_entity(codec::EntityType::Text(text));
    }
}

impl OpenCADStudio {
    /// Open the start dialog at its first step (also reused by `ECADSTART`).
    pub(super) fn open_start_hub(&mut self) -> Task<Message> {
        self.start_hub.step = HubStep::ChooseMode;
        self.start_hub.error = None;
        self.active_modal = Some(crate::app::ModalKind::StartMode);
        self.reset_modal_geometry();
        Task::none()
    }

    /// `Create` in the wizard: build the set, load the first sheet, and report.
    pub(super) fn create_electrical_set(&mut self) -> Task<Message> {
        match build_drawing_set(&self.start_hub) {
            Ok((project, files, note)) => {
                self.start_hub.step = HubStep::Configure;
                self.start_hub.error = None;
                self.active_modal = None;
                let folder = project
                    .parent()
                    .map(|path| path.display().to_string())
                    .unwrap_or_default();
                // Remember where the set actually landed: if Windows protected the
                // requested folder this is the fallback, so the next wizard opens
                // on a folder that works.
                if !folder.is_empty() {
                    self.start_hub.folder = folder.clone();
                }
                if let Some(note) = note {
                    self.command_line.push_output(&note);
                }
                self.command_line.push_output(&format!(
                    "ECAD: {} sheet(s) created in {folder} (project {}).",
                    files.len(),
                    project.display()
                ));
                #[cfg(not(target_arch = "wasm32"))]
                if let Some(first) = files.first() {
                    // The wizard can be opened again while the first sheet is
                    // already loaded (Create twice, or ELECSTART from a running
                    // session). Opening it a second time would create a second
                    // editor on the same path and make saving fail with a lock
                    // error, so reuse the tab that already holds the file.
                    if let Some(index) = self.tabs.iter().position(|tab| {
                        tab.current_path.as_deref() == Some(first.as_path())
                    }) {
                        return self.update(Message::TabSwitch(index));
                    }
                    return self.update(Message::OpenRecent(first.clone()));
                }
                Task::none()
            }
            Err(error) => {
                self.start_hub.error = Some(error);
                Task::none()
            }
        }
    }

    /// `Drafting` in the mode chooser: start with a blank drawing.
    pub(super) fn start_drafting_session(&mut self) -> Task<Message> {
        self.active_modal = None;
        self.update(Message::TabNew)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_wizard_lists_both_families() {
        assert_eq!(EcadStandard::Iec.sizes().len(), 8);
        assert_eq!(EcadStandard::Nec.sizes().len(), 7);
        assert_eq!(EcadStandard::Iec.sizes()[2].label, "A3 (landscape)");
        assert_eq!(EcadStandard::Nec.sizes()[0].label, "ANSI A (Letter)");
    }

    #[test]
    fn a_drawing_set_is_written_with_titles_and_a_project_file() {
        let root = match template_root() {
            Some(root) => root,
            None => return, // templates are not beside this test binary; skip.
        };
        assert!(root.join("iec/A3_landscape.dxf").is_file());
        let mut hub = StartHub::default();
        hub.standard = EcadStandard::Iec;
        hub.size = 2;
        hub.project = "LINE 3".to_string();
        hub.company = "ACME".to_string();
        hub.drawing = "AR-0042".to_string();
        hub.drawn_by = "JD".to_string();
        hub.sheets = "2".to_string();
        let folder = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target/tmp")
            .join(format!("ecad_set_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        hub.folder = folder.to_string_lossy().into_owned();

        let (project, files, note) = build_drawing_set(&hub).expect("set builds");
        assert!(note.is_none(), "a writable folder must not fall back");
        assert_eq!(files.len(), 2);
        assert!(files[0].is_file() && files[1].is_file());
        let text = std::fs::read_to_string(&project).expect("project file");
        assert!(text.contains("sheet AR-0042-01.dxf"));
        assert!(text.contains("sheet AR-0042-02.dxf"));

        // The title block carries the typed values and the per-sheet number.
        let sheet = crate::io::load_file(&files[1]).expect("saved sheet loads");
        let mut titles = Vec::new();
        for entity in sheet.entities() {
            if let codec::EntityType::Text(text) = entity {
                titles.push(text.value.clone());
            }
        }
        assert!(titles.iter().any(|value| value == "AR-0042"));
        assert!(titles.iter().any(|value| value == "2 / 2"));
        assert!(titles.iter().any(|value| value == "ACME"));
        assert!(titles.iter().any(|value| value == "LINE 3"));
        let _ = std::fs::remove_dir_all(&folder);
    }
}

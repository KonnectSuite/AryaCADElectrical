use crate::modules::{IconKind, ModuleEvent, ToolDef};
pub const ICON: IconKind = IconKind::Svg(include_bytes!("../../../assets/icons/file_tabs.svg"));
pub fn tool() -> ToolDef {
    ToolDef {
        id: "FILETAB",
        label: "File\nTabs",
        icon: ICON,
        event: ModuleEvent::Command("FILETAB".to_string()),
    }
}

use crate::modules::{IconKind, ModuleEvent, ToolDef};
pub const ICON: IconKind = IconKind::Svg(include_bytes!("../../../assets/icons/cui_import.svg"));
pub fn tool() -> ToolDef {
    ToolDef {
        id: "CUIIMPORT",
        label: "Import",
        icon: ICON,
        event: ModuleEvent::Command("CUIIMPORT".to_string()),
    }
}

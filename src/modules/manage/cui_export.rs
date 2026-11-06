use crate::modules::{IconKind, ModuleEvent, ToolDef};
pub const ICON: IconKind = IconKind::Svg(include_bytes!("../../../assets/icons/cui_export.svg"));
pub fn tool() -> ToolDef {
    ToolDef {
        id: "CUIEXPORT",
        label: "Export",
        icon: ICON,
        event: ModuleEvent::Command("CUIEXPORT".to_string()),
    }
}

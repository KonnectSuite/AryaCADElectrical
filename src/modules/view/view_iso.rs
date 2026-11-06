use crate::modules::{IconKind, ModuleEvent, ToolDef};
pub const ICON: IconKind = IconKind::Svg(include_bytes!("../../../assets/icons/view_iso.svg"));
pub fn tool() -> ToolDef {
    ToolDef {
        id: "VIEW_ISO",
        label: "Iso",
        icon: ICON,
        event: ModuleEvent::Command("VIEW ISO".to_string()),
    }
}

use crate::modules::{IconKind, ModuleEvent, ToolDef};
pub const ICON: IconKind =
    IconKind::Svg(include_bytes!("../../../assets/icons/vports_restore.svg"));
pub fn tool() -> ToolDef {
    ToolDef {
        id: "VPORTS_RESTORE",
        label: "Restore",
        icon: ICON,
        event: ModuleEvent::Command("VPORTS".to_string()),
    }
}

use crate::modules::{IconKind, ModuleEvent, ToolDef};
pub const ICON: IconKind = IconKind::Svg(include_bytes!("../../../assets/icons/audit.svg"));
pub fn tool() -> ToolDef {
    ToolDef {
        id: "AUDIT",
        label: "Audit",
        icon: ICON,
        event: ModuleEvent::Command("AUDIT".to_string()),
    }
}

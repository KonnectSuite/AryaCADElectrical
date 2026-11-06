use crate::modules::{IconKind, ModuleEvent, ToolDef};
pub const ICON: IconKind = IconKind::Svg(include_bytes!("../../../assets/icons/purge.svg"));
pub fn tool() -> ToolDef {
    ToolDef {
        id: "PURGE",
        label: "Purge",
        icon: ICON,
        event: ModuleEvent::Command("PURGE".to_string()),
    }
}

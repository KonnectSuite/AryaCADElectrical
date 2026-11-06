use crate::modules::{IconKind, ModuleEvent, ToolDef};
pub const ICON: IconKind = IconKind::Svg(include_bytes!("../../../assets/icons/attedit.svg"));
pub fn tool() -> ToolDef {
    ToolDef {
        id: "ATTEDIT",
        label: "Edit\nAttribute",
        icon: ICON,
        event: ModuleEvent::Command("ATTEDIT".to_string()),
    }
}

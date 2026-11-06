use crate::modules::{IconKind, ModuleEvent, ToolDef};
pub const ICON: IconKind =
    IconKind::Svg(include_bytes!("../../../assets/icons/user_interface.svg"));
pub fn tool() -> ToolDef {
    ToolDef {
        id: "CUI",
        label: "User\nInterface",
        icon: ICON,
        event: ModuleEvent::Command("CUI".to_string()),
    }
}

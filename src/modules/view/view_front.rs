use crate::modules::{IconKind, ModuleEvent, ToolDef};
pub const ICON: IconKind = IconKind::Svg(include_bytes!("../../../assets/icons/view_front.svg"));
pub fn tool() -> ToolDef {
    ToolDef {
        id: "VIEW_FRONT",
        label: "Front",
        icon: ICON,
        event: ModuleEvent::Command("VIEW FRONT".to_string()),
    }
}

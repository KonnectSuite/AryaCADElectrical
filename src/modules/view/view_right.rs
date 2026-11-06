use crate::modules::{IconKind, ModuleEvent, ToolDef};
pub const ICON: IconKind = IconKind::Svg(include_bytes!("../../../assets/icons/view_right.svg"));
pub fn tool() -> ToolDef {
    ToolDef {
        id: "VIEW_RIGHT",
        label: "Right",
        icon: ICON,
        event: ModuleEvent::Command("VIEW RIGHT".to_string()),
    }
}

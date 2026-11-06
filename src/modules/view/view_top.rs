use crate::modules::{IconKind, ModuleEvent, ToolDef};
pub const ICON: IconKind = IconKind::Svg(include_bytes!("../../../assets/icons/view_top.svg"));
pub fn tool() -> ToolDef {
    ToolDef {
        id: "VIEW_TOP",
        label: "Top",
        icon: ICON,
        event: ModuleEvent::Command("VIEW TOP".to_string()),
    }
}

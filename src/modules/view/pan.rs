use crate::modules::{IconKind, ModuleEvent, ToolDef};
pub const ICON: IconKind = IconKind::Svg(include_bytes!("../../../assets/icons/pan.svg"));
pub fn tool() -> ToolDef {
    ToolDef {
        id: "PAN",
        label: "Pan",
        icon: ICON,
        event: ModuleEvent::Command("PAN".to_string()),
    }
}

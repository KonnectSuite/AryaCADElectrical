use crate::modules::{IconKind, ModuleEvent, ToolDef};
pub const ICON: IconKind = IconKind::Svg(include_bytes!("../../../assets/icons/properties.svg"));
pub fn tool() -> ToolDef {
    ToolDef {
        id: "PROPERTIES",
        label: "Properties",
        icon: ICON,
        event: ModuleEvent::Command("PROPERTIES".to_string()),
    }
}

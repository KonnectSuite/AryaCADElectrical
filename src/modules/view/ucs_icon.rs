use crate::modules::{IconKind, ModuleEvent, ToolDef};
pub const ICON: IconKind = IconKind::Svg(include_bytes!("../../../assets/icons/ucs_icon.svg"));
pub fn tool() -> ToolDef {
    ToolDef {
        id: "UCSICON",
        label: "UCS\nIcon",
        icon: ICON,
        event: ModuleEvent::Command("UCSICON".to_string()),
    }
}

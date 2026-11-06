use crate::modules::{IconKind, ModuleEvent, ToolDef};
pub const ICON: IconKind = IconKind::Svg(include_bytes!("../../../assets/icons/zoom_out.svg"));
pub fn tool() -> ToolDef {
    ToolDef {
        id: "ZOOM_OUT",
        label: "Zoom Out",
        icon: ICON,
        event: ModuleEvent::Command("ZOOM OUT".to_string()),
    }
}

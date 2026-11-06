use crate::modules::{IconKind, ModuleEvent, ToolDef};
pub const ICON: IconKind = IconKind::Svg(include_bytes!("../../../assets/icons/zoom_ext.svg"));
pub fn tool() -> ToolDef {
    ToolDef {
        id: "ZOOM_EXT",
        label: "Zoom\nExtents",
        icon: ICON,
        event: ModuleEvent::Command("ZOOM EXTENTS".to_string()),
    }
}

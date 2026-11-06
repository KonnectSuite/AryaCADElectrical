use crate::modules::{IconKind, ModuleEvent, ToolDef};
pub const ICON: IconKind = IconKind::Svg(include_bytes!("../../../assets/icons/tile_horiz.svg"));
pub fn tool() -> ToolDef {
    ToolDef {
        id: "HORIZONTAL",
        label: "Tile\nHoriz.",
        icon: ICON,
        event: ModuleEvent::Command("HORIZONTAL".to_string()),
    }
}

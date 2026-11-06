use crate::modules::{IconKind, ModuleEvent, ToolDef};
pub const ICON: IconKind = IconKind::Svg(include_bytes!("../../../assets/icons/tile_vert.svg"));
pub fn tool() -> ToolDef {
    ToolDef {
        id: "VERTICAL",
        label: "Tile\nVert.",
        icon: ICON,
        event: ModuleEvent::Command("VERTICAL".to_string()),
    }
}

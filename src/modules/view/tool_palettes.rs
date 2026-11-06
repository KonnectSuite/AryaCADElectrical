use crate::modules::{IconKind, ModuleEvent, ToolDef};
pub const ICON: IconKind = IconKind::Svg(include_bytes!("../../../assets/icons/tool_palettes.svg"));
pub fn tool() -> ToolDef {
    ToolDef {
        id: "TOOLPALETTES",
        label: "Tool\nPalettes",
        icon: ICON,
        event: ModuleEvent::Command("TOOLPALETTES".to_string()),
    }
}

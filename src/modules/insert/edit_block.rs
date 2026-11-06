use crate::modules::{IconKind, ModuleEvent, ToolDef};
pub const ICON: IconKind = IconKind::Svg(include_bytes!("../../../assets/icons/edit_block.svg"));
pub fn tool() -> ToolDef {
    ToolDef {
        id: "BEDIT",
        label: "Edit Block",
        icon: ICON,
        event: ModuleEvent::Command("BEDIT".to_string()),
    }
}

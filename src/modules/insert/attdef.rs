use crate::modules::{IconKind, ModuleEvent, ToolDef};
pub const ICON: IconKind = IconKind::Svg(include_bytes!("../../../assets/icons/attdef.svg"));
pub fn tool() -> ToolDef {
    ToolDef {
        id: "ATTDEF",
        label: "Define\nAttributes",
        icon: ICON,
        event: ModuleEvent::Command("ATTDEF".to_string()),
    }
}

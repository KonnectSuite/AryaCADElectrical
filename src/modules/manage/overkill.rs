use crate::modules::{IconKind, ModuleEvent, ToolDef};
pub const ICON: IconKind = IconKind::Svg(include_bytes!("../../../assets/icons/overkill.svg"));
pub fn tool() -> ToolDef {
    ToolDef {
        id: "OVERKILL",
        label: "Overkill",
        icon: ICON,
        event: ModuleEvent::Command("OVERKILL".to_string()),
    }
}

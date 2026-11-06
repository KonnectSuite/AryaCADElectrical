use crate::modules::{IconKind, ModuleEvent, ToolDef};
pub const ICON: IconKind = IconKind::Svg(include_bytes!("../../../assets/icons/vports_config.svg"));
pub fn tool() -> ToolDef {
    ToolDef {
        id: "VPORTS",
        label: "Viewport\nConfiguration",
        icon: ICON,
        event: ModuleEvent::Command("VPORTS".to_string()),
    }
}

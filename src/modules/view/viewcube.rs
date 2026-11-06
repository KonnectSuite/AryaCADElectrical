use crate::modules::{IconKind, ModuleEvent, ToolDef};
pub const ICON: IconKind = IconKind::Svg(include_bytes!("../../../assets/icons/viewcube.svg"));
pub fn tool() -> ToolDef {
    ToolDef {
        id: "NAVVCUBE",
        label: "View\nCube",
        icon: ICON,
        event: ModuleEvent::Command("NAVVCUBE".to_string()),
    }
}

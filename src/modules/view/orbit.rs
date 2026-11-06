use crate::modules::{IconKind, ModuleEvent, ToolDef};
pub const ICON: IconKind = IconKind::Svg(include_bytes!("../../../assets/icons/orbit.svg"));
pub fn tool() -> ToolDef {
    ToolDef {
        id: "3DORBIT",
        label: "3D Orbit",
        icon: ICON,
        event: ModuleEvent::Command("3DORBIT".to_string()),
    }
}

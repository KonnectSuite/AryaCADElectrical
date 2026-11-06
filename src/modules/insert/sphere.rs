use crate::modules::{IconKind, ModuleEvent, ToolDef};
pub fn tool() -> ToolDef {
    ToolDef {
        id: "SPHERE",
        label: "Sphere",
        icon: IconKind::Svg(include_bytes!("../../../assets/icons/sphere3d.svg")),
        event: ModuleEvent::Command("SPHERE".to_string()),
    }
}

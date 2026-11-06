use crate::modules::{IconKind, ModuleEvent, ToolDef};
pub fn tool() -> ToolDef {
    ToolDef {
        id: "CYLINDER",
        label: "Cylinder",
        icon: IconKind::Svg(include_bytes!("../../../assets/icons/cylinder3d.svg")),
        event: ModuleEvent::Command("CYLINDER".to_string()),
    }
}

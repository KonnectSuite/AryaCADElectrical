use crate::modules::{IconKind, ModuleEvent, ToolDef};
pub fn tool() -> ToolDef {
    ToolDef {
        id: "BOX",
        label: "Box",
        icon: IconKind::Svg(include_bytes!("../../../assets/icons/box3d.svg")),
        event: ModuleEvent::Command("BOX".to_string()),
    }
}

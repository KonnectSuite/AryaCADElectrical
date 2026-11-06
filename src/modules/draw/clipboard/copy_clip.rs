use crate::modules::{IconKind, ModuleEvent, ToolDef};

pub fn tool() -> ToolDef {
    ToolDef {
        id: "COPYCLIP",
        label: "Copy",
        icon: IconKind::Svg(include_bytes!("../../../../assets/icons/copy_clip.svg")),
        event: ModuleEvent::Command("COPYCLIP".to_string()),
    }
}

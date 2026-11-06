use crate::modules::{IconKind, ModuleEvent, ToolDef};

pub fn tool() -> ToolDef {
    ToolDef {
        id: "CUTCLIP",
        label: "Cut",
        icon: IconKind::Svg(include_bytes!("../../../../assets/icons/cut.svg")),
        event: ModuleEvent::Command("CUTCLIP".to_string()),
    }
}

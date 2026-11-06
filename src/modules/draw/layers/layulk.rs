use crate::modules::{IconKind, ModuleEvent, ToolDef};

pub fn tool() -> ToolDef {
    ToolDef {
        id: "LAYULK",
        label: "Layer Unlock",
        icon: IconKind::Svg(include_bytes!("../../../../assets/icons/layers/layulk.svg")),
        event: ModuleEvent::Command("LAYULK".to_string()),
    }
}

use crate::modules::{IconKind, ModuleEvent, ToolDef};

pub fn tool() -> ToolDef {
    ToolDef {
        id: "LAYFRZ",
        label: "Layer Freeze",
        icon: IconKind::Svg(include_bytes!("../../../../assets/icons/layers/layfrz.svg")),
        event: ModuleEvent::Command("LAYFRZ".to_string()),
    }
}

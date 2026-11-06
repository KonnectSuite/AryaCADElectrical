use crate::modules::{IconKind, ModuleEvent, ToolDef};

pub fn tool() -> ToolDef {
    ToolDef {
        id: "LAYON",
        label: "Turn All Layers On",
        icon: IconKind::Svg(include_bytes!("../../../../assets/icons/layers/layon.svg")),
        event: ModuleEvent::Command("LAYON".to_string()),
    }
}

// ByLayer tool — ribbon definition.

use crate::modules::{IconKind, ModuleEvent, ToolDef};

pub fn tool() -> ToolDef {
    ToolDef {
        id: "BYLAYER",
        label: "ByLayer",
        icon: IconKind::Svg(include_bytes!("../../../../assets/icons/bylayer.svg")),
        event: ModuleEvent::Command("BYLAYER".to_string()),
    }
}

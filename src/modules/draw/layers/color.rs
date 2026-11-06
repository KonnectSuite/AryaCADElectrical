// Color tool — ribbon definition.

use crate::modules::{IconKind, ModuleEvent, ToolDef};

#[allow(dead_code)]
pub fn tool() -> ToolDef {
    ToolDef {
        id: "COLOR",
        label: "Color",
        icon: IconKind::Svg(include_bytes!("../../../../assets/icons/color_palette.svg")),
        event: ModuleEvent::Command("COLOR".to_string()),
    }
}

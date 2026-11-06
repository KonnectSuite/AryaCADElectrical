// Delete tool — ribbon definition.
//
// Fires the ERASE command (same as pressing Delete key or typing ERASE/E).
// Shown as "Delete" in the ribbon so the label matches familiar terminology.

use crate::modules::{IconKind, ModuleEvent, ToolDef};

pub fn tool() -> ToolDef {
    ToolDef {
        id: "ERASE",
        label: "Delete",
        icon: IconKind::Svg(include_bytes!("../../../../assets/icons/erase.svg")),
        event: ModuleEvent::Command("ERASE".to_string()),
    }
}

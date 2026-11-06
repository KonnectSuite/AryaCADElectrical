use crate::modules::{IconKind, ModuleEvent, ToolDef};
pub fn tool() -> ToolDef {
    ToolDef {
        id: "CLEAR",
        label: "Clear",
        icon: IconKind::Svg(include_bytes!("../../../assets/icons/clear_tool.svg")),
        event: ModuleEvent::ClearModels,
    }
}

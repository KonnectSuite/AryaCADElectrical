use crate::modules::{IconKind, ModuleEvent, ToolDef};
pub const ICON: IconKind = IconKind::Svg(include_bytes!("../../../assets/icons/cascade.svg"));
pub fn tool() -> ToolDef {
    ToolDef {
        id: "CASCADE",
        label: "Cascade",
        icon: ICON,
        event: ModuleEvent::Command("CASCADE".to_string()),
    }
}

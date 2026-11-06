use crate::modules::{IconKind, ModuleEvent, ToolDef};
pub const ICON: IconKind = IconKind::Svg(include_bytes!("../../../assets/icons/design_center.svg"));
pub fn tool() -> ToolDef {
    ToolDef {
        id: "ADCENTER",
        label: "Design\nCenter",
        icon: ICON,
        event: ModuleEvent::Command("ADCENTER".to_string()),
    }
}

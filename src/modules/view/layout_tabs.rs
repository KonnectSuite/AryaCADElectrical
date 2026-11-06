use crate::modules::{IconKind, ModuleEvent, ToolDef};
pub const ICON: IconKind = IconKind::Svg(include_bytes!("../../../assets/icons/layout_tabs.svg"));
pub fn tool() -> ToolDef {
    ToolDef {
        id: "LAYOUTTAB",
        label: "Layout\nTabs",
        icon: ICON,
        event: ModuleEvent::Command("LAYOUTTAB".to_string()),
    }
}

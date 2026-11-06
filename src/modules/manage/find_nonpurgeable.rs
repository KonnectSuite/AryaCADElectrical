use crate::modules::{IconKind, ModuleEvent, ToolDef};
pub const ICON: IconKind = IconKind::Svg(include_bytes!(
    "../../../assets/icons/find_nonpurgeable.svg"
));
pub fn tool() -> ToolDef {
    ToolDef {
        id: "FINDNONPURGEABLE",
        label: "Find Non-\nPurgeable Items",
        icon: ICON,
        event: ModuleEvent::Command("FINDNONPURGEABLE".to_string()),
    }
}

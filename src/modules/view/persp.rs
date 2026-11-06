use crate::modules::{IconKind, ModuleEvent, ToolDef};
pub const ICON: IconKind = IconKind::Svg(include_bytes!("../../../assets/icons/persp.svg"));
pub fn tool() -> ToolDef {
    ToolDef {
        id: "PERSP",
        label: "Persp",
        icon: ICON,
        event: ModuleEvent::Command("PERSP".into()),
    }
}

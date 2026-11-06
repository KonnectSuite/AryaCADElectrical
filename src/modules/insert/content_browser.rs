use crate::modules::{IconKind, ModuleEvent, ToolDef};
pub const ICON: IconKind =
    IconKind::Svg(include_bytes!("../../../assets/icons/content_browser.svg"));
pub fn tool() -> ToolDef {
    ToolDef {
        id: "CONTENTBROWSER",
        label: "Content\nBrowser",
        icon: ICON,
        event: ModuleEvent::Command("CONTENTBROWSER".to_string()),
    }
}

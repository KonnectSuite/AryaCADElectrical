use crate::modules::{IconKind, ModuleEvent, ToolDef};

pub fn tool() -> ToolDef {
    ToolDef {
        id: "LAYMCUR",
        label: "Make Current",
        icon: IconKind::Svg(include_bytes!(
            "../../../../assets/icons/layers/laymcur.svg"
        )),
        event: ModuleEvent::Command("LAYMCUR".to_string()),
    }
}

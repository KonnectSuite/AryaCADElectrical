//! Click-to-place for library symbols.
//!
//! The palette can only say *what* to insert; this command asks *where*. Everything that
//! needs the app - the library's attribute template, the next free tag, the catalogue -
//! is resolved before the command starts, so the command itself stays a plain value that
//! commits one entity when a point is picked.

use codec::types::Vector3;
use codec::EntityType;
use glam::DVec3;

use crate::command::{CadCommand, CmdResult};

/// One attribute to write on the new instance: tag and value.
#[derive(Debug, Clone)]
pub struct PlacedAttribute {
    pub tag: String,
    pub value: String,
    /// Where the block's attribute definition sits, in symbol units. When present the
    /// attribute is placed there (scaled with the insert) instead of stacked under the
    /// insertion point - that is what keeps a custom module's pin attributes on its edges.
    pub offset: Option<(f64, f64)>,
}

pub struct PlaceSymbolCommand {
    block: String,
    attributes: Vec<PlacedAttribute>,
    /// Text height for the attributes, taken from the symbol size.
    height: f64,
    /// Uniform scale the symbol is drawn at on the sheet.
    scale: f64,
    /// Set once a point has been picked, so a second click places another instance - the
    /// same behaviour as INSERT, and what a user expects when placing a row of contacts.
    placed: usize,
}

impl PlaceSymbolCommand {
    pub fn new(
        block: String,
        attributes: Vec<PlacedAttribute>,
        height: f64,
        scale: f64,
    ) -> Self {
        Self {
            block,
            attributes,
            height,
            scale,
            placed: 0,
        }
    }

    fn build(&self, at: DVec3) -> EntityType {
        let position = Vector3::new(at.x, at.y, at.z);
        let insert = codec::entities::Insert::new(self.block.clone(), position)
            .with_uniform_scale(self.scale);
        let mut insert = insert;
        let step = self.height * 1.25;
        for (row, attribute) in self.attributes.iter().enumerate() {
            let mut entity =
                codec::entities::AttributeEntity::new(attribute.tag.clone(), attribute.value.clone());
            entity.height = self.height;
            entity.insertion_point = match attribute.offset {
                Some((x, y)) => Vector3::new(
                    at.x + x * self.scale,
                    at.y + y * self.scale,
                    at.z,
                ),
                None => Vector3::new(at.x, at.y - self.height - row as f64 * step, at.z),
            };
            entity.alignment_point = entity.insertion_point;
            insert.attributes.push(entity);
        }
        EntityType::Insert(insert)
    }
}

impl CadCommand for PlaceSymbolCommand {
    fn name(&self) -> &'static str {
        "ELECPLACE"
    }

    fn prompt(&self) -> String {
        if self.placed == 0 {
            format!("{}  Specify insertion point:", self.block)
        } else {
            format!(
                "{} placed. Specify insertion point for another  [Enter to finish]:",
                self.block
            )
        }
    }

    fn on_point(&mut self, point: DVec3) -> CmdResult {
        self.placed += 1;
        CmdResult::CommitAndExit(self.build(point))
    }

    fn on_enter(&mut self) -> CmdResult {
        CmdResult::Done
    }

}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pick_commits_an_insert_with_its_attributes() {
        let mut command = PlaceSymbolCommand::new(
            "HCR1".to_string(),
            vec![
                PlacedAttribute {
                    tag: "TAG1".to_string(),
                    value: "-K7".to_string(),
                    offset: None,
                },
                PlacedAttribute {
                    tag: "CAT".to_string(),
                    value: "3RT".to_string(),
                    offset: None,
                },
            ],
            2.5,
            1.0,
        );
        match command.on_point(DVec3::new(10.0, 20.0, 0.0)) {
            CmdResult::CommitAndExit(EntityType::Insert(insert)) => {
                assert_eq!(insert.block_name, "HCR1");
                assert_eq!(insert.insert_point.x, 10.0);
                assert_eq!(insert.insert_point.y, 20.0);
                assert_eq!(insert.attributes.len(), 2);
                assert_eq!(insert.attributes[0].tag, "TAG1");
                assert_eq!(insert.attributes[0].value, "-K7");
                assert_eq!(insert.attributes[1].tag, "CAT");
            }
            // `CmdResult` deliberately carries no `Debug` (it embeds large scene values),
            // so the failure arm reports the variant by name instead of formatting it.
            _ => panic!("expected a CommitAndExit(Insert) from a picked point"),
        }
        assert!(command.prompt().contains("placed"));
    }

    #[test]
    fn definition_offsets_are_scaled_with_the_symbol() {
        let mut command = PlaceSymbolCommand::new(
            "IO16".to_string(),
            vec![
                PlacedAttribute {
                    tag: "TAG1".to_string(),
                    value: "-A1".to_string(),
                    offset: None,
                },
                PlacedAttribute {
                    tag: "X1TERM01".to_string(),
                    value: String::new(),
                    offset: Some((2.0, 1.0)),
                },
            ],
            0.4,
            25.4,
        );
        match command.on_point(DVec3::new(10.0, 20.0, 0.0)) {
            CmdResult::CommitAndExit(EntityType::Insert(insert)) => {
                let pin = &insert.attributes[1];
                assert_eq!(pin.tag, "X1TERM01");
                assert!((pin.insertion_point.x - (10.0 + 2.0 * 25.4)).abs() < 1e-9);
                assert!((pin.insertion_point.y - (20.0 + 1.0 * 25.4)).abs() < 1e-9);
            }
            _ => panic!("expected a CommitAndExit(Insert) from a picked point"),
        }
    }

    #[test]
    fn finishing_on_enter_leaves_the_command() {
        let mut command = PlaceSymbolCommand::new("X".to_string(), Vec::new(), 2.5, 1.0);
        assert!(matches!(command.on_enter(), CmdResult::Done));
    }
}






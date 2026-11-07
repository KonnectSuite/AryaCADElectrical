//! Electrical setup dialog: the numbering contract `ELECNUMBER` and
//! `ELECWIRENUMBER` follow on this drawing.
//!
//! AutoCAD Electrical keeps the contract in a `WD_M` block, and that block still
//! wins whenever it exists. A drawing created in this application has no `WD_M`
//! block, so the dialog stores the same values as custom properties (`ELEC_*`)
//! and `read_setup` falls back to them.

use crate::app::Message;
use crate::ui::modal::ModalSizing;
use crate::ui::style::common::muted_style;
use crate::ui::style::form::{dialog_button, hdivider, labeled_field, labeled_pick_list};
use iced::widget::{column, container, row, text, Space};
use iced::{Background, Border, Element, Length};

/// What the dialog keeps between frames.
#[derive(Debug, Clone, Default)]
pub struct State {
    pub tag_format: String,
    pub tag_mode: String,
    pub tag_start: String,
    pub tag_suffixes: String,
    pub wire_format: String,
    pub wire_mode: String,
    pub wire_start: String,
    pub xref_format: String,
    pub sheet: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub enum SetupMsg {
    TagFormat(String),
    TagMode(String),
    TagStart(String),
    TagSuffixes(String),
    WireFormat(String),
    WireMode(String),
    WireStart(String),
    XrefFormat(String),
    Sheet(String),
    Ok,
}

/// The mode pick list shows the letter with its meaning; `mode_letter` maps the
/// chosen row back to the single letter ACADE stores.
pub fn mode_options() -> Vec<String> {
    vec![
        "C - continuous".to_string(),
        "S - per sheet".to_string(),
        "R - fixed (keep existing)".to_string(),
    ]
}

pub fn mode_label(mode: &str) -> String {
    let letter = mode.trim().chars().next().unwrap_or('S');
    mode_options()
        .into_iter()
        .find(|option| option.starts_with(letter.to_ascii_uppercase()))
        .unwrap_or_else(|| "S - per sheet".to_string())
}

pub fn mode_letter(label: &str) -> String {
    match label.trim().chars().next() {
        Some(letter) if letter.is_ascii_alphabetic() => {
            letter.to_ascii_uppercase().to_string()
        }
        _ => "S".to_string(),
    }
}

pub(crate) fn view(state: &State, sizing: ModalSizing) -> Element<'_, Message> {
    let field_width = 200.0;
    let preset_width = Length::Fill;

    let tag_presets = vec![
        "%F%N".to_string(),
        "%N".to_string(),
        "%F%N%S".to_string(),
        "-%N".to_string(),
    ];
    let devices = column![
        text("Devices (tags)").size(12),
        labeled_field(
            "Format".into(),
            &state.tag_format,
            |value| Message::ElecSetup(SetupMsg::TagFormat(value)),
            field_width,
        ),
        labeled_pick_list(
            "Preset".into(),
            tag_presets,
            None::<String>,
            |value| Message::ElecSetup(SetupMsg::TagFormat(value)),
            preset_width,
        ),
        labeled_pick_list(
            "Mode".into(),
            mode_options(),
            Some(state.tag_mode.clone()),
            |value| Message::ElecSetup(SetupMsg::TagMode(value)),
            preset_width,
        ),
        labeled_field(
            "Start".into(),
            &state.tag_start,
            |value| Message::ElecSetup(SetupMsg::TagStart(value)),
            field_width,
        ),
        labeled_field(
            "Suffixes".into(),
            &state.tag_suffixes,
            |value| Message::ElecSetup(SetupMsg::TagSuffixes(value)),
            field_width,
        ),
        text("Comma-separated per-sheet suffixes, e.g. A,B.")
            .size(10)
            .style(muted_style),
    ]
    .spacing(6)
    .width(Length::FillPortion(1));

    let wire_presets = vec![
        "%N".to_string(),
        "%N%S".to_string(),
        "%F%N".to_string(),
        "W%N".to_string(),
    ];
    let wires = column![
        text("Wires").size(12),
        labeled_field(
            "Format".into(),
            &state.wire_format,
            |value| Message::ElecSetup(SetupMsg::WireFormat(value)),
            field_width,
        ),
        labeled_pick_list(
            "Preset".into(),
            wire_presets,
            None::<String>,
            |value| Message::ElecSetup(SetupMsg::WireFormat(value)),
            preset_width,
        ),
        labeled_pick_list(
            "Mode".into(),
            mode_options(),
            Some(state.wire_mode.clone()),
            |value| Message::ElecSetup(SetupMsg::WireMode(value)),
            preset_width,
        ),
        labeled_field(
            "Start".into(),
            &state.wire_start,
            |value| Message::ElecSetup(SetupMsg::WireStart(value)),
            field_width,
        ),
        text("%F is the drawing family, %S the sheet, %N the next number.")
            .size(10)
            .style(muted_style),
    ]
    .spacing(6)
    .width(Length::FillPortion(1));

    let xref_presets = vec!["%N".to_string(), "%S:%N".to_string()];
    let cross_references = column![
        text("Cross references").size(12),
        row![
            labeled_pick_list(
                "Format".into(),
                xref_presets,
                Some(state.xref_format.clone()),
                |value| Message::ElecSetup(SetupMsg::XrefFormat(value)),
                Length::Fixed(220.0),
            ),
            labeled_field(
                "Sheet".into(),
                &state.sheet,
                |value| Message::ElecSetup(SetupMsg::Sheet(value)),
                160.0,
            ),
        ]
        .spacing(14),
    ]
    .spacing(6);

    let hint = text(
        "Stored on this drawing. A drawing that carries an AutoCAD Electrical WD_M block keeps using it.",
    )
    .size(10)
    .style(muted_style);

    let error: Element<'_, Message> = match &state.error {
        Some(message) => text(message.clone()).size(11).into(),
        None => Space::new().height(Length::Fixed(0.0)).into(),
    };

    let body = column![
        text("Electrical setup").size(15),
        hint,
        hdivider(Length::Fill),
        row![devices, wires].spacing(24),
        hdivider(Length::Fill),
        cross_references,
        error,
        row![
            Space::new().width(Length::Fill),
            dialog_button("Cancel", Message::CloseModal, false),
            dialog_button("Save", Message::ElecSetup(SetupMsg::Ok), true),
        ]
        .spacing(8),
    ]
    .spacing(10);

    container(body)
        .padding(16)
        .width(sizing.width)
        .height(sizing.height)
        .style(|theme| container::Style {
            background: Some(Background::Color(theme.palette().background.base.color)),
            border: Border {
                color: theme.palette().background.neutral.color,
                width: 1.0,
                radius: 8.0.into(),
            },
            ..Default::default()
        })
        .into()
}

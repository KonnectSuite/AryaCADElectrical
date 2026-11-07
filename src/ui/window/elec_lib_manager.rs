//! Symbol library manager: the dialog behind **Electrical → Symbol Manager**.
//!
//! It lists the symbols in the loaded library folder, lets the user search and
//! place them, change a symbol's class, delete a symbol, or add the current
//! drawing selection as a new symbol with a name, class and pin count.  It is
//! deliberately a data dialog like the electrical reports: library path and
//! status at the top, the symbol list on the left, the selected symbol's
//! details on the right, and the "add from selection" form at the bottom.

use iced::widget::{button, column, container, row, scrollable, text, text_input, Space};
use iced::{Background, Border, Element, Length};

use crate::app::Message;
use crate::ui::modal::ModalSizing;
use crate::ui::style::common::muted_style;
use crate::ui::style::form::{button_style, dialog_button, field_style};

/// One catalogue row as the manager dialog needs it.
pub struct LibSymbolRow {
    pub name: String,
    pub class: String,
    pub pins: usize,
    pub scale: f64,
}

/// What the dialog keeps between frames.
#[derive(Default)]
pub struct LibraryManagerState {
    /// Library folder shown in the header and the rows currently in the catalogue.
    pub folder: String,
    pub rows: Vec<LibSymbolRow>,
    pub search: String,
    pub selected: Option<String>,
    pub class_edit: String,
    pub new_name: String,
    pub new_class: String,
    pub new_pins: String,
    pub status: String,
    pub confirm_delete: bool,
}

pub fn view<'a>(
    state: &'a LibraryManagerState,
    folder: Option<&'a str>,
    rows: &'a [LibSymbolRow],
    sizing: ModalSizing,
) -> Element<'a, Message> {
    let needle = state.search.to_lowercase();
    let visible: Vec<&LibSymbolRow> = rows
        .iter()
        .filter(|row| {
            needle.is_empty()
                || row.name.to_lowercase().contains(&needle)
                || row.class.to_lowercase().contains(&needle)
        })
        .collect();

    // ---- header: where the library lives, and what can be done to it ----------
    let header = row![
        column![
            text("Symbol library").size(15),
            text(folder.unwrap_or("No library folder loaded."))
                .size(11)
                .style(muted_style),
        ]
        .spacing(2)
        .width(Length::Fill),
        button(text("Reload").size(12))
            .on_press(Message::ElecLibReload)
            .padding([6, 12])
            .style(button_style(false)),
        button(text("Open folder").size(12))
            .on_press(Message::ElecLibOpenFolder)
            .padding([6, 12])
            .style(button_style(false)),
    ]
    .spacing(8)
    .align_y(iced::Center);

    // ---- left: search + symbol list -------------------------------------------
    let search = text_input("Search symbols…", &state.search)
        .on_input(Message::ElecLibSearch)
        .style(field_style)
        .padding([8, 10])
        .size(13);

    let mut list = column![].spacing(2);
    for row in &visible {
        let selected = state.selected.as_deref() == Some(row.name.as_str());
        let entry = column![
            text(row.name.clone()).size(13),
            text(format!("{}  ·  {} pin(s)", row.class, row.pins))
                .size(11)
                .style(muted_style),
        ]
        .spacing(1)
        .width(Length::Fill);
        list = list.push(
            button(entry)
                .on_press(Message::ElecLibSelect(row.name.clone()))
                .padding([6, 8])
                .width(Length::Fill)
                .style(button_style(selected)),
        );
    }
    if visible.is_empty() {
        list = list.push(
            text("No symbols match this search.")
                .size(12)
                .style(muted_style),
        );
    }
    let left = column![
        search,
        scrollable(list).height(Length::Fill),
        text(format!("{} symbol(s) · {} shown", rows.len(), visible.len()))
            .size(10)
            .style(muted_style),
    ]
    .spacing(8)
    .width(Length::FillPortion(5));

    // ---- right: the selected symbol -------------------------------------------
    let (details, actions) = match state
        .selected
        .as_ref()
        .and_then(|name| rows.iter().find(|row| &row.name == name))
    {
        Some(row) => {
            let class_field = text_input("Class", &state.class_edit)
                .on_input(Message::ElecLibClassEdit)
                .style(field_style)
                .padding([7, 9])
                .size(12);
            let details = column![
                text(row.name.clone()).size(16),
                text(format!("{} pin(s) · scale {}", row.pins, row.scale))
                    .size(11)
                    .style(muted_style),
                text("Class").size(11).style(muted_style),
                class_field,
                text("The class groups the symbol in the palette and in the reports.")
                    .size(10)
                    .style(muted_style),
            ]
            .spacing(8);
            let delete_label = if state.confirm_delete {
                "Confirm delete"
            } else {
                "Delete symbol"
            };
            let actions = column![
                dialog_button("Save class", Message::ElecLibSaveClass, true),
                dialog_button("Place symbol", Message::ElecLibPlace, false),
                dialog_button(delete_label, Message::ElecLibDelete, false),
            ]
            .spacing(6);
            (details, actions)
        }
        None => (
            column![
                text("Select a symbol").size(14),
                text("Pick a row to see its class, pins and actions.")
                    .size(11)
                    .style(muted_style),
            ]
            .spacing(4),
            column![],
        ),
    };
    let right = column![details, Space::new().height(Length::Fixed(8.0)), actions]
        .spacing(4)
        .width(Length::FillPortion(4));

    // ---- bottom: add the current selection as a new symbol --------------------
    let name_input = text_input("New symbol name", &state.new_name)
        .on_input(Message::ElecLibNewName)
        .style(field_style)
        .padding([7, 9])
        .size(12)
        .width(Length::FillPortion(3));
    let class_input = text_input("Class (e.g. IO Module)", &state.new_class)
        .on_input(Message::ElecLibNewClass)
        .style(field_style)
        .padding([7, 9])
        .size(12)
        .width(Length::FillPortion(3));
    let pins_input = text_input("Pins", &state.new_pins)
        .on_input(Message::ElecLibNewPins)
        .style(field_style)
        .padding([7, 9])
        .size(12)
        .width(Length::FillPortion(1));
    let add_row = row![
        name_input,
        class_input,
        pins_input,
        dialog_button("Add from selection", Message::ElecLibAdd, true),
    ]
    .spacing(8)
    .align_y(iced::Center);
    let add_hint = text(
        "Select the symbol body and its ATTDEFs in the drawing first. Pins are optional; the odd pin goes on the right edge.",
    )
    .size(10)
    .style(muted_style);

    let footer_status = if state.status.is_empty() {
        text("").size(11)
    } else {
        text(state.status.clone()).size(11)
    };

    let body = column![
        header,
        row![left, right].spacing(14).height(Length::Fill),
        add_row,
        add_hint,
        footer_status,
        row![
            Space::new().width(Length::Fill),
            dialog_button("Close", Message::CloseModal, true)
        ],
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

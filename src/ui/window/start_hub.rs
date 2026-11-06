//! Two dialogs drawn by the start hub: the drafting-or-electrical chooser and
//! the electrical drawing-set wizard. State lives in
//! [`crate::app::elec_start::StartHub`]; messages are handled in
//! `src/app/update/mod.rs`.

use crate::app::elec_start::{EcadField, EcadStandard, StartHub};
use crate::app::Message;
use crate::ui::modal::ModalSizing;
use iced::widget::{button, column, container, row, scrollable, text, text_input, Space};
use iced::{Element, Fill, Length};

fn big_choice<'a>(
    title: &'static str,
    description: &'static str,
    message: Message,
) -> Element<'a, Message> {
    button(
        column![
            text(title).size(21),
            Space::new().height(4),
            text(description).size(12).width(Fill),
        ]
        .spacing(2),
    )
    .on_press(message)
    .padding(20)
    .width(Fill)
    .height(Length::Fixed(168.0))
    .into()
}

pub(crate) fn mode_view(sizing: ModalSizing) -> Element<'static, Message> {
    container(
        column![
            text("Open CAD Studio").size(30),
            text("How do you want to begin?").size(14),
            Space::new().height(16),
            row![
                big_choice(
                    "Drafting drawing",
                    "Start a blank AutoCAD-style drawing. The electrical commands stay available if you need them.",
                    Message::StartModeDrafting,
                ),
                big_choice(
                    "Electrical drawing set",
                    "Create IEC or NEC sheets with a title block, sheet numbering and an electrical project file.",
                    Message::StartModeElectrical,
                ),
            ]
            .spacing(14),
            Space::new().height(12),
            row![
                text("ELECSTART reopens this window at any time.").size(11),
                Space::new().width(Fill),
                button("Continue to the start page").on_press(Message::CloseModal),
            ]
            .align_y(iced::Center),
        ]
        .spacing(10),
    )
    .padding(24)
    .width(sizing.width)
    .height(sizing.height)
    .into()
}

fn standard_button<'a>(standard: EcadStandard, selected: bool) -> Element<'a, Message> {
    let prefix = if selected { "\u{25CF}  " } else { "\u{25CB}  " };
    button(text(format!("{prefix}{}", standard.label())).size(13))
        .on_press(Message::EcadStandard(standard))
        .padding([8, 12])
        .width(Fill)
        .into()
}

fn size_button<'a>(
    label: &'static str,
    dimensions: String,
    selected: bool,
    message: Message,
) -> Element<'a, Message> {
    let prefix = if selected { "\u{25B8}  " } else { "    " };
    button(
        row![
            text(format!("{prefix}{label}")).size(13).width(Fill),
            text(dimensions).size(11),
        ]
        .spacing(8)
        .align_y(iced::Center),
    )
    .on_press(message)
    .padding([5, 10])
    .width(Fill)
    .into()
}

fn field<'a>(
    label: &'static str,
    value: &'a str,
    hint: &'static str,
    message: impl Fn(String) -> Message + 'a,
) -> Element<'a, Message> {
    column![
        text(label).size(11),
        text_input(hint, value)
            .on_input(message)
            .padding([6, 8])
            .size(13),
    ]
    .spacing(3)
    .width(Fill)
    .into()
}

/// The wizard. Returns a view borrowing the dialog state.
pub(crate) fn setup_view<'a>(hub: &'a StartHub, sizing: ModalSizing) -> Element<'a, Message> {
    let sizes = hub.standard.sizes();
    let mut size_list = column![].spacing(2);
    for (index, size) in sizes.iter().enumerate() {
        size_list = size_list.push(size_button(
            size.label,
            size.dimensions(),
            hub.size.min(sizes.len() - 1) == index,
            Message::EcadSize(index),
        ));
    }

    let mut left = column![text("Standard").size(12), Space::new().height(2)].spacing(4);
    left = left.push(standard_button(EcadStandard::Iec, hub.standard == EcadStandard::Iec));
    left = left.push(standard_button(EcadStandard::Nec, hub.standard == EcadStandard::Nec));
    left = left.push(Space::new().height(8));
    left = left.push(text("Sheet size").size(12));
    left = left.push(scrollable(size_list).height(Length::Fixed(300.0)));

    let right = column![
        row![
            field("Project name", &hub.project, "e.g. Line 3 conveyor", |value| {
                Message::EcadFieldChanged(EcadField::Project, value)
            }),
            field("Drawing title", &hub.title, "Electrical schematic", |value| {
                Message::EcadFieldChanged(EcadField::Title, value)
            }),
        ]
        .spacing(12),
        row![
            field("Company", &hub.company, "Your company", |value| {
                Message::EcadFieldChanged(EcadField::Company, value)
            }),
            field("Drawn by", &hub.drawn_by, "Initials", |value| {
                Message::EcadFieldChanged(EcadField::DrawnBy, value)
            }),
        ]
        .spacing(12),
        row![
            field("Drawing number", &hub.drawing, "AR-0001", |value| {
                Message::EcadFieldChanged(EcadField::Drawing, value)
            }),
            field("Sheets in the set", &hub.sheets, "1", |value| {
                Message::EcadFieldChanged(EcadField::Sheets, value)
            }),
        ]
        .spacing(12),
        row![
            field(
                "Folder for the drawing set",
                &hub.folder,
                "C:\\Users\\...\\Documents\\AryaCAD\\Project1",
                |value| Message::EcadFieldChanged(EcadField::Folder, value),
            ),
            button("Browse\u{2026}")
                .on_press(Message::EcadBrowseFolder)
                .padding([8, 12]),
        ]
        .spacing(8)
        .align_y(iced::Bottom),
        Space::new().height(4),
        match hub.template_path() {
            Some(path) => text(format!("Template: {}", path.display())).size(11),
            None => text("Template not found: keep the templates folder beside the program.").size(11),
        },
        Space::new().height(4),
        text("Sheet files are written as <drawing number>-NN.dxf and listed in project.eprj.").size(11),
    ]
    .spacing(10);

    let mut body = column![
        row![
            text("New electrical drawing set").size(22),
            Space::new().width(Fill),
            button("Back").on_press(Message::EcadBack),
        ]
        .align_y(iced::Center),
        Space::new().height(4),
        row![left.width(Length::Fixed(330.0)).spacing(4), right.width(Fill)].spacing(18),
    ]
    .spacing(10);
    if let Some(error) = &hub.error {
        body = body.push(text(format!("Could not create the set: {error}")).size(12));
    }
    body = body.push(
        row![
            Space::new().width(Fill),
            button("Cancel").on_press(Message::CloseModal),
            button("Create drawing set")
                .on_press(Message::EcadCreate)
                .padding([8, 16]),
        ]
        .spacing(8),
    );

    container(body)
        .padding(22)
        .width(sizing.width)
        .height(sizing.height)
        .into()
}


//! The read-only electrical report dialog.
//!
//! The ECAD commands produce tables — components, terminal strips, nets, bills
//! of material and the parts catalogue. Printing them to the command line made
//! the ribbon buttons look dead, so the same data is shown in a real dialog:
//! a summary strip, a search field, one-chip-per-class filter and a scrollable
//! table that follows the OpenCAD theme. The command line still receives the
//! text, which keeps `--batch`/`--each` scripts and the automation bridge able
//! to read reports without a window.

use iced::widget::{button, column, container, scrollable, text, text_input, Space};
use iced::{Background, Border, Element, Length, Theme};

use crate::app::Message;
use crate::ui::modal::ModalSizing;
use crate::ui::style::common::muted_style;
use crate::ui::style::form::{dialog_button, field_style};

/// One report as the dialog shows it.
#[derive(Debug, Clone, Default)]
pub struct ElecReport {
    /// Shown in the modal title bar.
    pub title: String,
    /// Headline facts above the table ("5 nets, 20 pins", warnings, …).
    pub summary: Vec<String>,
    /// Column captions; an empty list means the report is text-only.
    pub columns: Vec<String>,
    /// Cell values, one row per item.
    pub rows: Vec<Vec<String>>,
    /// Index of the column the class chips filter on.
    pub class_column: Option<usize>,
    /// Caption of the "no filter" chip ("All classes", "All strips", …).
    pub filter_label: String,
    /// Live search query (owned by the report so redraws keep it).
    pub search: String,
    /// Selected class, or `None` for "all classes".
    pub class_filter: Option<String>,
}

impl ElecReport {
    /// Classes present in the table, in first-seen order.
    pub fn classes(&self) -> Vec<String> {
        let Some(index) = self.class_column else {
            return Vec::new();
        };
        let mut classes: Vec<String> = Vec::new();
        for row in &self.rows {
            if let Some(value) = row.get(index) {
                let value = value.trim();
                if !value.is_empty() && !classes.iter().any(|seen| seen.eq_ignore_ascii_case(value)) {
                    classes.push(value.to_string());
                }
            }
        }
        classes
    }

    /// Indices of the rows matching the search text and the class chip.
    pub fn visible_rows(&self) -> Vec<usize> {
        let needle = self.search.trim().to_ascii_lowercase();
        self.rows
            .iter()
            .enumerate()
            .filter(|(_, row)| {
                if let (Some(index), Some(class)) = (self.class_column, &self.class_filter) {
                    if row
                        .get(index)
                        .is_none_or(|value| !value.trim().eq_ignore_ascii_case(class))
                    {
                        return false;
                    }
                }
                if needle.is_empty() {
                    return true;
                }
                row.iter()
                    .any(|cell| cell.to_ascii_lowercase().contains(&needle))
            })
            .map(|(index, _)| index)
            .collect()
    }
}

fn surface(theme: &Theme) -> container::Style {
    let palette = theme.palette();
    container::Style {
        background: Some(Background::Color(palette.background.base.color)),
        border: Border {
            color: palette.background.neutral.color,
            width: 1.0,
            radius: 8.0.into(),
        },
        ..container::Style::default()
    }
}

fn summary_chip(theme: &Theme) -> container::Style {
    let palette = theme.palette();
    container::Style {
        background: Some(Background::Color(palette.background.weak.color)),
        border: Border {
            color: palette.background.neutral.color,
            width: 1.0,
            radius: 5.0.into(),
        },
        ..container::Style::default()
    }
}

fn header_style(theme: &Theme) -> container::Style {
    let palette = theme.palette();
    container::Style {
        background: Some(Background::Color(palette.background.weak.color)),
        border: Border {
            color: palette.background.neutral.color,
            width: 0.0,
            radius: 5.0.into(),
        },
        ..container::Style::default()
    }
}

/// Zebra banding: even rows blend into the dialog, odd rows lift slightly.
fn row_style(index: usize) -> impl Fn(&Theme) -> container::Style {
    move |theme| {
        if index % 2 == 1 {
            let palette = theme.palette();
            container::Style {
                background: Some(Background::Color(
                    palette.background.weak.color.scale_alpha(0.45),
                )),
                ..container::Style::default()
            }
        } else {
            container::Style::default()
        }
    }
}

/// A class chip: the accent marks the active class, the rest stay quiet.
fn class_chip_style(selected: bool, theme: &Theme, status: button::Status) -> button::Style {
    let palette = theme.palette();
    let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
    let background = if selected {
        palette.primary.strong.color
    } else if hovered {
        palette.background.strong.color
    } else {
        palette.background.weak.color
    };
    let text_color = if selected {
        palette.primary.strong.text
    } else {
        palette.background.base.text
    };
    button::Style {
        background: Some(Background::Color(background)),
        text_color,
        border: Border {
            color: if selected {
                palette.primary.strong.color
            } else {
                palette.background.neutral.color
            },
            width: 1.0,
            radius: 999.0.into(),
        },
        ..button::Style::default()
    }
}

/// Column widths as `FillPortion` weights: long columns get more room, short
/// ones (a tag, a quantity) keep their width so the table does not wobble
/// between reports.
fn column_weights(report: &ElecReport) -> Vec<u16> {
    let mut weights = Vec::with_capacity(report.columns.len());
    for (index, caption) in report.columns.iter().enumerate() {
        let longest = report
            .rows
            .iter()
            .filter_map(|row| row.get(index))
            .map(|cell| cell.chars().count())
            .chain(std::iter::once(caption.chars().count()))
            .max()
            .unwrap_or(0);
        let weight = longest.clamp(5, 36);
        weights.push(weight as u16);
    }
    weights
}

fn alignment_for(index: usize, report: &ElecReport) -> iced::alignment::Horizontal {
    // Numeric-looking columns right-align: quantity, segment counts.
    let numeric = report
        .rows
        .iter()
        .filter_map(|row| row.get(index))
        .filter(|cell| !cell.trim().is_empty())
        .all(|cell| cell.trim().parse::<f64>().is_ok());
    if numeric && !report.rows.is_empty() {
        iced::alignment::Horizontal::Right
    } else {
        iced::alignment::Horizontal::Left
    }
}

/// Shared cell: uniform size and padding so headers and data line up.
fn cell_text(value: &str, index: usize, report: &ElecReport) -> Element<'static, Message> {
    text(value.to_string())
        .size(12)
        .width(Length::FillPortion(column_weights(report)[index]))
        .align_x(alignment_for(index, report))
        .into()
}

fn header_cell(caption: &str, index: usize, report: &ElecReport) -> Element<'static, Message> {
    text(caption.to_ascii_uppercase())
        .size(10)
        .style(muted_style)
        .width(Length::FillPortion(column_weights(report)[index]))
        .align_x(alignment_for(index, report))
        .into()
}

fn class_chips<'a>(report: &'a ElecReport) -> Element<'a, Message> {
    let classes = report.classes();
    if classes.is_empty() {
        return Space::new().width(0).into();
    }
    let selected = report.class_filter.as_deref();
    let all_label = if report.filter_label.trim().is_empty() {
        "All classes"
    } else {
        report.filter_label.as_str()
    };
    let mut chips: Vec<Element<'a, Message>> = vec![button(text(all_label).size(11))
        .on_press(Message::ElecReportClass(None))
        .padding([5, 10])
        .style(move |theme, status| class_chip_style(selected.is_none(), theme, status))
        .into()];
    for class in classes {
        let active = selected.is_some_and(|value| value.eq_ignore_ascii_case(&class));
        let label = class.clone();
        let target = class.clone();
        chips.push(
            button(text(label).size(11))
                .on_press(Message::ElecReportClass(Some(target)))
                .padding([5, 10])
                .style(move |theme, status| class_chip_style(active, theme, status))
                .into(),
        );
    }
    iced::widget::Row::with_children(chips)
        .spacing(6)
        .wrap()
        .into()
}

/// The report dialog body.
pub fn view<'a>(report: &'a ElecReport, sizing: ModalSizing) -> Element<'a, Message> {
    let palette = |theme: &Theme| (theme.palette().background.base.text, theme.palette().background.weak.text);
    let _ = palette;
    let total = report.rows.len();
    let visible = report.visible_rows();

    let mut body = column![].spacing(12).width(Length::Fill);

    // Summary facts as quiet chips, capped so one noisy report cannot push the
    // table off the dialog.
    if !report.summary.is_empty() {
        let mut chips: Vec<Element<'a, Message>> = Vec::new();
        for line in report.summary.iter().take(4) {
            chips.push(
                container(text(line.as_str()).size(11))
                    .padding([6, 10])
                    .style(summary_chip)
                    .into(),
            );
        }
        body = body.push(iced::widget::Row::with_children(chips).spacing(8).wrap());
    }

    // Toolbar: search on the left, class chips underneath (ribbon dialogs are
    // 820 px wide, so chips wrap above the table rather than squeezing it).
    if !report.columns.is_empty() {
        body = body.push(
            text_input("Search this report…", &report.search)
                .on_input(Message::ElecReportSearch)
                .padding([8, 10])
                .size(13)
                .width(Length::Fill)
                .style(field_style),
        );
        let chips = class_chips(report);
        if !report.classes().is_empty() {
            body = body.push(chips);
        }
    }

    if report.columns.is_empty() {
        // Text-only fallback (a command that has nothing tabular to show).
        let lines: Vec<Element<'a, Message>> = report
            .summary
            .iter()
            .map(|line| text(line.as_str()).size(12).into())
            .collect();
        body = body.push(scrollable(column(lines).spacing(6)).height(Length::Fill));
    } else {
        // Header band.
        let header_cells: Vec<Element<'a, Message>> = report
            .columns
            .iter()
            .enumerate()
            .map(|(index, caption)| header_cell(caption, index, report))
            .collect();
        let header = iced::widget::Row::with_children(header_cells).spacing(14);
        body = body.push(
            container(header)
                .padding([8, 12])
                .width(Length::Fill)
                .style(header_style),
        );

        // Data rows.
        let mut table = column![].spacing(0).width(Length::Fill);
        for index in &visible {
            let values = &report.rows[*index];
            let cells: Vec<Element<'a, Message>> = report
                .columns
                .iter()
                .enumerate()
                .map(|(column, _)| {
                    cell_text(values.get(column).map(String::as_str).unwrap_or(""), column, report)
                })
                .collect();
            let cells = iced::widget::Row::with_children(cells).spacing(14);
            table = table.push(
                container(cells)
                    .padding([7, 12])
                    .width(Length::Fill)
                    .style(row_style(*index)),
            );
        }
        if visible.is_empty() {
            table = table.push(
                container(
                    text("No rows match this view.")
                        .size(13)
                        .style(muted_style),
                )
                .padding([24, 12])
                .width(Length::Fill)
                .center_x(Length::Fill)
                .center_y(Length::Fill),
            );
        }
        body = body.push(
            container(scrollable(table).height(Length::Fill))
                .width(Length::Fill)
                .height(Length::Fill)
                .style(surface),
        );
    }

    // Footer: what is shown, and the one action the dialog has.
    let count = if report.columns.is_empty() {
        format!("{} line(s)", report.summary.len())
    } else if visible.len() == total {
        format!("{total} row(s)")
    } else {
        format!("{} of {total} row(s)", visible.len())
    };
    body = body.push(
        iced::widget::Row::with_children(vec![
            text(count).size(11).style(muted_style).into(),
            Space::new().width(Length::Fill).into(),
            dialog_button("Close", Message::CloseModal, true).into(),
        ])
        .align_y(iced::Alignment::Center),
    );

    container(body)
        .padding(16)
        .width(sizing.width)
        .height(sizing.height)
        .style(|theme: &Theme| container::Style {
            background: Some(Background::Color(theme.palette().background.base.color)),
            ..container::Style::default()
        })
        .into()
}

/// Kept for the report's own unit tests and for callers that need the parsed
/// class count without a widget tree.
pub fn filtered_len(report: &ElecReport) -> usize {
    report.visible_rows().len()
}

#[cfg(test)]
mod tests {
    use super::ElecReport;

    fn sample() -> ElecReport {
        ElecReport {
            title: "Parts".to_string(),
            summary: vec!["3 parts".to_string()],
            columns: vec!["Part".to_string(), "Class".to_string()],
            rows: vec![
                vec!["100-C09".to_string(), "Contactor".to_string()],
                vec!["193-EC2".to_string(), "Overload".to_string()],
                vec!["800T".to_string(), "Button".to_string()],
            ],
            class_column: Some(1),
            filter_label: "All classes".to_string(),
            search: String::new(),
            class_filter: None,
        }
    }

    #[test]
    fn classes_follow_first_seen_order() {
        let report = sample();
        assert_eq!(report.classes(), vec!["Contactor", "Overload", "Button"]);
    }

    #[test]
    fn search_matches_any_column_case_insensitively() {
        let mut report = sample();
        report.search = "100-c".to_string();
        assert_eq!(report.visible_rows(), vec![0]);
        report.search = "OVER".to_string();
        assert_eq!(report.visible_rows(), vec![1]);
        report.search = "  ".to_string();
        assert_eq!(report.visible_rows(), vec![0, 1, 2]);
    }

    #[test]
    fn class_filter_and_search_combine() {
        let mut report = sample();
        report.class_filter = Some("contactor".to_string());
        assert_eq!(report.visible_rows(), vec![0]);
        report.search = "nothing".to_string();
        assert!(report.visible_rows().is_empty());
    }

    #[test]
    fn class_filter_ignores_case_and_unknown_class_hides_everything() {
        let mut report = sample();
        report.class_filter = Some("BUTTON".to_string());
        assert_eq!(report.visible_rows(), vec![2]);
        report.class_filter = Some("Relay".to_string());
        assert!(report.visible_rows().is_empty());
    }
}

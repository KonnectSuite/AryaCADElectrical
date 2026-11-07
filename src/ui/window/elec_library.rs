//! The electrical symbol library palette.
//!
//! Lists the symbols a library loaded, each with a thumbnail of its geometry, and inserts
//! one on click. The thumbnail matters: block names like `HT0_01` say nothing, and picking
//! a symbol by sight is the whole point of a library browser.
//!
//! The body is one widget rather than a stock `scrollable` because the search field has to
//! see the keystrokes and the list has to shrink with them, and the palette's message
//! contract is only `view(symbols, sizing)`: the modal carries no search state of its own.
//! Keeping the query in this widget's tree state makes the palette self-contained — the
//! only message it ever sends is the `ELECPLACE` a click always sent, so placement behaves
//! exactly as it did.

use iced::advanced::layout::{self, Layout};
use iced::advanced::text::{self as advanced_text, Paragraph as _, Renderer as _};
use iced::advanced::widget::{self, tree, Tree, Widget};
use iced::advanced::{mouse, overlay, renderer, Renderer as _, Shell};
use iced::widget::canvas::{Frame, Geometry, Path, Program, Stroke};
use iced::widget::{button, canvas, column, container, row, text};
use iced::{
    alignment, border, keyboard, Border, Color, Element, Event, Fill, Length, Pixels, Point,
    Rectangle, Renderer, Size, Theme, Vector,
};

use crate::app::Message;
use crate::ui::modal::ModalSizing;
use crate::ui::style::common::muted_style;
use crate::ui::style::row_button::cell_button_style;

/// One symbol as the palette shows it: name, class and the segments to draw.
pub type SymbolRow = (String, String, Vec<((f64, f64), (f64, f64))>);

/// Height of the search strip above the list.
const SEARCH_H: f32 = 40.0;
/// Height of a row slot in the list, gap included.
const ROW_H: f32 = 44.0;
/// Height the row button itself occupies inside its slot.
const ROW_BUTTON_H: f32 = ROW_H - 4.0;

/// What a click on a symbol runs.
///
/// Reusing the command message keeps the palette free of new message plumbing: the button
/// dispatches the same command a user would type, and `ELECPLACE` closes the modal itself.
fn insert_command(symbol: &str) -> Message {
    Message::Command(format!("ELECPLACE {symbol}"))
}

/// Draws one symbol's segments scaled into the canvas.
struct Thumbnail {
    segments: Vec<((f64, f64), (f64, f64))>,
}

impl Program<Message> for Thumbnail {
    type State = ();

    fn draw(
        &self,
        _state: &(),
        renderer: &iced::Renderer,
        theme: &Theme,
        bounds: iced::Rectangle,
        _cursor: iced::mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        if self.segments.is_empty() {
            return vec![frame.into_geometry()];
        }

        // Fit the symbol into the canvas with a small margin.
        let (mut min_x, mut min_y) = (f64::MAX, f64::MAX);
        let (mut max_x, mut max_y) = (f64::MIN, f64::MIN);
        for ((ax, ay), (bx, by)) in &self.segments {
            min_x = min_x.min(*ax).min(*bx);
            min_y = min_y.min(*ay).min(*by);
            max_x = max_x.max(*ax).max(*bx);
            max_y = max_y.max(*ay).max(*by);
        }
        let width = (max_x - min_x).max(1e-6);
        let height = (max_y - min_y).max(1e-6);
        let size = bounds.size();
        let margin = 3.0_f32;
        let scale = ((size.width - margin * 2.0) / width as f32)
            .min((size.height - margin * 2.0) / height as f32)
            .max(0.0001);
        // Symbols are drawn with Y up; the canvas has Y down.
        let to_view = |x: f64, y: f64| {
            iced::Point::new(
                margin + ((x - min_x) as f32) * scale,
                size.height - margin - ((y - min_y) as f32) * scale,
            )
        };

        let color = theme.palette().background.base.text;
        for ((ax, ay), (bx, by)) in &self.segments {
            let path = Path::line(to_view(*ax, *ay), to_view(*bx, *by));
            frame.stroke(&path, Stroke::default().with_color(color).with_width(1.0));
        }
        vec![frame.into_geometry()]
    }
}

/// One row: the symbol's geometry beside its name and class.
fn symbol_row(symbol: &SymbolRow, index: usize) -> Element<'static, Message> {
    let (name, class, segments) = symbol;
    let thumbnail = canvas(Thumbnail {
        segments: segments.clone(),
    })
    .width(Length::Fixed(52.0))
    .height(Length::Fixed(32.0));
    let labels = column![
        text(name.clone()).size(13),
        // The class is the part a reader actually recognizes: `CONT-NO` is a
        // contactor, and the electrical reports group by it too.
        text(class.clone()).size(11).style(muted_style),
    ]
    .spacing(1);

    button(
        row![
            container(thumbnail)
                .width(Length::Fixed(56.0))
                .height(Length::Fill)
                .align_y(alignment::Vertical::Center),
            labels,
        ]
        .spacing(10)
        .align_y(alignment::Vertical::Center),
    )
    .width(Fill)
    .height(Length::Fixed(ROW_BUTTON_H))
    .padding([2, 10])
    // The layers and xref list rows use the same hover and text colors.
    .style(cell_button_style(false, index))
    .on_press(insert_command(name))
    .into()
}

/// The palette's own state: the search query and how far the list scrolled.
#[derive(Default)]
struct State {
    query: String,
    /// First matching row shown. Scrolling moves by whole rows, so a row is
    /// never clipped in half and can never catch a click outside the list.
    scroll: usize,
    /// Row index (into `entries`) the keyboard cursor is on, so ArrowUp/Down and
    /// Enter can drive the palette without a mouse.
    selected: Option<usize>,
}

/// The palette body: a search strip over a filtered, scrolling list of symbols.
struct Palette {
    /// (name, class) in the same order as `rows`, for filtering.
    entries: Vec<(String, String)>,
    rows: Vec<Element<'static, Message>>,
}

/// The rounded search strip at the top of the palette.
fn search_rect(bounds: Rectangle) -> Rectangle {
    Rectangle {
        x: bounds.x + 2.0,
        y: bounds.y + 5.0,
        width: (bounds.width - 4.0).max(0.0),
        height: 30.0,
    }
}

/// The area the symbol rows are drawn and clipped in.
fn list_rect(bounds: Rectangle) -> Rectangle {
    Rectangle {
        x: bounds.x,
        y: bounds.y + SEARCH_H,
        width: bounds.width,
        height: (bounds.height - SEARCH_H).max(0.0),
    }
}

/// How many rows fit under the search strip.
fn capacity(height: f32) -> usize {
    (((height - SEARCH_H).max(0.0) / ROW_H).floor() as usize).max(1)
}

/// A rounded quad with no outline, for the surfaces this widget paints itself.
fn surface(bounds: Rectangle, radius: f32) -> renderer::Quad {
    renderer::Quad {
        bounds,
        border: border::rounded(radius),
        ..renderer::Quad::default()
    }
}

/// One line of text inside a bounds this widget owns.
#[allow(clippy::too_many_arguments)]
fn draw_text(
    renderer: &mut Renderer,
    content: &str,
    at: Point,
    size: f32,
    color: Color,
    align_x: advanced_text::Alignment,
    bounds: Size,
    clip: Rectangle,
) {
    let font = renderer.default_font();
    renderer.fill_text(
        advanced_text::Text {
            content: content.to_string(),
            bounds,
            size: Pixels(size),
            line_height: advanced_text::LineHeight::default(),
            font,
            align_x,
            align_y: alignment::Vertical::Center,
            shaping: advanced_text::Shaping::Advanced,
            wrapping: advanced_text::Wrapping::None,
            ellipsis: advanced_text::Ellipsis::None,
            hint_factor: None,
        },
        at,
        color,
        clip,
    );
}

/// The width `content` occupies at `size`, for placing the caret after it.
fn text_width(renderer: &Renderer, content: &str, size: f32) -> f32 {
    let font = renderer.default_font();
    <Renderer as advanced_text::Renderer>::Paragraph::with_text(advanced_text::Text {
        content,
        bounds: Size::INFINITE,
        size: Pixels(size),
        line_height: advanced_text::LineHeight::default(),
        font,
        align_x: advanced_text::Alignment::Left,
        align_y: alignment::Vertical::Top,
        shaping: advanced_text::Shaping::Advanced,
        wrapping: advanced_text::Wrapping::None,
        ellipsis: advanced_text::Ellipsis::None,
        hint_factor: None,
    })
    .min_bounds()
    .width
}

impl Palette {
    /// Indices of the rows whose name or class contains the query. Matching the
    /// class too means `relay` finds the relays without knowing a block name.
    fn matches(&self, query: &str) -> Vec<usize> {
        let needle = query.trim().to_lowercase();
        if needle.is_empty() {
            return (0..self.entries.len()).collect();
        }
        self.entries
            .iter()
            .enumerate()
            .filter(|(_, (name, class))| {
                name.to_lowercase().contains(needle.as_str())
                    || class.to_lowercase().contains(needle.as_str())
            })
            .map(|(index, _)| index)
            .collect()
    }
}

impl Widget<Message, Theme, Renderer> for Palette {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }

    fn diff(&mut self, tree: &mut Tree) {
        tree.diff_children(self.rows.as_mut_slice());
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fill)
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let space = limits.max();
        let state = tree.state.downcast_mut::<State>();
        let matches = self.matches(&state.query);
        let capacity = capacity(space.height);
        // A filter that shortens the list may leave the scroll past its end.
        state.scroll = state
            .scroll
            .min(matches.len().saturating_sub(capacity));
        // The keyboard cursor belongs to the filtered list, not to the query.
        if state
            .selected
            .is_some_and(|selected| !matches.contains(&selected))
        {
            state.selected = None;
        }

        let mut nodes = Vec::with_capacity(self.rows.len());
        for (index, row) in self.rows.iter_mut().enumerate() {
            let slot = matches
                .iter()
                .position(|candidate| *candidate == index)
                .and_then(|position| position.checked_sub(state.scroll))
                .filter(|slot| *slot < capacity);
            let Some(slot) = slot else {
                nodes.push(layout::Node::new(Size::ZERO));
                continue;
            };
            let y = SEARCH_H + slot as f32 * ROW_H;
            let row_limits =
                layout::Limits::new(Size::ZERO, Size::new(space.width, ROW_H));
            let node = row
                .as_widget_mut()
                .layout(&mut tree.children[index], renderer, &row_limits);
            nodes.push(node.move_to(Point::new(0.0, y + 2.0)));
        }
        layout::Node::with_children(space, nodes)
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn widget::Operation,
    ) {
        for (index, child_layout) in layout.children().enumerate() {
            if index >= self.rows.len() || index >= tree.children.len() {
                break;
            }
            self.rows[index].as_widget_mut().operate(
                &mut tree.children[index],
                child_layout,
                renderer,
                operation,
            );
        }
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        let state = tree.state.downcast_mut::<State>();
        let matches = self.matches(&state.query);
        let capacity = capacity(bounds.height);

        // The rows are real buttons: let them see the event first so hover and
        // press behave like the list rows everywhere else in the application.
        for (index, child_layout) in layout.children().enumerate() {
            if index >= self.rows.len() || index >= tree.children.len() {
                break;
            }
            let child_bounds = child_layout.bounds();
            if child_bounds.width <= 0.0 || child_bounds.height <= 0.0 {
                continue;
            }
            self.rows[index].as_widget_mut().update(
                &mut tree.children[index],
                event,
                child_layout,
                cursor,
                renderer,
                shell,
                viewport,
            );
        }

        // The search field owns the keyboard: every printable key filters the
        // list. Chords stay with the application, and Escape stays with the
        // modal (it closes the palette).
        if let Event::Keyboard(keyboard::Event::KeyPressed {
            key, modifiers, text, ..
        }) = event
        {
            if !modifiers.control() && !modifiers.alt() && !modifiers.logo() {
                // The arrow keys walk the filtered list and Enter inserts the
                // highlighted row, so the palette works without a mouse.
                match key {
                    keyboard::Key::Named(keyboard::key::Named::ArrowDown)
                    | keyboard::Key::Named(keyboard::key::Named::ArrowUp) => {
                        if !matches.is_empty() {
                            let forward = matches!(
                                key,
                                keyboard::Key::Named(keyboard::key::Named::ArrowDown)
                            );
                            let current = state
                                .selected
                                .and_then(|selected| {
                                    matches.iter().position(|row| *row == selected)
                                });
                            let next_position = match (current, forward) {
                                (Some(position), true) => (position + 1).min(matches.len() - 1),
                                (Some(position), false) => position.saturating_sub(1),
                                (None, true) => 0,
                                (None, false) => matches.len() - 1,
                            };
                            state.selected = Some(matches[next_position]);
                            // Keep the cursor row inside the visible window.
                            state.scroll = state.scroll.min(next_position);
                            if next_position >= state.scroll + capacity {
                                state.scroll = next_position + 1 - capacity;
                            }
                            shell.invalidate_layout();
                            shell.request_redraw();
                            shell.capture_event();
                        }
                        return;
                    }
                    keyboard::Key::Named(keyboard::key::Named::PageDown)
                    | keyboard::Key::Named(keyboard::key::Named::PageUp)
                    | keyboard::Key::Named(keyboard::key::Named::Home)
                    | keyboard::Key::Named(keyboard::key::Named::End) => {
                        if !matches.is_empty() {
                            let position = state
                                .selected
                                .and_then(|selected| {
                                    matches.iter().position(|row| *row == selected)
                                });
                            let next_position = match key {
                                keyboard::Key::Named(keyboard::key::Named::Home) => 0,
                                keyboard::Key::Named(keyboard::key::Named::End) => {
                                    matches.len() - 1
                                }
                                keyboard::Key::Named(keyboard::key::Named::PageDown) => (position
                                    .map(|position| position + capacity)
                                    .unwrap_or(0))
                                .min(matches.len() - 1),
                                _ => position
                                    .map(|position| position.saturating_sub(capacity))
                                    .unwrap_or(matches.len() - 1),
                            };
                            state.selected = Some(matches[next_position]);
                            state.scroll = state.scroll.min(next_position);
                            if next_position >= state.scroll + capacity {
                                state.scroll = next_position + 1 - capacity;
                            }
                            shell.invalidate_layout();
                            shell.request_redraw();
                            shell.capture_event();
                        }
                        return;
                    }
                    keyboard::Key::Named(keyboard::key::Named::Enter) => {
                        if let Some(name) = state
                            .selected
                            .and_then(|selected| self.entries.get(selected))
                            .map(|(name, _)| name.clone())
                        {
                            shell.publish(insert_command(&name));
                            shell.capture_event();
                        }
                        return;
                    }
                    _ => {}
                }
                match key {
                    keyboard::Key::Named(keyboard::key::Named::Backspace) => {
                        if state.query.pop().is_some() {
                            state.selected = None;
                            state.scroll = 0;
                            shell.invalidate_layout();
                            shell.request_redraw();
                        }
                        shell.capture_event();
                        return;
                    }
                    keyboard::Key::Named(keyboard::key::Named::Escape) => {}
                    _ => {
                        let typed: String = text
                            .as_deref()
                            .unwrap_or_default()
                            .chars()
                            .filter(|character| {
                                !character.is_control() && !character.is_whitespace()
                            })
                            .collect();
                        if !typed.is_empty() {
                            state.query.push_str(&typed);
                            state.selected = None;
                            state.scroll = 0;
                            shell.invalidate_layout();
                            shell.request_redraw();
                            shell.capture_event();
                            return;
                        }
                    }
                }
            }
        }

        // The wheel scrolls the list by whole rows while the pointer is on it.
        if let Event::Mouse(mouse::Event::WheelScrolled { delta }) = event {
            let over_list = cursor
                .position()
                .is_some_and(|position| list_rect(bounds).contains(position));
            if over_list && matches.len() > capacity {
                let step = match *delta {
                    mouse::ScrollDelta::Lines { y, .. } => -y,
                    mouse::ScrollDelta::Pixels { y, .. } => -y / ROW_H,
                };
                let max_scroll = matches.len() - capacity;
                let next = (state.scroll as f32 + step)
                    .round()
                    .clamp(0.0, max_scroll as f32) as usize;
                if next != state.scroll {
                    state.scroll = next;
                    shell.invalidate_layout();
                    shell.request_redraw();
                }
                shell.capture_event();
            }
        }
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        let Some(position) = cursor.position() else {
            return mouse::Interaction::default();
        };
        let bounds = layout.bounds();
        if search_rect(bounds).contains(position) {
            return mouse::Interaction::Text;
        }
        if list_rect(bounds).contains(position) {
            for (index, child_layout) in layout.children().enumerate() {
                if index >= self.rows.len() || index >= tree.children.len() {
                    break;
                }
                if child_layout.bounds().contains(position) {
                    return self.rows[index].as_widget().mouse_interaction(
                        &tree.children[index],
                        child_layout,
                        cursor,
                        viewport,
                        renderer,
                    );
                }
            }
        }
        mouse::Interaction::default()
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        let palette = theme.palette();
        let state = tree.state.downcast_ref::<State>();
        let matches = self.matches(&state.query);

        // ── Search strip ─────────────────────────────────────────────────
        let field = search_rect(bounds);
        let text_color = palette.background.base.text;
        renderer.fill_quad(
            renderer::Quad {
                bounds: field,
                border: Border {
                    color: palette.primary.base.color,
                    width: 1.0,
                    radius: 3.0.into(),
                },
                ..renderer::Quad::default()
            },
            palette.background.base.color,
        );
        let text_area = Rectangle {
            x: field.x + 8.0,
            y: field.y,
            width: (field.width - 16.0).max(0.0),
            height: field.height,
        };
        let text_bounds = Size::new(text_area.width, text_area.height);
        let text_center = text_area.center_y();
        let text_size = 12.5;
        if state.query.is_empty() {
            draw_text(
                renderer,
                "Search symbols...",
                Point::new(text_area.x, text_center),
                text_size,
                text_color.scale_alpha(0.48),
                advanced_text::Alignment::Left,
                text_bounds,
                field,
            );
        } else {
            draw_text(
                renderer,
                &state.query,
                Point::new(text_area.x, text_center),
                text_size,
                text_color,
                advanced_text::Alignment::Left,
                text_bounds,
                field,
            );
            // The caret after the query, so the field reads as a focused input.
            let caret_x =
                (text_area.x + text_width(renderer, &state.query, text_size) + 1.0)
                    .min(text_area.x + text_area.width - 1.0);
            renderer.fill_quad(
                surface(
                    Rectangle {
                        x: caret_x,
                        y: text_center - 7.0,
                        width: 1.0,
                        height: 14.0,
                    },
                    0.5,
                ),
                palette.primary.base.color,
            );
        }
        if !self.entries.is_empty() {
            let count = format!("{} / {}", matches.len(), self.entries.len());
            draw_text(
                renderer,
                &count,
                Point::new(text_area.x + text_area.width, text_center),
                11.0,
                text_color.scale_alpha(0.6),
                advanced_text::Alignment::Right,
                text_bounds,
                field,
            );
        }

        // ── List ─────────────────────────────────────────────────────────
        let list = list_rect(bounds);
        if list.height <= 0.0 {
            return;
        }
        if matches.is_empty() {
            draw_text(
                renderer,
                "No symbol matches this search.",
                Point::new(list.x + list.width * 0.5, list.y + 22.0),
                12.0,
                text_color.scale_alpha(0.6),
                advanced_text::Alignment::Center,
                Size::new(list.width, 20.0),
                list,
            );
            return;
        }

        renderer.with_layer(list, |renderer| {
            // The keyboard cursor gets a band, so Enter has a visible target.
            if let Some(selected) = state.selected {
                if let Some(position) = matches.iter().position(|row| *row == selected) {
                    let slot = position.saturating_sub(state.scroll);
                    if slot < capacity(bounds.height) {
                        renderer.fill_quad(
                            surface(
                                Rectangle {
                                    x: list.x + 4.0,
                                    y: list.y + slot as f32 * ROW_H + 2.0,
                                    width: (list.width - 10.0).max(0.0),
                                    height: ROW_BUTTON_H,
                                },
                                3.0,
                            ),
                            palette.primary.base.color.scale_alpha(0.18),
                        );
                    }
                }
            }
            for (index, child_layout) in layout.children().enumerate() {
                if index >= self.rows.len() || index >= tree.children.len() {
                    break;
                }
                let child_bounds = child_layout.bounds();
                if child_bounds.width <= 0.0 || child_bounds.height <= 0.0 {
                    continue;
                }
                self.rows[index].as_widget().draw(
                    &tree.children[index],
                    renderer,
                    theme,
                    style,
                    child_layout,
                    cursor,
                    viewport,
                );
            }
        });

        // ── Scrollbar ────────────────────────────────────────────────────
        let capacity = capacity(bounds.height);
        if matches.len() > capacity {
            let track = Rectangle {
                x: bounds.x + bounds.width - 5.0,
                y: list.y + 4.0,
                width: 3.0,
                height: (list.height - 8.0).max(1.0),
            };
            renderer.fill_quad(
                surface(track, 1.5),
                palette.background.weak.color,
            );
            let visible = capacity.min(matches.len()) as f32 / matches.len() as f32;
            let thumb_height = (track.height * visible).max(18.0);
            let max_scroll = (matches.len() - capacity) as f32;
            let progress = if max_scroll > 0.0 {
                state.scroll as f32 / max_scroll
            } else {
                0.0
            };
            renderer.fill_quad(
                surface(
                    Rectangle {
                        x: track.x,
                        y: track.y + (track.height - thumb_height) * progress,
                        width: track.width,
                        height: thumb_height,
                    },
                    1.5,
                ),
                palette.background.strong.color,
            );
        }
    }

    fn overlay<'b>(
        &'b mut self,
        _tree: &'b mut Tree,
        _layout: Layout<'b>,
        _renderer: &Renderer,
        _viewport: &Rectangle,
        _translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
        None
    }
}

/// The palette body.
pub fn view(symbols: &[SymbolRow], sizing: ModalSizing) -> Element<'static, Message> {
    if symbols.is_empty() {
        return container(
            column![
                text("No symbols loaded.").size(13),
                text("Load a library with  ELECLIB <folder>, or press Symbol Library on the Electrical tab.")
                    .size(11)
                    .style(muted_style),
            ]
            .spacing(6),
        )
        .padding(12)
        .width(sizing.width)
        .height(sizing.height)
        .into();
    }

    let entries = symbols
        .iter()
        .map(|(name, class, _)| (name.clone(), class.clone()))
        .collect();
    let rows = symbols
        .iter()
        .enumerate()
        .map(|(index, symbol)| symbol_row(symbol, index))
        .collect();
    container(Element::new(Palette { entries, rows }))
        .width(sizing.width)
        .height(sizing.height)
        .into()
}

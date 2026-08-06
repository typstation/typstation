//! The completion popup and hover tooltip overlays.
//!
//! Both draw directly with the renderer (a bordered box plus text), so the
//! widget stays dependency-light. The completion popup is interactive; the
//! tooltip is passive.

use std::marker::PhantomData;

use iced_core::layout::{self, Layout};
use iced_core::renderer::{self, Quad};
use iced_core::text::{self, Paragraph as _, Text};
use iced_core::{alignment, mouse, overlay, Clipboard, Event, Shell};
use iced_core::{Font, Pixels, Point, Rectangle, Size, Vector};

use crate::action::Action;
use crate::complete::Completion;
use crate::style::{Catalog, Status};
use crate::widget::CompletionUi;

/// Shared geometry for the overlays.
#[derive(Clone, Copy)]
pub(crate) struct PopupMetrics {
    pub font: Font,
    pub size: f32,
    pub line_height: f32,
}

const PADDING: f32 = 6.0;
const MIN_WIDTH: f32 = 140.0;
const COMPLETION_MAX_WIDTH: f32 = 640.0;
const TOOLTIP_MAX_WIDTH: f32 = 460.0;
const DETAIL_SPACING: &str = "    ";

fn popup_text<C>(content: C, metrics: &PopupMetrics, align_x: text::Alignment) -> Text<C, Font> {
    Text {
        content,
        // A large finite bound: an infinite one degenerates the layout in
        // some backends, clipping the text to its first glyph.
        bounds: Size::new(10_000.0, 10_000.0),
        size: Pixels(metrics.size),
        line_height: text::LineHeight::Absolute(Pixels(metrics.line_height)),
        font: metrics.font,
        align_x,
        align_y: alignment::Vertical::Top,
        shaping: text::Shaping::Advanced,
        wrapping: text::Wrapping::None,
    }
}

fn measure<Renderer: text::Renderer<Font = Font>>(content: &str, metrics: &PopupMetrics) -> Size {
    Renderer::Paragraph::with_text(popup_text(content, metrics, text::Alignment::Left)).min_bounds()
}

fn completion_popup_width(content_width: f32, viewport_width: f32) -> f32 {
    let available_content_width = (viewport_width - PADDING * 2.0).max(0.0);
    let content_width = content_width
        .clamp(MIN_WIDTH, COMPLETION_MAX_WIDTH)
        .min(available_content_width);

    (content_width + PADDING * 2.0).min(viewport_width)
}

fn detail_geometry(
    inner: Rectangle,
    y: f32,
    row_height: f32,
    label_width: f32,
    spacing_width: f32,
    detail_width: f32,
) -> Option<(Point, Rectangle)> {
    let left = inner.x + label_width + spacing_width;
    let right = inner.x + inner.width;

    if left >= right {
        return None;
    }

    let position = Point::new((right - detail_width).max(left), y);
    let clip = Rectangle {
        x: left,
        y,
        width: right - left,
        height: row_height,
    };

    Some((position, clip))
}

/// Text that wraps at `max_width`, for the tooltip.
fn wrapped_text(content: String, metrics: &PopupMetrics, max_width: f32) -> Text<String, Font> {
    Text {
        bounds: Size::new(max_width, 10_000.0),
        wrapping: text::Wrapping::Word,
        ..popup_text(content, metrics, text::Alignment::Left)
    }
}

fn draw_text<Renderer: text::Renderer<Font = Font>>(
    renderer: &mut Renderer,
    content: &str,
    position: Point,
    color: iced_core::Color,
    metrics: &PopupMetrics,
    clip: Rectangle,
) {
    renderer.fill_text(
        popup_text(content.to_owned(), metrics, text::Alignment::Left),
        position,
        color,
        clip,
    );
}

/// Clamps a rectangle so it stays inside `viewport` when possible.
fn clamp_within(mut rect: Rectangle, viewport: Size) -> Rectangle {
    if rect.x + rect.width > viewport.width {
        rect.x = (viewport.width - rect.width).max(0.0);
    }
    if rect.x < 0.0 {
        rect.x = 0.0;
    }
    if rect.y + rect.height > viewport.height {
        rect.y = (viewport.height - rect.height).max(0.0);
    }
    if rect.y < 0.0 {
        rect.y = 0.0;
    }
    rect
}

/// The interactive completion popup, anchored to the caret.
///
/// The style is resolved in [`draw`](CompletionPopup::draw) from the theme,
/// because a widget's overlay is built before it is drawn — caching a
/// resolved style would lag by a frame.
pub(crate) struct CompletionPopup<'a, 'b, Message, Theme, Renderer>
where
    Theme: Catalog,
{
    pub slot: &'a mut Option<CompletionUi>,
    /// The candidates to display; owned by the document, cloned in per frame.
    pub items: Vec<Completion>,
    pub on_action: &'a dyn Fn(Action) -> Message,
    /// The caret rectangle, in window coordinates.
    pub caret: Rectangle,
    pub metrics: PopupMetrics,
    pub class: &'a Theme::Class<'b>,
    pub _renderer: PhantomData<Renderer>,
}

impl<Message, Theme, Renderer> CompletionPopup<'_, '_, Message, Theme, Renderer>
where
    Theme: Catalog,
    Renderer: text::Renderer<Font = Font>,
{
    fn ui(&self) -> &CompletionUi {
        self.slot.as_ref().expect("popup shown without a session")
    }

    fn row_height(&self) -> f32 {
        self.metrics.line_height
    }

    /// The item index at a window position, if the position is over a row.
    fn item_at(&self, position: Point, bounds: Rectangle) -> Option<usize> {
        let inner = bounds.shrink(PADDING);

        if !inner.contains(position) {
            return None;
        }

        let row = ((position.y - inner.y) / self.row_height()) as usize;
        let index = self.ui().scroll + row;

        (index < self.items.len()).then_some(index)
    }
}

impl<Message, Theme, Renderer> overlay::Overlay<Message, Theme, Renderer>
    for CompletionPopup<'_, '_, Message, Theme, Renderer>
where
    Theme: Catalog,
    Renderer: text::Renderer<Font = Font>,
{
    fn layout(&mut self, _renderer: &Renderer, bounds: Size) -> layout::Node {
        let visible = self.items.len().min(CompletionUi::VISIBLE);

        let content_width = self
            .items
            .iter()
            .skip(self.ui().scroll)
            .take(visible)
            .map(|item| {
                let mut text = item.label.clone();
                if let Some(detail) = &item.detail {
                    text.push_str(DETAIL_SPACING);
                    text.push_str(detail);
                }
                measure::<Renderer>(&text, &self.metrics).width
            })
            .fold(MIN_WIDTH, f32::max);
        let width = completion_popup_width(content_width, bounds.width);

        let height = visible as f32 * self.row_height() + PADDING * 2.0;

        // Prefer below the caret; flip above if it would overflow downward.
        let below = self.caret.y + self.caret.height;
        let y = if below + height > bounds.height && self.caret.y - height >= 0.0 {
            self.caret.y - height
        } else {
            below
        };

        let rect = clamp_within(
            Rectangle {
                x: self.caret.x,
                y,
                width,
                height,
            },
            bounds,
        );

        layout::Node::new(rect.size()).move_to(rect.position())
    }

    fn draw(
        &self,
        renderer: &mut Renderer,
        theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
    ) {
        let bounds = layout.bounds();
        let ui = self.ui();
        let style = theme
            .style(self.class, Status::Focused { is_hovered: false })
            .popup;

        renderer.fill_quad(
            Quad {
                bounds,
                border: style.border,
                ..Quad::default()
            },
            style.background,
        );

        let inner = bounds.shrink(PADDING);
        let row_height = self.row_height();
        let visible = self.items.len().min(CompletionUi::VISIBLE);

        for row in 0..visible {
            let index = ui.scroll + row;
            let Some(item) = self.items.get(index) else {
                break;
            };

            let y = inner.y + row as f32 * row_height;

            if index == ui.selected {
                renderer.fill_quad(
                    Quad {
                        bounds: Rectangle {
                            x: bounds.x + 2.0,
                            y,
                            width: bounds.width - 4.0,
                            height: row_height,
                        },
                        border: iced_core::Border {
                            radius: 2.0.into(),
                            ..Default::default()
                        },
                        ..Quad::default()
                    },
                    style.selection,
                );
            }

            draw_text(
                renderer,
                &item.label,
                Point::new(inner.x, y),
                style.text,
                &self.metrics,
                bounds,
            );

            if let Some(detail) = &item.detail {
                let label_width = measure::<Renderer>(&item.label, &self.metrics).width;
                let spacing_width = measure::<Renderer>(DETAIL_SPACING, &self.metrics).width;
                let detail_width = measure::<Renderer>(detail, &self.metrics).width;

                if let Some((position, clip)) = detail_geometry(
                    inner,
                    y,
                    row_height,
                    label_width,
                    spacing_width,
                    detail_width,
                ) {
                    draw_text(
                        renderer,
                        detail,
                        position,
                        style.dim_text,
                        &self.metrics,
                        clip,
                    );
                }
            }
        }
    }

    fn update(
        &mut self,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _renderer: &Renderer,
        _clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
    ) {
        let bounds = layout.bounds();

        match event {
            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                if let Some(position) = cursor.position() {
                    if let Some(index) = self.item_at(position, bounds) {
                        if let Some(ui) = self.slot.as_mut() {
                            if ui.selected != index {
                                ui.selected = index;
                                shell.request_redraw();
                            }
                        }
                    }
                }
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let Some(position) = cursor.position() else {
                    return;
                };

                if let Some(index) = self.item_at(position, bounds) {
                    let item = self.items[index].clone();
                    shell.publish((self.on_action)(Action::Replace {
                        range: item.replace,
                        text: item.insert,
                    }));
                    *self.slot = None;
                    shell.capture_event();
                    shell.request_redraw();
                } else if !bounds.contains(position) {
                    // A click outside dismisses the popup.
                    *self.slot = None;
                    shell.request_redraw();
                }
            }
            Event::Mouse(mouse::Event::WheelScrolled { delta }) if cursor.is_over(bounds) => {
                let lines = match delta {
                    mouse::ScrollDelta::Lines { y, .. } => *y,
                    mouse::ScrollDelta::Pixels { y, .. } => y / self.row_height(),
                };

                let max_scroll = self.items.len().saturating_sub(CompletionUi::VISIBLE);
                if let Some(ui) = self.slot.as_mut() {
                    let next = (ui.scroll as f32 - lines).round();
                    ui.scroll = next.clamp(0.0, max_scroll as f32) as usize;
                    shell.request_redraw();
                }
                shell.capture_event();
            }
            _ => {}
        }
    }

    fn mouse_interaction(
        &self,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _renderer: &Renderer,
    ) -> mouse::Interaction {
        if cursor.is_over(layout.bounds()) {
            mouse::Interaction::Pointer
        } else {
            mouse::Interaction::None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn completion_popup_expands_for_long_details() {
        assert_eq!(completion_popup_width(560.0, 800.0), 572.0);
    }

    #[test]
    fn completion_popup_stays_inside_a_narrow_viewport() {
        assert_eq!(completion_popup_width(560.0, 320.0), 320.0);
    }

    #[test]
    fn completion_detail_is_right_aligned_when_it_fits() {
        let inner = Rectangle::new(Point::ORIGIN, Size::new(500.0, 200.0));
        let (position, clip) =
            detail_geometry(inner, 20.0, 18.0, 80.0, 20.0, 200.0).expect("detail fits");

        assert_eq!(position, Point::new(300.0, 20.0));
        assert_eq!(
            clip,
            Rectangle::new(Point::new(100.0, 20.0), Size::new(400.0, 18.0))
        );
    }

    #[test]
    fn long_completion_detail_starts_after_the_label() {
        let inner = Rectangle::new(Point::ORIGIN, Size::new(500.0, 200.0));
        let (position, clip) =
            detail_geometry(inner, 20.0, 18.0, 80.0, 20.0, 600.0).expect("detail has room");

        assert_eq!(position, Point::new(100.0, 20.0));
        assert_eq!(
            clip,
            Rectangle::new(Point::new(100.0, 20.0), Size::new(400.0, 18.0))
        );
    }
}

/// The passive hover tooltip, anchored near the pointer.
pub(crate) struct Tooltip<'a, 'b, Theme, Renderer>
where
    Theme: Catalog,
{
    pub content: String,
    /// The pointer position, in window coordinates.
    pub anchor: Point,
    pub metrics: PopupMetrics,
    pub class: &'a Theme::Class<'b>,
    pub _renderer: PhantomData<Renderer>,
}

impl<Message, Theme, Renderer> overlay::Overlay<Message, Theme, Renderer>
    for Tooltip<'_, '_, Theme, Renderer>
where
    Theme: Catalog,
    Renderer: text::Renderer<Font = Font>,
{
    fn layout(&mut self, _renderer: &Renderer, bounds: Size) -> layout::Node {
        // Long hover content wraps instead of being clipped at the maximum width.
        let max_width = TOOLTIP_MAX_WIDTH - PADDING * 2.0;
        let size = Renderer::Paragraph::with_text(
            wrapped_text(self.content.clone(), &self.metrics, max_width).as_ref(),
        )
        .min_bounds();

        let rect = clamp_within(
            Rectangle {
                x: self.anchor.x + 12.0,
                y: self.anchor.y + 18.0,
                width: size.width.min(max_width) + PADDING * 2.0,
                height: size.height + PADDING * 2.0,
            },
            bounds,
        );

        layout::Node::new(rect.size()).move_to(rect.position())
    }

    fn draw(
        &self,
        renderer: &mut Renderer,
        theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
    ) {
        let bounds = layout.bounds();
        let style = theme
            .style(self.class, Status::Focused { is_hovered: false })
            .popup;

        renderer.fill_quad(
            Quad {
                bounds,
                border: style.border,
                ..Quad::default()
            },
            style.background,
        );

        renderer.fill_text(
            wrapped_text(
                self.content.clone(),
                &self.metrics,
                bounds.width - PADDING * 2.0,
            ),
            bounds.position() + Vector::new(PADDING, PADDING),
            style.text,
            bounds,
        );
    }
}

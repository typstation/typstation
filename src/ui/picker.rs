//! Picker baseado no componente Picker do Adobe Spectrum 2.

use std::borrow::Borrow;

use iced::{
    Alignment, Background, Border, Element, Event, Length, Padding, Rectangle, Shadow, Size, Theme,
    Vector,
    advanced::{
        Clipboard, Layout, Shell, Widget, layout, mouse, overlay, renderer,
        widget::{self, Tree},
    },
    widget::{Stack, container, pick_list, svg},
};

use super::{
    icons::WorkflowIcon,
    tokens::{self, SpectrumColors},
};

pub fn spectrum_picker<'a, T, L, V, Message>(
    options: L,
    selected: Option<V>,
    on_select: impl Fn(T) -> Message + 'a,
    width: f32,
) -> Element<'a, Message>
where
    T: ToString + PartialEq + Clone + 'a,
    L: Borrow<[T]> + 'a,
    V: Borrow<T> + 'a,
    Message: Clone + 'a,
{
    let width = spectrum_picker_width(width);
    let picker: Element<'a, Message> = pick_list(options, selected, on_select)
        .width(Length::Fixed(width))
        .padding(Padding {
            top: tokens::spacing::FIELD_TOP_TO_TEXT_MEDIUM,
            right: picker_handle_slot_width(),
            bottom: tokens::spacing::FIELD_TOP_TO_TEXT_MEDIUM,
            left: tokens::spacing::PICKER_EDGE_TO_TEXT_MEDIUM,
        })
        .text_size(tokens::typography::FONT_SIZE_100)
        .text_line_height(iced::widget::text::LineHeight::Absolute(iced::Pixels(20.0)))
        .handle(iced::widget::pick_list::Handle::None)
        .style(spectrum_picker_style)
        .menu_style(spectrum_picker_menu_style)
        .into();
    let picker: Element<'a, Message> =
        MenuGap::new(picker, tokens::spacing::PICKER_TO_MENU_MEDIUM).into();
    let handle = svg(WorkflowIcon::ChevronDown.handle())
        .width(Length::Fixed(tokens::dimension::PICKER_HANDLE_SIZE_MEDIUM))
        .height(Length::Fixed(tokens::dimension::PICKER_HANDLE_SIZE_MEDIUM))
        .style(picker_handle_style);
    let handle = container(handle)
        .width(Length::Fixed(width))
        .height(Length::Fixed(tokens::dimension::FIELD_HEIGHT_MEDIUM))
        .padding([0.0, tokens::spacing::PICKER_EDGE_TO_TEXT_MEDIUM])
        .align_x(Alignment::End)
        .align_y(Alignment::Center);

    Stack::new()
        .width(Length::Fixed(width))
        .height(Length::Fixed(tokens::dimension::FIELD_HEIGHT_MEDIUM))
        .push(picker)
        .push(handle)
        .into()
}

fn spectrum_picker_width(requested: f32) -> f32 {
    requested.max(tokens::dimension::PICKER_MINIMUM_WIDTH_MEDIUM)
        + tokens::dimension::PICKER_HANDLE_SIZE_MEDIUM
        + tokens::spacing::PICKER_TEXT_TO_HANDLE_MEDIUM
}

fn picker_handle_slot_width() -> f32 {
    tokens::spacing::PICKER_EDGE_TO_TEXT_MEDIUM
        + tokens::dimension::PICKER_HANDLE_SIZE_MEDIUM
        + tokens::spacing::PICKER_TEXT_TO_HANDLE_MEDIUM
}

fn spectrum_picker_style(
    theme: &Theme,
    status: iced::widget::pick_list::Status,
) -> iced::widget::pick_list::Style {
    let colors = SpectrumColors::from_theme(theme);
    let (background, border_color, border_width) = match status {
        iced::widget::pick_list::Status::Active => {
            (colors.gray.gray_100, colors.gray.gray_500, 1.0)
        }
        iced::widget::pick_list::Status::Hovered => {
            (colors.gray.gray_100, colors.gray.gray_700, 1.0)
        }
        iced::widget::pick_list::Status::Opened { .. } => (
            colors.gray.gray_100,
            colors.focus_indicator,
            tokens::dimension::FOCUS_RING_THICKNESS,
        ),
    };

    iced::widget::pick_list::Style {
        text_color: colors.gray.gray_800,
        placeholder_color: colors.gray.gray_600,
        handle_color: colors.gray.gray_800,
        background: Background::Color(background),
        border: Border {
            color: border_color,
            width: border_width,
            radius: tokens::dimension::CORNER_RADIUS_100.into(),
        },
    }
}

fn spectrum_picker_menu_style(theme: &Theme) -> iced::widget::overlay::menu::Style {
    let colors = SpectrumColors::from_theme(theme);
    let shadow_opacity = if tokens::ColorScheme::from_theme(theme) == tokens::ColorScheme::Dark {
        0.48
    } else {
        0.16
    };

    iced::widget::overlay::menu::Style {
        background: Background::Color(colors.gray.gray_50),
        border: Border {
            color: colors.gray.gray_300,
            width: 1.0,
            radius: tokens::dimension::MENU_POPOVER_CORNER_RADIUS.into(),
        },
        text_color: colors.gray.gray_800,
        selected_text_color: colors.gray.gray_800,
        selected_background: Background::Color(colors.gray.gray_200),
        shadow: Shadow {
            color: iced::Color::from_rgba(0.0, 0.0, 0.0, shadow_opacity),
            offset: Vector::new(0.0, 2.0),
            blur_radius: 8.0,
        },
    }
}

fn picker_handle_style(theme: &Theme, _status: svg::Status) -> svg::Style {
    svg::Style {
        color: Some(SpectrumColors::from_theme(theme).neutral_content.default),
    }
}

struct MenuGap<'a, Message> {
    content: Element<'a, Message>,
    gap: f32,
}

impl<'a, Message> MenuGap<'a, Message> {
    fn new(content: Element<'a, Message>, gap: f32) -> Self {
        Self { content, gap }
    }
}

impl<Message> Widget<Message, Theme, iced::Renderer> for MenuGap<'_, Message> {
    fn tag(&self) -> widget::tree::Tag {
        self.content.as_widget().tag()
    }

    fn state(&self) -> widget::tree::State {
        self.content.as_widget().state()
    }

    fn children(&self) -> Vec<Tree> {
        self.content.as_widget().children()
    }

    fn diff(&self, tree: &mut Tree) {
        self.content.as_widget().diff(tree);
    }

    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }

    fn size_hint(&self) -> Size<Length> {
        self.content.as_widget().size_hint()
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.content.as_widget_mut().layout(tree, renderer, limits)
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &iced::Renderer,
        operation: &mut dyn widget::Operation,
    ) {
        self.content
            .as_widget_mut()
            .operate(tree, layout, renderer, operation);
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &iced::Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        self.content.as_widget_mut().update(
            tree, event, layout, cursor, renderer, clipboard, shell, viewport,
        );
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut iced::Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        self.content
            .as_widget()
            .draw(tree, renderer, theme, style, layout, cursor, viewport);
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        self.content
            .as_widget()
            .mouse_interaction(tree, layout, cursor, viewport, renderer)
    }

    fn overlay<'a>(
        &'a mut self,
        tree: &'a mut Tree,
        layout: Layout<'a>,
        renderer: &iced::Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'a, Message, Theme, iced::Renderer>> {
        let anchor = layout.bounds() + translation;

        self.content
            .as_widget_mut()
            .overlay(tree, layout, renderer, viewport, translation)
            .map(|content| {
                overlay::Element::new(Box::new(OffsetOverlay {
                    content,
                    anchor,
                    gap: self.gap,
                }))
            })
    }
}

impl<'a, Message: 'a> From<MenuGap<'a, Message>> for Element<'a, Message> {
    fn from(menu_gap: MenuGap<'a, Message>) -> Self {
        Element::new(menu_gap)
    }
}

struct OffsetOverlay<'a, Message> {
    content: overlay::Element<'a, Message, Theme, iced::Renderer>,
    anchor: Rectangle,
    gap: f32,
}

impl<Message> overlay::Overlay<Message, Theme, iced::Renderer> for OffsetOverlay<'_, Message> {
    fn layout(&mut self, renderer: &iced::Renderer, bounds: Size) -> layout::Node {
        let node = self.content.as_overlay_mut().layout(renderer, bounds);
        let translation = menu_gap_translation(node.bounds(), self.anchor, self.gap);

        node.translate(Vector::new(0.0, translation))
    }

    fn operate(
        &mut self,
        layout: Layout<'_>,
        renderer: &iced::Renderer,
        operation: &mut dyn widget::Operation,
    ) {
        self.content
            .as_overlay_mut()
            .operate(layout, renderer, operation);
    }

    fn update(
        &mut self,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &iced::Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
    ) {
        self.content
            .as_overlay_mut()
            .update(event, layout, cursor, renderer, clipboard, shell);
    }

    fn mouse_interaction(
        &self,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        self.content
            .as_overlay()
            .mouse_interaction(layout, cursor, renderer)
    }

    fn draw(
        &self,
        renderer: &mut iced::Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
    ) {
        self.content
            .as_overlay()
            .draw(renderer, theme, style, layout, cursor);
    }

    fn overlay<'a>(
        &'a mut self,
        layout: Layout<'a>,
        renderer: &iced::Renderer,
    ) -> Option<overlay::Element<'a, Message, Theme, iced::Renderer>> {
        self.content.as_overlay_mut().overlay(layout, renderer)
    }

    fn index(&self) -> f32 {
        self.content.as_overlay().index()
    }
}

fn menu_gap_translation(menu: Rectangle, anchor: Rectangle, gap: f32) -> f32 {
    if menu.y >= anchor.y + anchor.height {
        gap
    } else {
        -gap
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picker_enforces_the_spectrum_minimum_width() {
        assert_eq!(
            spectrum_picker_width(40.0),
            tokens::dimension::PICKER_MINIMUM_WIDTH_MEDIUM
                + tokens::dimension::PICKER_HANDLE_SIZE_MEDIUM
                + tokens::spacing::PICKER_TEXT_TO_HANDLE_MEDIUM
        );
        assert_eq!(spectrum_picker_width(120.0), 138.0);
        assert_eq!(picker_handle_slot_width(), 30.0);
    }

    #[test]
    fn opened_picker_uses_the_focus_indicator() {
        let theme = super::super::spectrum_theme(tokens::ColorScheme::Light);
        let style = spectrum_picker_style(
            &theme,
            iced::widget::pick_list::Status::Opened { is_hovered: false },
        );

        assert_eq!(
            style.border.color,
            SpectrumColors::from_theme(&theme).focus_indicator
        );
        assert_eq!(style.border.width, tokens::dimension::FOCUS_RING_THICKNESS);
    }

    #[test]
    fn menu_gap_moves_away_from_the_picker_in_both_directions() {
        let anchor = Rectangle::new(iced::Point::new(10.0, 100.0), Size::new(120.0, 32.0));
        let below = Rectangle::new(iced::Point::new(10.0, 132.0), Size::new(120.0, 96.0));
        let above = Rectangle::new(iced::Point::new(10.0, 4.0), Size::new(120.0, 96.0));
        let gap = tokens::spacing::PICKER_TO_MENU_MEDIUM;

        assert_eq!(gap, 8.0);
        assert_eq!(menu_gap_translation(below, anchor, gap), gap);
        assert_eq!(menu_gap_translation(above, anchor, gap), -gap);
    }
}

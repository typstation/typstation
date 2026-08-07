//! Superfícies semânticas compartilhadas pela composição da aplicação.

use iced::widget::{Space, Stack, button, column, container, pane_grid, text};
use iced::{Background, Border, Color, Element, Length, Shadow, Theme, Vector};

use super::tokens::{self, SpectrumColors};

pub fn layer_style(theme: &Theme) -> container::Style {
    container::Style::default().background(SpectrumColors::from_theme(theme).gray.gray_25)
}

pub fn bar_style(theme: &Theme) -> container::Style {
    container::Style::default().background(SpectrumColors::from_theme(theme).gray.gray_50)
}

pub fn divider_style(theme: &Theme) -> container::Style {
    container::Style::default().background(SpectrumColors::from_theme(theme).gray.gray_300)
}

pub fn with_top_divider<'a, Message>(
    content: impl Into<Element<'a, Message>>,
) -> Element<'a, Message>
where
    Message: 'a,
{
    with_divider(content, DividerEdge::Top)
}

pub fn with_bottom_divider<'a, Message>(
    content: impl Into<Element<'a, Message>>,
) -> Element<'a, Message>
where
    Message: 'a,
{
    with_divider(content, DividerEdge::Bottom)
}

pub fn vertical_divider<'a, Message>(height: f32) -> Element<'a, Message>
where
    Message: 'a,
{
    container(Space::new())
        .width(Length::Fixed(tokens::dimension::BORDER_WIDTH_100))
        .height(Length::Fixed(height))
        .style(divider_style)
        .into()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DividerEdge {
    Top,
    Bottom,
}

fn with_divider<'a, Message>(
    content: impl Into<Element<'a, Message>>,
    edge: DividerEdge,
) -> Element<'a, Message>
where
    Message: 'a,
{
    let divider = container(Space::new())
        .width(Length::Fill)
        .height(Length::Fixed(tokens::dimension::BORDER_WIDTH_100))
        .style(divider_style);
    let divider_layer = match edge {
        DividerEdge::Top => column![divider, Space::new().height(Length::Fill)],
        DividerEdge::Bottom => column![Space::new().height(Length::Fill), divider],
    }
    .width(Length::Fill)
    .height(Length::Fill);

    Stack::new()
        .width(Length::Fill)
        .height(Length::Shrink)
        .push(content)
        .push(divider_layer)
        .into()
}

pub fn metadata_text_style(theme: &Theme) -> text::Style {
    text::Style {
        color: Some(SpectrumColors::from_theme(theme).gray.gray_600),
    }
}

pub fn selectable_row_style(theme: &Theme, status: button::Status) -> button::Style {
    let colors = SpectrumColors::from_theme(theme);
    let background = match status {
        button::Status::Hovered => Some(Background::Color(colors.gray.gray_100)),
        button::Status::Pressed => Some(Background::Color(colors.gray.gray_200)),
        button::Status::Active | button::Status::Disabled => None,
    };

    button::Style {
        background,
        text_color: if status == button::Status::Disabled {
            colors.disabled_content
        } else {
            colors.neutral_content.default
        },
        border: Border::default(),
        ..button::Style::default()
    }
}

pub fn modal_backdrop_style(_theme: &Theme) -> container::Style {
    container::Style::default().background(Color::from_rgba(0.0, 0.0, 0.0, 0.40))
}

pub fn elevated_dialog_style(theme: &Theme) -> container::Style {
    let colors = SpectrumColors::from_theme(theme);

    container::Style {
        background: Some(Background::Color(colors.gray.gray_50)),
        border: Border {
            color: colors.gray.gray_300,
            width: 1.0,
            radius: tokens::dimension::CORNER_RADIUS_500.into(),
        },
        shadow: Shadow {
            color: Color::from_rgba(0.0, 0.0, 0.0, 0.28),
            offset: Vector::new(0.0, 6.0),
            blur_radius: 18.0,
        },
        ..container::Style::default()
    }
}

pub fn split_view_style(theme: &Theme) -> pane_grid::Style {
    let colors = SpectrumColors::from_theme(theme);

    pane_grid::Style {
        hovered_region: pane_grid::Highlight {
            background: Background::Color(Color {
                a: 0.08,
                ..colors.focus_indicator
            }),
            border: Border {
                color: colors.focus_indicator,
                width: tokens::dimension::FOCUS_RING_THICKNESS,
                ..Border::default()
            },
        },
        picked_split: pane_grid::Line {
            color: colors.focus_indicator,
            width: tokens::dimension::SPLIT_VIEW_DIVIDER_INTERACTION_THICKNESS,
        },
        hovered_split: pane_grid::Line {
            color: colors.gray.gray_600,
            width: tokens::dimension::SPLIT_VIEW_DIVIDER_INTERACTION_THICKNESS,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bars_use_layers_without_drawing_a_frame() {
        for scheme in [tokens::ColorScheme::Light, tokens::ColorScheme::Dark] {
            let theme = super::super::spectrum_theme(scheme);
            let style = bar_style(&theme);

            assert_eq!(style.border.width, 0.0);
            assert_eq!(
                style.background,
                Some(Background::Color(
                    SpectrumColors::for_scheme(scheme).gray.gray_50
                ))
            );
        }
    }

    #[test]
    fn structural_dividers_use_the_small_border_token() {
        assert_eq!(tokens::dimension::BORDER_WIDTH_100, 1.0);
    }
}

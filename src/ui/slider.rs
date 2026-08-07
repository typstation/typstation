//! Slider médio baseado no componente Slider do Adobe Spectrum 2.

use std::ops::RangeInclusive;

use iced::{Background, Border, Theme, widget::Slider};

use super::tokens::{self, SpectrumColors};

pub fn spectrum_slider<'a, T, Message>(
    range: RangeInclusive<T>,
    value: T,
    on_change: impl Fn(T) -> Message + 'a,
) -> Slider<'a, T, Message>
where
    T: Copy + From<u8> + PartialOrd,
    Message: Clone,
{
    iced::widget::slider(range, value, on_change)
        .height(tokens::dimension::SLIDER_CONTROL_HEIGHT_MEDIUM)
        .style(spectrum_slider_style)
}

fn spectrum_slider_style(
    theme: &Theme,
    status: iced::widget::slider::Status,
) -> iced::widget::slider::Style {
    let colors = SpectrumColors::from_theme(theme);
    let handle_color = match status {
        iced::widget::slider::Status::Active => colors.neutral_content.default,
        iced::widget::slider::Status::Hovered => colors.neutral_content.hover,
        iced::widget::slider::Status::Dragged => colors.neutral_content.down,
    };
    let track_color = colors.gray.gray_400;

    iced::widget::slider::Style {
        rail: iced::widget::slider::Rail {
            // O Slider Spectrum não possui preenchimento por padrão.
            backgrounds: (
                Background::Color(track_color),
                Background::Color(track_color),
            ),
            width: tokens::dimension::SLIDER_TRACK_THICKNESS,
            border: Border {
                radius: (tokens::dimension::SLIDER_TRACK_THICKNESS / 2.0).into(),
                ..Border::default()
            },
        },
        handle: iced::widget::slider::Handle {
            shape: iced::widget::slider::HandleShape::Circle {
                radius: tokens::dimension::SLIDER_HANDLE_SIZE_MEDIUM / 2.0,
            },
            background: Background::Color(handle_color),
            border_width: 0.0,
            border_color: iced::Color::TRANSPARENT,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn medium_slider_uses_component_specific_spectrum_tokens() {
        let theme = super::super::spectrum_theme(tokens::ColorScheme::Light);
        let style = spectrum_slider_style(&theme, iced::widget::slider::Status::Active);

        assert_eq!(style.rail.width, 2.0);
        assert_eq!(
            style.handle.shape,
            iced::widget::slider::HandleShape::Circle { radius: 8.0 }
        );
    }

    #[test]
    fn default_slider_track_has_no_progress_fill() {
        let theme = super::super::spectrum_theme(tokens::ColorScheme::Dark);
        let style = spectrum_slider_style(&theme, iced::widget::slider::Status::Active);

        assert_eq!(style.rail.backgrounds.0, style.rail.backgrounds.1);
    }
}

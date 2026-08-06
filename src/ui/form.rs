//! Campos e controles de formulário baseados no Adobe Spectrum 2.

use iced::{
    Background, Border, Color, Element, Length, Padding, Theme,
    widget::{Checkbox, Stack, TextInput, checkbox, container, row, svg, text, text_input},
};

use super::{
    ActionButtonOptions, ActionButtonSize,
    button::workflow_icon_action_button_with_tooltip,
    icons::WorkflowIcon,
    tokens::{self, SpectrumColors},
};

pub fn spectrum_checkbox<'a, Message>(
    label: impl text::IntoFragment<'a>,
    is_checked: bool,
    on_toggle: impl Fn(bool) -> Message + 'a,
) -> Checkbox<'a, Message>
where
    Message: 'a,
{
    checkbox(is_checked)
        .label(label)
        .on_toggle(on_toggle)
        .size(tokens::dimension::CHECKBOX_SIZE_MEDIUM)
        .spacing(tokens::spacing::CHECKBOX_TO_LABEL)
        .text_size(tokens::typography::FONT_SIZE_100)
        .style(spectrum_checkbox_style)
}

pub fn spectrum_text_field<'a, Message>(
    label: &str,
    value: &str,
    on_input: impl Fn(String) -> Message + 'a,
    on_submit: Option<Message>,
    width: Length,
) -> TextInput<'a, Message>
where
    Message: Clone + 'a,
{
    text_input(label, value)
        .on_input(on_input)
        .on_submit_maybe(on_submit)
        .width(width)
        .padding([6.0, tokens::spacing::FIELD_EDGE_TO_TEXT_MEDIUM])
        .size(tokens::typography::FONT_SIZE_100)
        .line_height(text::LineHeight::Absolute(iced::Pixels(20.0)))
        .style(spectrum_text_field_style)
}

pub fn search_field<'a, Message>(
    label: &str,
    value: &str,
    id: iced::widget::Id,
    on_input: impl Fn(String) -> Message + 'a,
    on_submit: Message,
    on_clear: Option<Message>,
    width: Length,
) -> Element<'a, Message>
where
    Message: Clone + 'a,
{
    let field = spectrum_text_field(label, value, on_input, Some(on_submit), width)
        .id(id)
        .padding(Padding {
            top: 6.0,
            right: tokens::spacing::SEARCH_FIELD_ICON_SLOT,
            bottom: 6.0,
            left: tokens::spacing::SEARCH_FIELD_ICON_SLOT,
        });
    let search_icon = svg(WorkflowIcon::Search.handle())
        .width(Length::Fixed(tokens::icon::SEARCH_FIELD_ICON_SIZE))
        .height(Length::Fixed(tokens::icon::SEARCH_FIELD_ICON_SIZE))
        .style(search_icon_style);
    let leading = container(search_icon)
        .height(Length::Fill)
        .center_x(Length::Fixed(tokens::spacing::SEARCH_FIELD_ICON_SLOT))
        .center_y(Length::Fill);
    let clear: Element<'a, Message> = if value.is_empty() {
        iced::widget::Space::new()
            .width(Length::Fixed(tokens::spacing::SEARCH_FIELD_ICON_SLOT))
            .into()
    } else {
        container(workflow_icon_action_button_with_tooltip(
            WorkflowIcon::Close,
            "Limpar busca",
            on_clear,
            ActionButtonOptions::QUIET.size(ActionButtonSize::ExtraSmall),
            iced::widget::tooltip::Position::Bottom,
        ))
        .height(Length::Fill)
        .center_x(Length::Fixed(tokens::spacing::SEARCH_FIELD_ICON_SLOT))
        .center_y(Length::Fill)
        .into()
    };
    let decorations = row![
        leading,
        iced::widget::Space::new().width(Length::Fill),
        clear
    ]
    .width(width)
    .height(Length::Fixed(tokens::dimension::FIELD_HEIGHT_MEDIUM));

    Stack::new()
        .width(width)
        .height(Length::Fixed(tokens::dimension::FIELD_HEIGHT_MEDIUM))
        .push(field)
        .push(decorations)
        .into()
}

fn spectrum_text_field_style(theme: &Theme, status: text_input::Status) -> text_input::Style {
    let colors = SpectrumColors::from_theme(theme);
    let (background, border_color, border_width, value) = match status {
        text_input::Status::Active => (
            colors.gray.gray_50,
            colors.gray.gray_400,
            1.0,
            colors.neutral_content.default,
        ),
        text_input::Status::Hovered => (
            colors.gray.gray_50,
            colors.gray.gray_600,
            1.0,
            colors.neutral_content.hover,
        ),
        text_input::Status::Focused { .. } => (
            colors.gray.gray_50,
            colors.focus_indicator,
            tokens::dimension::FOCUS_RING_THICKNESS,
            colors.neutral_content.default,
        ),
        text_input::Status::Disabled => (
            colors.disabled_background,
            colors.disabled_border,
            1.0,
            colors.disabled_content,
        ),
    };

    text_input::Style {
        background: Background::Color(background),
        border: Border {
            color: border_color,
            width: border_width,
            radius: tokens::dimension::CORNER_RADIUS_100.into(),
        },
        icon: colors.neutral_content.default,
        placeholder: colors.gray.gray_600,
        value,
        selection: colors.accent_background.default,
    }
}

fn spectrum_checkbox_style(theme: &Theme, status: checkbox::Status) -> checkbox::Style {
    let colors = SpectrumColors::from_theme(theme);
    let (is_checked, hovered, disabled) = match status {
        checkbox::Status::Active { is_checked } => (is_checked, false, false),
        checkbox::Status::Hovered { is_checked } => (is_checked, true, false),
        checkbox::Status::Disabled { is_checked } => (is_checked, false, true),
    };
    let background = if disabled {
        colors.disabled_background
    } else if is_checked {
        if hovered {
            colors.accent_background.hover
        } else {
            colors.accent_background.default
        }
    } else if hovered {
        colors.gray.gray_100
    } else {
        colors.gray.gray_25
    };
    let border_color = if disabled {
        colors.disabled_border
    } else if is_checked {
        colors.accent_background.default
    } else if hovered {
        colors.gray.gray_800
    } else {
        colors.gray.gray_600
    };

    checkbox::Style {
        background: Background::Color(background),
        icon_color: if disabled {
            colors.disabled_content
        } else {
            Color::WHITE
        },
        border: Border {
            color: border_color,
            width: 1.0,
            radius: tokens::dimension::CHECKBOX_CORNER_RADIUS.into(),
        },
        text_color: Some(if disabled {
            colors.disabled_content
        } else {
            colors.neutral_content.default
        }),
    }
}

fn search_icon_style(theme: &Theme, _status: svg::Status) -> svg::Style {
    svg::Style {
        color: Some(SpectrumColors::from_theme(theme).neutral_content.default),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn medium_fields_reuse_the_medium_component_height() {
        assert_eq!(
            tokens::dimension::FIELD_HEIGHT_MEDIUM,
            tokens::dimension::COMPONENT_HEIGHT_100
        );
        assert_eq!(tokens::spacing::SEARCH_FIELD_ICON_SLOT, 36.0);
    }
}

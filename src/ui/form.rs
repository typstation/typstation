//! Campos e controles de formulário baseados no Adobe Spectrum 2.

use iced::{
    Background, Border, Color, Element, Length, Padding, Theme,
    widget::{
        Checkbox, Stack, TextInput, Toggler, checkbox, column, container, row, svg, text,
        text_input, toggler,
    },
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

pub fn spectrum_switch<'a, Message>(
    label: impl text::IntoFragment<'a>,
    is_toggled: bool,
    on_toggle: impl Fn(bool) -> Message + 'a,
) -> Toggler<'a, Message>
where
    Message: 'a,
{
    toggler(is_toggled)
        .label(label)
        .on_toggle(on_toggle)
        .size(tokens::dimension::SWITCH_CONTROL_HEIGHT_MEDIUM)
        .spacing(tokens::spacing::SWITCH_TO_LABEL)
        .text_size(tokens::typography::FONT_SIZE_100)
        .style(spectrum_switch_style)
}

pub fn spectrum_text_field<'a, Message>(
    label: &'a str,
    value: &'a str,
    on_input: impl Fn(String) -> Message + 'a,
    on_submit: Option<Message>,
    width: Length,
) -> Element<'a, Message>
where
    Message: Clone + 'a,
{
    labeled_text_field(label, value, on_input, on_submit, width, None)
}

pub fn spectrum_text_field_with_id<'a, Message>(
    label: &'a str,
    value: &'a str,
    id: iced::widget::Id,
    on_input: impl Fn(String) -> Message + 'a,
    on_submit: Option<Message>,
    width: Length,
) -> Element<'a, Message>
where
    Message: Clone + 'a,
{
    labeled_text_field(label, value, on_input, on_submit, width, Some(id))
}

fn labeled_text_field<'a, Message>(
    label: &'a str,
    value: &'a str,
    on_input: impl Fn(String) -> Message + 'a,
    on_submit: Option<Message>,
    width: Length,
    id: Option<iced::widget::Id>,
) -> Element<'a, Message>
where
    Message: Clone + 'a,
{
    let label = text(label)
        .size(tokens::typography::FONT_SIZE_75)
        .line_height(tokens::typography::LINE_HEIGHT_100)
        .style(field_label_style);
    let control = text_field_control("", value, on_input, on_submit, width);
    let control = match id {
        Some(id) => control.id(id),
        None => control,
    };

    column![label, control]
        .width(width)
        .spacing(tokens::spacing::FIELD_LABEL_TO_CONTROL)
        .into()
}

fn text_field_control<'a, Message>(
    prompt: &'a str,
    value: &'a str,
    on_input: impl Fn(String) -> Message + 'a,
    on_submit: Option<Message>,
    width: Length,
) -> TextInput<'a, Message>
where
    Message: Clone + 'a,
{
    text_input(prompt, value)
        .on_input(on_input)
        .on_submit_maybe(on_submit)
        .width(width)
        .padding([
            tokens::spacing::FIELD_TOP_TO_TEXT_MEDIUM,
            tokens::spacing::FIELD_EDGE_TO_TEXT_MEDIUM,
        ])
        .size(tokens::typography::FONT_SIZE_100)
        .line_height(text::LineHeight::Absolute(iced::Pixels(20.0)))
        .style(spectrum_text_field_style)
}

pub fn search_field<'a, Message>(
    label: &'a str,
    value: &'a str,
    id: iced::widget::Id,
    on_input: impl Fn(String) -> Message + 'a,
    on_submit: Message,
    on_clear: Option<Message>,
    width: Length,
) -> Element<'a, Message>
where
    Message: Clone + 'a,
{
    let field = text_field_control(label, value, on_input, Some(on_submit), width)
        .id(id)
        .padding(Padding {
            top: tokens::spacing::FIELD_TOP_TO_TEXT_MEDIUM,
            right: tokens::spacing::SEARCH_FIELD_ICON_SLOT,
            bottom: tokens::spacing::FIELD_TOP_TO_TEXT_MEDIUM,
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

fn spectrum_switch_style(theme: &Theme, status: toggler::Status) -> toggler::Style {
    let colors = SpectrumColors::from_theme(theme);
    let (is_toggled, hovered, disabled) = match status {
        toggler::Status::Active { is_toggled } => (is_toggled, false, false),
        toggler::Status::Hovered { is_toggled } => (is_toggled, true, false),
        toggler::Status::Disabled { is_toggled } => (is_toggled, false, true),
    };
    let background = if disabled {
        colors.disabled_background
    } else if is_toggled {
        if hovered {
            colors.accent_background.hover
        } else {
            colors.accent_background.default
        }
    } else if hovered {
        colors.gray.gray_500
    } else {
        colors.gray.gray_400
    };
    let background_border_color = if disabled {
        colors.disabled_border
    } else if is_toggled {
        Color::TRANSPARENT
    } else if hovered {
        colors.gray.gray_700
    } else {
        colors.gray.gray_600
    };
    let foreground = if disabled {
        colors.disabled_content
    } else {
        Color::WHITE
    };
    let padding_ratio = (tokens::dimension::SWITCH_CONTROL_HEIGHT_MEDIUM
        - tokens::dimension::SWITCH_HANDLE_SIZE_MEDIUM)
        / (2.0 * tokens::dimension::SWITCH_CONTROL_HEIGHT_MEDIUM);

    toggler::Style {
        background: Background::Color(background),
        background_border_width: 1.0,
        background_border_color,
        foreground: Background::Color(foreground),
        foreground_border_width: 0.0,
        foreground_border_color: Color::TRANSPARENT,
        text_color: Some(if disabled {
            colors.disabled_content
        } else {
            colors.neutral_content.default
        }),
        border_radius: None,
        padding_ratio,
    }
}

fn search_icon_style(theme: &Theme, _status: svg::Status) -> svg::Style {
    svg::Style {
        color: Some(SpectrumColors::from_theme(theme).neutral_content.default),
    }
}

fn field_label_style(theme: &Theme) -> text::Style {
    text::Style {
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

    #[test]
    fn medium_switch_uses_a_ten_pixel_handle() {
        let padding = (tokens::dimension::SWITCH_CONTROL_HEIGHT_MEDIUM
            - tokens::dimension::SWITCH_HANDLE_SIZE_MEDIUM)
            / 2.0;

        assert_eq!(tokens::dimension::SWITCH_CONTROL_HEIGHT_MEDIUM, 16.0);
        assert_eq!(padding, 3.0);
    }
}

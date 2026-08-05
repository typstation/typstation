//! Menu e acionador de barra baseados no componente Menu do Adobe Spectrum 2.

use iced::{
    Alignment, Background, Border, Element, Font, Length, Shadow, Theme, Vector,
    font::Weight,
    mouse,
    widget::{Space, button, column, container, mouse_area, row, svg, text},
};

use super::{
    icons::UiIcon,
    tokens::{self, SpectrumColors},
};

#[derive(Debug, Clone)]
pub struct MenuItem<Message> {
    pub label: String,
    pub value: Option<String>,
    pub selected: bool,
    pub focused: bool,
    pub on_press: Option<Message>,
    pub on_focus: Option<Message>,
}

impl<Message> MenuItem<Message> {
    pub fn new(label: impl Into<String>, on_press: Option<Message>) -> Self {
        Self {
            label: label.into(),
            value: None,
            selected: false,
            focused: false,
            on_press,
            on_focus: None,
        }
    }

    pub fn value(mut self, value: impl Into<String>) -> Self {
        self.value = Some(value.into());
        self
    }

    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    pub fn focused(mut self, focused: bool) -> Self {
        self.focused = focused;
        self
    }

    pub fn on_focus(mut self, on_focus: Message) -> Self {
        self.on_focus = Some(on_focus);
        self
    }
}

#[derive(Debug, Clone)]
pub enum MenuEntry<Message> {
    Item(MenuItem<Message>),
    Divider,
}

pub struct Menu<Message> {
    entries: Vec<MenuEntry<Message>>,
    width: f32,
}

impl<Message> Menu<Message> {
    pub fn new(entries: Vec<MenuEntry<Message>>) -> Self {
        Self {
            entries,
            width: 264.0,
        }
    }

    pub fn width(mut self, width: f32) -> Self {
        self.width = width;
        self
    }
}

impl<'a, Message> From<Menu<Message>> for Element<'a, Message>
where
    Message: Clone + 'a,
{
    fn from(menu: Menu<Message>) -> Self {
        let mut entries = column![];

        for entry in menu.entries {
            entries = entries.push(match entry {
                MenuEntry::Item(item) => menu_item(item),
                MenuEntry::Divider => menu_divider(),
            });
        }

        container(entries)
            .width(Length::Fixed(menu.width))
            .padding(tokens::spacing::MENU_POPOVER_PADDING)
            .style(menu_container_style)
            .into()
    }
}

pub fn menu_bar_button<'a, Message>(
    label: &'a str,
    width: f32,
    on_activate: Message,
    on_pointer_press: Message,
    on_pointer_enter: Message,
    selected: bool,
) -> Element<'a, Message>
where
    Message: Clone + 'a,
{
    let label = container(
        text(label)
            .size(tokens::typography::FONT_SIZE_100)
            .line_height(tokens::typography::LINE_HEIGHT_100)
            .font(Font {
                weight: Weight::Medium,
                ..Font::DEFAULT
            })
            .height(Length::Fill)
            .align_x(Alignment::Center)
            .align_y(Alignment::Center),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .center_x(Length::Fill)
    .center_y(Length::Fill);
    let pointer = mouse_area(label)
        .on_press(on_pointer_press)
        .on_enter(on_pointer_enter)
        .interaction(mouse::Interaction::Pointer);

    button(pointer)
        .on_press(on_activate)
        .width(Length::Fixed(width))
        .height(Length::Fixed(tokens::dimension::MENU_ITEM_HEIGHT_MEDIUM))
        .padding(0)
        .style(move |theme, status| menu_bar_button_style(theme, status, selected))
        .into()
}

fn menu_item<'a, Message>(item: MenuItem<Message>) -> Element<'a, Message>
where
    Message: Clone + 'a,
{
    let MenuItem {
        label,
        value,
        selected,
        focused,
        on_press,
        on_focus,
    } = item;
    let enabled = on_press.is_some();
    let selection: Element<'a, Message> = if selected {
        svg(UiIcon::Checkmark100.handle())
            .width(Length::Fixed(tokens::icon::UI_CHECKMARK_100_SIZE))
            .height(Length::Fixed(tokens::icon::UI_CHECKMARK_100_SIZE))
            .style(move |theme, _status| checkmark_style(theme, enabled))
            .into()
    } else {
        Space::new()
            .width(Length::Fixed(tokens::icon::UI_CHECKMARK_100_SIZE))
            .height(Length::Fixed(tokens::icon::UI_CHECKMARK_100_SIZE))
            .into()
    };
    let label = text(label)
        .size(tokens::typography::FONT_SIZE_100)
        .line_height(tokens::typography::LINE_HEIGHT_100)
        .font(Font {
            weight: Weight::Normal,
            ..Font::DEFAULT
        })
        .width(Length::Fill)
        .height(Length::Fill)
        .align_y(Alignment::Center)
        .wrapping(text::Wrapping::None);
    let value: Element<'a, Message> = value.map_or_else(
        || Space::new().height(Length::Fill).into(),
        |value| {
            text(value)
                .size(tokens::typography::FONT_SIZE_75)
                .line_height(tokens::typography::LINE_HEIGHT_100)
                .height(Length::Fill)
                .align_y(Alignment::Center)
                .wrapping(text::Wrapping::None)
                .style(move |theme: &Theme| value_style(theme, enabled, focused))
                .into()
        },
    );
    let label_area = row![selection, label]
        .align_y(Alignment::Center)
        .spacing(tokens::spacing::MENU_CHECKMARK_TO_TEXT)
        .width(Length::Fill)
        .height(Length::Fill);
    let content = row![label_area, value]
        .align_y(Alignment::Center)
        .spacing(tokens::spacing::MENU_TEXT_TO_VALUE)
        .width(Length::Fill)
        .height(Length::Fill);

    let control = button(content)
        .on_press_maybe(on_press)
        .width(Length::Fill)
        .height(Length::Fixed(tokens::dimension::MENU_ITEM_HEIGHT_MEDIUM))
        .padding([0.0, tokens::spacing::MENU_EDGE_TO_CONTENT_MEDIUM])
        .style(move |theme, status| menu_item_style(theme, status, focused));

    match on_focus {
        Some(on_focus) => mouse_area(control).on_enter(on_focus).into(),
        None => control.into(),
    }
}

fn menu_divider<'a, Message>() -> Element<'a, Message>
where
    Message: 'a,
{
    container(
        container(Space::new())
            .width(Length::Fill)
            .height(Length::Fixed(1.0))
            .style(menu_divider_style),
    )
    .width(Length::Fill)
    .height(Length::Fixed(
        tokens::dimension::MENU_SECTION_DIVIDER_HEIGHT,
    ))
    .padding([5.5, tokens::spacing::MENU_EDGE_TO_CONTENT_MEDIUM])
    .into()
}

fn menu_bar_button_style(theme: &Theme, status: button::Status, selected: bool) -> button::Style {
    let colors = SpectrumColors::from_theme(theme);
    let background = match status {
        button::Status::Pressed => Some(colors.gray.gray_200),
        button::Status::Hovered => Some(colors.gray.gray_100),
        button::Status::Active if selected => Some(colors.gray.gray_100),
        button::Status::Active | button::Status::Disabled => None,
    };

    button::Style {
        background: background.map(Background::Color),
        text_color: if status == button::Status::Disabled {
            colors.disabled_content
        } else {
            colors.gray.gray_800
        },
        border: Border::default().rounded(tokens::dimension::MENU_ITEM_CORNER_RADIUS),
        ..button::Style::default()
    }
}

fn menu_item_style(theme: &Theme, status: button::Status, focused: bool) -> button::Style {
    let colors = SpectrumColors::from_theme(theme);
    let background = match status {
        button::Status::Disabled => None,
        button::Status::Pressed => Some(colors.gray.gray_200),
        button::Status::Hovered => Some(colors.gray.gray_100),
        button::Status::Active if focused => Some(colors.gray.gray_100),
        button::Status::Active => None,
    };

    button::Style {
        background: background.map(Background::Color),
        text_color: if status == button::Status::Disabled {
            colors.disabled_content
        } else {
            colors.gray.gray_800
        },
        border: Border::default().rounded(tokens::dimension::MENU_ITEM_CORNER_RADIUS),
        ..button::Style::default()
    }
}

fn menu_container_style(theme: &Theme) -> container::Style {
    let colors = SpectrumColors::from_theme(theme);
    let shadow_opacity = if tokens::ColorScheme::from_theme(theme) == tokens::ColorScheme::Dark {
        0.48
    } else {
        0.16
    };

    container::Style {
        background: Some(Background::Color(colors.gray.gray_50)),
        border: Border {
            color: colors.gray.gray_300,
            width: 1.0,
            radius: tokens::dimension::MENU_POPOVER_CORNER_RADIUS.into(),
        },
        shadow: Shadow {
            color: iced::Color::from_rgba(0.0, 0.0, 0.0, shadow_opacity),
            offset: Vector::new(0.0, 2.0),
            blur_radius: 8.0,
        },
        ..container::Style::default()
    }
}

fn menu_divider_style(theme: &Theme) -> container::Style {
    container::Style::default().background(SpectrumColors::from_theme(theme).gray.gray_300)
}

fn checkmark_style(theme: &Theme, enabled: bool) -> svg::Style {
    let colors = SpectrumColors::from_theme(theme);

    svg::Style {
        color: Some(if enabled {
            colors.gray.gray_800
        } else {
            colors.disabled_content
        }),
    }
}

fn value_style(theme: &Theme, enabled: bool, focused: bool) -> text::Style {
    let colors = SpectrumColors::from_theme(theme);

    text::Style {
        color: Some(if !enabled {
            colors.disabled_content
        } else if focused {
            colors.gray.gray_800
        } else {
            colors.gray.gray_600
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_item_builders_preserve_semantic_state() {
        let item = MenuItem::new("Mostrar números de linha", Some(1))
            .value("Ctrl+L")
            .selected(true)
            .focused(true);

        assert_eq!(item.label, "Mostrar números de linha");
        assert_eq!(item.value.as_deref(), Some("Ctrl+L"));
        assert!(item.selected);
        assert!(item.focused);
        assert_eq!(item.on_press, Some(1));
        assert_eq!(item.on_focus, None);
    }
}

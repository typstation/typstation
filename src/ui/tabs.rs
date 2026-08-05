//! Abas horizontais baseadas no componente Tabs do Adobe Spectrum 2.

use std::time::Duration;

use iced::{
    Alignment, Background, Border, Element, Length, Padding, Theme,
    widget::{Space, Stack, button, column, container, row, scrollable, svg, text, tooltip},
};

use super::{
    icons::UiIcon,
    tokens::{self, SpectrumColors},
};

#[derive(Debug, Clone)]
pub struct TabItem<Message> {
    pub label: String,
    pub selected: bool,
    pub on_select: Option<Message>,
    pub on_close: Option<Message>,
}

impl<Message> TabItem<Message> {
    pub fn new(
        label: impl Into<String>,
        on_select: Option<Message>,
        on_close: Option<Message>,
    ) -> Self {
        Self {
            label: label.into(),
            selected: false,
            on_select,
            on_close,
        }
    }

    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }
}

pub struct Tabs<'a, Message> {
    items: Vec<TabItem<Message>>,
    panel: Element<'a, Message>,
}

impl<'a, Message> Tabs<'a, Message>
where
    Message: Clone + 'a,
{
    pub fn new(items: Vec<TabItem<Message>>, panel: impl Into<Element<'a, Message>>) -> Self {
        Self {
            items,
            panel: panel.into(),
        }
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    fn build(self) -> Element<'a, Message> {
        let mut tab_items = row![]
            .align_y(Alignment::Center)
            .spacing(tokens::spacing::TAB_GAP_HORIZONTAL_MEDIUM)
            .padding(Padding {
                top: 0.0,
                right: tokens::spacing::TAB_START_TO_EDGE_MEDIUM,
                bottom: 0.0,
                left: tokens::spacing::TAB_START_TO_EDGE_MEDIUM,
            });

        for item in self.items {
            tab_items = tab_items.push(tab_item(item));
        }

        let divider = container(Space::new())
            .width(Length::Fill)
            .height(Length::Fixed(
                tokens::dimension::TAB_SELECTION_INDICATOR_HEIGHT,
            ))
            .style(divider_style);
        let divider_layer = column![Space::new().height(Length::Fill), divider]
            .width(Length::Fill)
            .height(Length::Fill);
        let tab_list = scrollable(tab_items)
            .direction(scrollable::Direction::Horizontal(
                scrollable::Scrollbar::hidden(),
            ))
            .width(Length::Fill)
            .height(Length::Fixed(
                tokens::dimension::TAB_ITEM_COMPACT_HEIGHT_MEDIUM,
            ));
        let tab_bar = Stack::new()
            .width(Length::Fill)
            .height(Length::Fixed(
                tokens::dimension::TAB_ITEM_COMPACT_HEIGHT_MEDIUM,
            ))
            .push(divider_layer)
            .push(tab_list);
        let panel = container(self.panel)
            .width(Length::Fill)
            .height(Length::Fill);

        column![tab_bar, panel]
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }
}

impl<'a, Message> From<Tabs<'a, Message>> for Element<'a, Message>
where
    Message: Clone + 'a,
{
    fn from(tabs: Tabs<'a, Message>) -> Self {
        tabs.build()
    }
}

fn tab_item<'a, Message>(item: TabItem<Message>) -> Element<'a, Message>
where
    Message: Clone + 'a,
{
    let TabItem {
        label,
        selected,
        on_select,
        on_close,
    } = item;
    let enabled = on_select.is_some();
    let close_enabled = on_close.is_some();
    let label = text(label)
        .size(tokens::typography::FONT_SIZE_100)
        .line_height(tokens::typography::LINE_HEIGHT_100)
        .wrapping(text::Wrapping::None);
    let close_icon = svg(UiIcon::Cross100.handle())
        .width(Length::Fixed(tokens::icon::UI_CROSS_100_SIZE))
        .height(Length::Fixed(tokens::icon::UI_CROSS_100_SIZE))
        .style(move |theme, status| close_icon_style(theme, status, close_enabled));
    let close_button = button(close_icon)
        .on_press_maybe(on_close)
        .width(Length::Fixed(tokens::dimension::TAB_CLOSE_BUTTON_SIZE))
        .height(Length::Fixed(tokens::dimension::TAB_CLOSE_BUTTON_SIZE))
        .padding((tokens::dimension::TAB_CLOSE_BUTTON_SIZE - tokens::icon::UI_CROSS_100_SIZE) / 2.0)
        .style(close_button_style);
    let close: Element<'a, Message> = tooltip(
        close_button,
        text("Fechar aba").size(tokens::typography::FONT_SIZE_75),
        tooltip::Position::Top,
    )
    .gap(tokens::spacing::BASE_GAP_SMALL)
    .padding(8)
    .delay(Duration::from_millis(500))
    .style(tooltip_style)
    .into();
    let content = row![label, close]
        .align_y(Alignment::Center)
        .spacing(tokens::spacing::TAB_ITEM_CONTENT_GAP);
    let control = button(content)
        .on_press_maybe(on_select)
        .height(Length::Fixed(
            tokens::dimension::TAB_ITEM_COMPACT_HEIGHT_MEDIUM,
        ))
        .padding(0)
        .style(move |theme, status| tab_button_style(theme, status, selected));
    let indicator = container(Space::new())
        .width(Length::Fill)
        .height(Length::Fixed(
            tokens::dimension::TAB_SELECTION_INDICATOR_HEIGHT,
        ))
        .style(move |theme| indicator_style(theme, selected, enabled));
    let indicator_layer = column![Space::new().height(Length::Fill), indicator]
        .width(Length::Fill)
        .height(Length::Fill);

    Stack::new()
        .height(Length::Fixed(
            tokens::dimension::TAB_ITEM_COMPACT_HEIGHT_MEDIUM,
        ))
        .push(control)
        .push(indicator_layer)
        .into()
}

fn tab_button_style(theme: &Theme, status: button::Status, selected: bool) -> button::Style {
    let colors = SpectrumColors::from_theme(theme);
    let text_color = match status {
        button::Status::Disabled => colors.disabled_content,
        button::Status::Hovered | button::Status::Pressed => colors.gray.gray_800,
        button::Status::Active if selected => colors.gray.gray_800,
        button::Status::Active => colors.gray.gray_700,
    };

    button::Style {
        background: None,
        text_color,
        border: Border::default(),
        ..button::Style::default()
    }
}

fn close_button_style(theme: &Theme, status: button::Status) -> button::Style {
    let colors = SpectrumColors::from_theme(theme);
    let background = match status {
        button::Status::Hovered => Some(Background::Color(colors.gray.gray_100)),
        button::Status::Pressed => Some(Background::Color(colors.gray.gray_200)),
        button::Status::Active | button::Status::Disabled => None,
    };

    button::Style {
        background,
        border: Border {
            radius: tokens::dimension::CORNER_RADIUS_100.into(),
            ..Border::default()
        },
        ..button::Style::default()
    }
}

fn close_icon_style(theme: &Theme, status: svg::Status, enabled: bool) -> svg::Style {
    let colors = SpectrumColors::from_theme(theme);
    let color = if !enabled {
        colors.disabled_content
    } else {
        match status {
            svg::Status::Idle => colors.gray.gray_700,
            svg::Status::Hovered => colors.gray.gray_800,
        }
    };

    svg::Style { color: Some(color) }
}

fn divider_style(theme: &Theme) -> container::Style {
    let colors = SpectrumColors::from_theme(theme);

    container::Style::default().background(colors.gray.gray_300)
}

fn indicator_style(theme: &Theme, selected: bool, enabled: bool) -> container::Style {
    let colors = SpectrumColors::from_theme(theme);
    let color = if !selected {
        iced::Color::TRANSPARENT
    } else if enabled {
        colors.gray.gray_800
    } else {
        colors.disabled_border
    };

    container::Style::default().background(color)
}

fn tooltip_style(theme: &Theme) -> container::Style {
    let colors = SpectrumColors::from_theme(theme);

    container::Style::default()
        .background(colors.gray.gray_900)
        .color(colors.gray.gray_25)
        .border(Border {
            radius: tokens::dimension::CORNER_RADIUS_300.into(),
            ..Border::default()
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, PartialEq, Eq)]
    enum Message {
        Select,
        Close,
    }

    #[test]
    fn tab_item_keeps_selection_and_both_actions() {
        let item =
            TabItem::new("main.typ", Some(Message::Select), Some(Message::Close)).selected(true);

        assert_eq!(item.label, "main.typ");
        assert!(item.selected);
        assert_eq!(item.on_select, Some(Message::Select));
        assert_eq!(item.on_close, Some(Message::Close));
    }

    #[test]
    fn tabs_report_the_number_of_tab_items() {
        let tabs = Tabs::new(
            vec![TabItem::new("main.typ", Some(Message::Select), None)],
            Space::new(),
        );

        assert_eq!(tabs.len(), 1);
        assert!(!tabs.is_empty());
    }
}

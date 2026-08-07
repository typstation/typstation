//! Tabs verticais compactas para alternar os painéis laterais da aplicação.

use std::time::Duration;

use iced::{
    Alignment, Background, Border, Element, Length, Padding, Theme,
    widget::{Column, Space, Stack, button, container, row, svg, text, tooltip},
};

use super::{
    icons::WorkflowIcon,
    tokens::{self, SpectrumColors},
};

#[derive(Debug, Clone)]
pub struct PanelTabItem<Message> {
    pub label: String,
    pub icon: WorkflowIcon,
    pub selected: bool,
    pub notification: Option<PanelTabNotification>,
    pub on_select: Option<Message>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelTabNotification {
    Error,
    Warning,
}

impl<Message> PanelTabItem<Message> {
    pub fn new(label: impl Into<String>, icon: WorkflowIcon, on_select: Option<Message>) -> Self {
        Self {
            label: label.into(),
            icon,
            selected: false,
            notification: None,
            on_select,
        }
    }

    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    pub fn notification(mut self, notification: Option<PanelTabNotification>) -> Self {
        self.notification = notification;
        self
    }
}

pub struct PanelTabs<Message> {
    items: Vec<PanelTabItem<Message>>,
}

impl<Message> PanelTabs<Message> {
    pub fn new(items: Vec<PanelTabItem<Message>>) -> Self {
        Self { items }
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    fn build<'a>(self) -> Element<'a, Message>
    where
        Message: Clone + 'a,
    {
        let items = self.items.into_iter().fold(
            Column::new()
                .spacing(tokens::spacing::PANEL_TABS_ITEM_GAP)
                .width(Length::Fill),
            |items, item| items.push(panel_tab_item(item)),
        );
        let tabs = container(items)
            .width(Length::Fixed(tokens::dimension::PANEL_TABS_RAIL_WIDTH))
            .height(Length::Fill)
            .padding(Padding {
                top: tokens::spacing::PANEL_TABS_PADDING_VERTICAL,
                right: tokens::spacing::PANEL_TABS_PADDING_HORIZONTAL,
                bottom: tokens::spacing::PANEL_TABS_PADDING_VERTICAL,
                left: tokens::spacing::PANEL_TABS_PADDING_HORIZONTAL,
            })
            .style(panel_tabs_style);
        let divider = container(Space::new())
            .width(Length::Fixed(tokens::dimension::BORDER_WIDTH_100))
            .height(Length::Fill)
            .style(super::divider_style);
        let divider_layer = row![Space::new().width(Length::Fill), divider]
            .width(Length::Fill)
            .height(Length::Fill);

        Stack::new()
            .width(Length::Fixed(tokens::dimension::PANEL_TABS_RAIL_WIDTH))
            .height(Length::Fill)
            .push(tabs)
            .push(divider_layer)
            .into()
    }
}

impl<'a, Message> From<PanelTabs<Message>> for Element<'a, Message>
where
    Message: Clone + 'a,
{
    fn from(tabs: PanelTabs<Message>) -> Self {
        tabs.build()
    }
}

fn panel_tab_item<'a, Message>(item: PanelTabItem<Message>) -> Element<'a, Message>
where
    Message: Clone + 'a,
{
    let PanelTabItem {
        label,
        icon,
        selected,
        notification,
        on_select,
    } = item;
    let enabled = on_select.is_some();
    let icon = svg(icon.handle())
        .width(Length::Fixed(tokens::icon::WORKFLOW_SIZE_100))
        .height(Length::Fixed(tokens::icon::WORKFLOW_SIZE_100))
        .style(move |theme, status| panel_tab_icon_style(theme, status, enabled, selected));
    let control = button(
        container(icon)
            .center_x(Length::Fill)
            .center_y(Length::Fill),
    )
    .on_press_maybe(on_select)
    .width(Length::Fixed(tokens::dimension::PANEL_TAB_ITEM_SIZE))
    .height(Length::Fixed(tokens::dimension::PANEL_TAB_ITEM_SIZE))
    .padding(0)
    .style(move |theme, status| panel_tab_button_style(theme, status, selected));
    let indicator = container(Space::new())
        .width(Length::Fixed(
            tokens::dimension::PANEL_TAB_SELECTION_INDICATOR_WIDTH,
        ))
        .height(Length::Fill)
        .style(move |theme| panel_tab_indicator_style(theme, selected, enabled));
    let indicator_layer = row![indicator, Space::new().width(Length::Fill)]
        .width(Length::Fill)
        .height(Length::Fill);
    let notification_layer: Element<'a, Message> = match notification {
        Some(notification) => container(
            container(Space::new())
                .width(Length::Fixed(
                    tokens::dimension::PANEL_TAB_NOTIFICATION_SIZE,
                ))
                .height(Length::Fixed(
                    tokens::dimension::PANEL_TAB_NOTIFICATION_SIZE,
                ))
                .style(move |theme| panel_tab_notification_style(theme, notification)),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(tokens::spacing::PANEL_TAB_NOTIFICATION_OFFSET)
        .align_x(Alignment::End)
        .align_y(Alignment::Start)
        .into(),
        None => Space::new().into(),
    };
    let tab = Stack::new()
        .width(Length::Fixed(tokens::dimension::PANEL_TAB_ITEM_SIZE))
        .height(Length::Fixed(tokens::dimension::PANEL_TAB_ITEM_SIZE))
        .push(control)
        .push(indicator_layer)
        .push(notification_layer);

    tooltip(
        tab,
        text(label).size(tokens::typography::FONT_SIZE_75),
        tooltip::Position::Right,
    )
    .gap(tokens::spacing::SPACING_100)
    .padding(tokens::spacing::TOOLTIP_EDGE_TO_CONTENT)
    .delay(Duration::from_millis(500))
    .style(tooltip_style)
    .into()
}

fn panel_tabs_style(theme: &Theme) -> container::Style {
    container::Style::default().background(SpectrumColors::from_theme(theme).gray.gray_50)
}

fn panel_tab_button_style(theme: &Theme, status: button::Status, selected: bool) -> button::Style {
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
        } else if selected {
            colors.gray.gray_900
        } else {
            colors.gray.gray_700
        },
        border: Border {
            radius: tokens::dimension::CORNER_RADIUS_100.into(),
            ..Border::default()
        },
        ..button::Style::default()
    }
}

fn panel_tab_icon_style(
    theme: &Theme,
    status: svg::Status,
    enabled: bool,
    selected: bool,
) -> svg::Style {
    let colors = SpectrumColors::from_theme(theme);
    let color = if !enabled {
        colors.disabled_content
    } else {
        match status {
            svg::Status::Hovered => colors.gray.gray_900,
            svg::Status::Idle if selected => colors.gray.gray_900,
            svg::Status::Idle => colors.gray.gray_700,
        }
    };

    svg::Style { color: Some(color) }
}

fn panel_tab_indicator_style(theme: &Theme, selected: bool, enabled: bool) -> container::Style {
    let colors = SpectrumColors::from_theme(theme);
    let color = if !selected {
        iced::Color::TRANSPARENT
    } else if enabled {
        colors.gray.gray_900
    } else {
        colors.disabled_border
    };

    container::Style::default().background(color)
}

fn panel_tab_notification_style(
    theme: &Theme,
    notification: PanelTabNotification,
) -> container::Style {
    let colors = SpectrumColors::from_theme(theme);
    let background = match notification {
        PanelTabNotification::Error => colors.negative_background.default,
        PanelTabNotification::Warning => colors.notice,
    };

    container::Style {
        background: Some(Background::Color(background)),
        border: Border {
            color: colors.gray.gray_50,
            width: tokens::dimension::BORDER_WIDTH_200,
            radius: (tokens::dimension::PANEL_TAB_NOTIFICATION_SIZE
                * tokens::dimension::CORNER_RADIUS_FULL_MULTIPLIER)
                .into(),
        },
        ..container::Style::default()
    }
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

    #[test]
    fn item_preserves_selection_action_and_notification() {
        let item = PanelTabItem::new("Problemas", WorkflowIcon::AlertCircleFilled, Some(7))
            .selected(true)
            .notification(Some(PanelTabNotification::Error));

        assert_eq!(item.label, "Problemas");
        assert!(item.selected);
        assert_eq!(item.on_select, Some(7));
        assert_eq!(item.notification, Some(PanelTabNotification::Error));
    }

    #[test]
    fn rail_and_items_use_stable_compact_dimensions() {
        assert_eq!(tokens::dimension::PANEL_TABS_RAIL_WIDTH, 48.0);
        assert_eq!(tokens::dimension::PANEL_TAB_ITEM_SIZE, 40.0);
        assert_eq!(
            tokens::dimension::PANEL_TAB_ITEM_SIZE
                + 2.0 * tokens::spacing::PANEL_TABS_PADDING_HORIZONTAL,
            tokens::dimension::PANEL_TABS_RAIL_WIDTH
        );
    }

    #[test]
    fn tabs_report_their_item_count() {
        let tabs = PanelTabs::new(vec![PanelTabItem::<()>::new(
            "Arquivos",
            WorkflowIcon::FolderOpen,
            None,
        )]);

        assert_eq!(tabs.len(), 1);
        assert!(!tabs.is_empty());
    }
}

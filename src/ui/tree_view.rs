//! Árvore hierárquica baseada no Tree View do Adobe Spectrum 2.

use std::time::Duration;

use iced::{
    Alignment, Background, Border, Color, Element, Font, Length, Padding, Theme,
    font::Weight,
    widget::{Column, Space, button, container, mouse_area, row, svg, text, tooltip},
};

use super::{
    icons::WorkflowIcon,
    tokens::{self, SpectrumColors},
};

#[derive(Debug, Clone)]
pub struct TreeViewItem<Message> {
    pub label: String,
    pub icon: Option<WorkflowIcon>,
    pub expanded: bool,
    pub selected: bool,
    pub status_icon: Option<WorkflowIcon>,
    pub status_label: Option<String>,
    pub on_press: Option<Message>,
    pub on_context_menu: Option<Message>,
    pub has_children: bool,
    pub children: Vec<TreeViewItem<Message>>,
}

impl<Message> TreeViewItem<Message> {
    pub fn new(
        label: impl Into<String>,
        icon: Option<WorkflowIcon>,
        on_press: Option<Message>,
    ) -> Self {
        Self {
            label: label.into(),
            icon,
            expanded: false,
            selected: false,
            status_icon: None,
            status_label: None,
            on_press,
            on_context_menu: None,
            has_children: false,
            children: Vec::new(),
        }
    }

    pub fn expanded(mut self, expanded: bool) -> Self {
        self.expanded = expanded;
        self
    }

    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    pub fn status_icon(mut self, icon: WorkflowIcon, label: impl Into<String>) -> Self {
        self.status_icon = Some(icon);
        self.status_label = Some(label.into());
        self
    }

    pub fn on_context_menu(mut self, on_context_menu: Message) -> Self {
        self.on_context_menu = Some(on_context_menu);
        self
    }

    pub fn children(mut self, children: Vec<Self>) -> Self {
        self.has_children |= !children.is_empty();
        self.children = children;
        self
    }

    pub fn has_children(mut self, has_children: bool) -> Self {
        self.has_children = has_children;
        self
    }
}

#[derive(Debug, Clone)]
pub struct TreeView<Message> {
    items: Vec<TreeViewItem<Message>>,
    emphasized: bool,
    reserve_icon_space: bool,
}

impl<Message> TreeView<Message> {
    pub fn new(items: Vec<TreeViewItem<Message>>) -> Self {
        Self {
            items,
            emphasized: false,
            reserve_icon_space: true,
        }
    }

    pub fn emphasized(mut self, emphasized: bool) -> Self {
        self.emphasized = emphasized;
        self
    }

    pub fn reserve_icon_space(mut self, reserve: bool) -> Self {
        self.reserve_icon_space = reserve;
        self
    }

    pub fn visible_len(&self) -> usize {
        visible_item_count(&self.items)
    }

    fn build<'a>(self) -> Element<'a, Message>
    where
        Message: Clone + 'a,
    {
        let rows = append_items(
            Column::new().width(Length::Fill),
            self.items,
            0,
            self.emphasized,
            self.reserve_icon_space,
        );

        container(rows)
            .width(Length::Fill)
            .height(Length::Shrink)
            .into()
    }
}

impl<'a, Message> From<TreeView<Message>> for Element<'a, Message>
where
    Message: Clone + 'a,
{
    fn from(tree: TreeView<Message>) -> Self {
        tree.build()
    }
}

fn append_items<'a, Message>(
    mut rows: Column<'a, Message>,
    items: Vec<TreeViewItem<Message>>,
    depth: usize,
    emphasized: bool,
    reserve_icon_space: bool,
) -> Column<'a, Message>
where
    Message: Clone + 'a,
{
    for mut item in items {
        let expanded = item.expanded;
        let has_children = item.has_children;
        let children = std::mem::take(&mut item.children);
        rows = rows.push(tree_row(
            item,
            depth,
            has_children,
            emphasized,
            reserve_icon_space,
        ));

        if expanded {
            rows = append_items(rows, children, depth + 1, emphasized, reserve_icon_space);
        }
    }

    rows
}

fn tree_row<'a, Message>(
    item: TreeViewItem<Message>,
    depth: usize,
    has_children: bool,
    emphasized: bool,
    reserve_icon_space: bool,
) -> Element<'a, Message>
where
    Message: Clone + 'a,
{
    let TreeViewItem {
        label,
        icon,
        expanded,
        selected,
        status_icon,
        status_label,
        on_press,
        on_context_menu,
        ..
    } = item;
    let enabled = on_press.is_some();
    let full_label =
        status_label.map_or_else(|| label.clone(), |status| format!("{label} - {status}"));
    let disclosure_icon = if expanded {
        WorkflowIcon::ChevronDown
    } else {
        WorkflowIcon::ChevronRight
    };
    let disclosure: Element<'a, Message> = if has_children {
        centered_icon(
            disclosure_icon,
            tokens::icon::TREE_VIEW_DISCLOSURE_SIZE,
            tokens::dimension::TREE_VIEW_DISCLOSURE_WIDTH,
            tokens::dimension::TREE_VIEW_DISCLOSURE_HEIGHT,
            enabled,
        )
    } else {
        Space::new()
            .width(Length::Fixed(tokens::dimension::TREE_VIEW_DISCLOSURE_WIDTH))
            .height(Length::Fixed(
                tokens::dimension::TREE_VIEW_DISCLOSURE_HEIGHT,
            ))
            .into()
    };
    let leading: Element<'a, Message> = if let Some(icon) = icon {
        centered_icon(
            icon,
            tokens::icon::TREE_VIEW_WORKFLOW_SIZE,
            tokens::icon::TREE_VIEW_WORKFLOW_SIZE,
            tokens::dimension::TREE_VIEW_DISCLOSURE_HEIGHT,
            enabled,
        )
    } else {
        Space::new()
            .width(Length::Fixed(if reserve_icon_space {
                tokens::icon::TREE_VIEW_WORKFLOW_SIZE
            } else {
                0.0
            }))
            .height(Length::Fixed(
                tokens::dimension::TREE_VIEW_DISCLOSURE_HEIGHT,
            ))
            .into()
    };
    let label = text(label)
        .size(tokens::typography::FONT_SIZE_100)
        .font(Font {
            weight: Weight::Medium,
            ..Font::DEFAULT
        })
        .width(Length::Fill)
        .wrapping(text::Wrapping::None)
        .align_y(Alignment::Center);
    let mut content_area = row![leading, label]
        .align_y(Alignment::Center)
        .spacing(tokens::spacing::TREE_VIEW_ITEM_GAP)
        .width(Length::Fill);

    if let Some(status_icon) = status_icon {
        content_area = content_area.push(tree_status_icon(status_icon));
    }

    let content = row![disclosure, content_area]
        .align_y(Alignment::Center)
        .width(Length::Fill);
    let left_padding = tokens::spacing::TREE_VIEW_EDGE_TO_CONTENT
        + depth as f32 * tokens::spacing::TREE_VIEW_LEVEL_INCREMENT;
    let control = button(content)
        .on_press_maybe(on_press)
        .width(Length::Fill)
        .height(Length::Fixed(tokens::dimension::TREE_VIEW_MINIMUM_HEIGHT))
        .padding(Padding {
            top: tokens::spacing::TREE_VIEW_TOP_TO_DISCLOSURE,
            right: tokens::spacing::TREE_VIEW_EDGE_TO_CONTENT,
            bottom: tokens::spacing::TREE_VIEW_TOP_TO_DISCLOSURE,
            left: left_padding,
        })
        .clip(true)
        .style(move |theme, status| tree_row_style(theme, status, selected, emphasized));
    let control: Element<'a, Message> = match on_context_menu {
        Some(message) => mouse_area(control).on_right_press(message).into(),
        None => control.into(),
    };

    tooltip(
        control,
        text(full_label).size(tokens::typography::FONT_SIZE_75),
        tooltip::Position::Right,
    )
    .gap(tokens::spacing::BASE_GAP_SMALL)
    .padding(8)
    .delay(Duration::from_millis(700))
    .style(tooltip_style)
    .into()
}

fn tree_status_icon<'a, Message>(icon: WorkflowIcon) -> Element<'a, Message>
where
    Message: 'a,
{
    let icon = svg(icon.handle())
        .width(Length::Fixed(tokens::icon::TREE_VIEW_WORKFLOW_SIZE))
        .height(Length::Fixed(tokens::icon::TREE_VIEW_WORKFLOW_SIZE))
        .style(|theme, _status| svg::Style {
            color: Some(SpectrumColors::from_theme(theme).accent_background.default),
        });

    container(icon)
        .width(Length::Fixed(
            tokens::dimension::TREE_VIEW_STATUS_ICON_SLOT_WIDTH,
        ))
        .height(Length::Fixed(
            tokens::dimension::TREE_VIEW_DISCLOSURE_HEIGHT,
        ))
        .center_x(Length::Fixed(
            tokens::dimension::TREE_VIEW_STATUS_ICON_SLOT_WIDTH,
        ))
        .center_y(Length::Fixed(
            tokens::dimension::TREE_VIEW_DISCLOSURE_HEIGHT,
        ))
        .into()
}

fn centered_icon<'a, Message>(
    icon: WorkflowIcon,
    icon_size: f32,
    width: f32,
    height: f32,
    enabled: bool,
) -> Element<'a, Message>
where
    Message: 'a,
{
    let icon = svg(icon.handle())
        .width(Length::Fixed(icon_size))
        .height(Length::Fixed(icon_size))
        .style(move |theme, _status| svg::Style {
            color: Some(if enabled {
                SpectrumColors::from_theme(theme).neutral_content.default
            } else {
                SpectrumColors::from_theme(theme).disabled_content
            }),
        });

    container(icon)
        .width(Length::Fixed(width))
        .height(Length::Fixed(height))
        .center_x(Length::Fixed(width))
        .center_y(Length::Fixed(height))
        .into()
}

fn tree_row_style(
    theme: &Theme,
    status: button::Status,
    selected: bool,
    emphasized: bool,
) -> button::Style {
    let colors = SpectrumColors::from_theme(theme);
    let background = if selected && emphasized {
        let opacity = if status == button::Status::Hovered {
            0.15
        } else {
            0.10
        };
        Some(with_alpha(colors.accent_background.default, opacity))
    } else if selected || status == button::Status::Hovered {
        Some(colors.gray.gray_100)
    } else if status == button::Status::Pressed {
        Some(colors.gray.gray_200)
    } else {
        None
    };

    button::Style {
        background: background.map(Background::Color),
        text_color: if status == button::Status::Disabled {
            colors.disabled_content
        } else {
            colors.neutral_content.default
        },
        border: Border::default(),
        ..button::Style::default()
    }
}

fn tooltip_style(theme: &Theme) -> container::Style {
    let colors = SpectrumColors::from_theme(theme);

    container::Style::default()
        .background(colors.gray.gray_900)
        .color(colors.gray.gray_25)
        .border(Border::default().rounded(tokens::dimension::CORNER_RADIUS_300))
}

fn with_alpha(color: Color, alpha: f32) -> Color {
    Color { a: alpha, ..color }
}

fn visible_item_count<Message>(items: &[TreeViewItem<Message>]) -> usize {
    items
        .iter()
        .map(|item| {
            1 + if item.expanded {
                visible_item_count(&item.children)
            } else {
                0
            }
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collapsed_branches_can_omit_their_descendants() {
        let branch =
            TreeViewItem::<()>::new("src", Some(WorkflowIcon::Folder), None).has_children(true);

        assert!(branch.has_children);
        assert_eq!(TreeView::new(vec![branch]).visible_len(), 1);
    }

    #[test]
    fn expanded_branches_include_nested_descendants() {
        let branch = TreeViewItem::<()>::new("src", Some(WorkflowIcon::FolderOpen), None)
            .expanded(true)
            .children(vec![
                TreeViewItem::new("main.typ", Some(WorkflowIcon::FileCode), None),
                TreeViewItem::new("notes.md", Some(WorkflowIcon::Document), None),
            ]);

        assert_eq!(TreeView::new(vec![branch]).visible_len(), 3);
    }

    #[test]
    fn status_and_context_menu_are_explicit_item_semantics() {
        let item = TreeViewItem::new("main.typ", Some(WorkflowIcon::FileCode), Some(1))
            .status_icon(WorkflowIcon::Preview, "Exibido no Preview")
            .on_context_menu(2);

        assert_eq!(item.status_icon, Some(WorkflowIcon::Preview));
        assert_eq!(item.status_label.as_deref(), Some("Exibido no Preview"));
        assert_eq!(item.on_context_menu, Some(2));
    }

    #[test]
    fn icon_space_can_be_removed_for_text_only_trees() {
        let tree = TreeView::<()>::new(Vec::new()).reserve_icon_space(false);

        assert!(!tree.reserve_icon_space);
    }
}

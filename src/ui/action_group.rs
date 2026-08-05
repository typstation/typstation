//! Grupos compactos de Action Buttons relacionados.

use iced::{Element, widget::Row};

use super::{
    ActionButtonOptions, WorkflowIcon,
    button::{
        ActionButtonPosition, grouped_icon_action_button, grouped_workflow_icon_action_button,
    },
    tokens,
};

enum ActionGroupIcon<'a> {
    Symbol(&'a str),
    Workflow(WorkflowIcon),
}

pub struct ActionGroupItem<'a, Message> {
    icon: ActionGroupIcon<'a>,
    label: &'a str,
    on_press: Option<Message>,
}

impl<'a, Message> ActionGroupItem<'a, Message> {
    pub fn symbol(symbol: &'a str, label: &'a str, on_press: Option<Message>) -> Self {
        Self {
            icon: ActionGroupIcon::Symbol(symbol),
            label,
            on_press,
        }
    }

    pub fn workflow(icon: WorkflowIcon, label: &'a str, on_press: Option<Message>) -> Self {
        Self {
            icon: ActionGroupIcon::Workflow(icon),
            label,
            on_press,
        }
    }
}

pub fn compact_action_group<'a, Message, const N: usize>(
    items: [ActionGroupItem<'a, Message>; N],
    options: ActionButtonOptions,
) -> Element<'a, Message>
where
    Message: Clone + 'a,
{
    let mut group = Row::new().spacing(tokens::spacing::ACTION_GROUP_COMPACT_SPACING);

    for (index, item) in items.into_iter().enumerate() {
        let position = ActionButtonPosition::in_group(index, N);
        let control = match item.icon {
            ActionGroupIcon::Symbol(symbol) => {
                grouped_icon_action_button(symbol, item.label, item.on_press, options, position)
            }
            ActionGroupIcon::Workflow(icon) => grouped_workflow_icon_action_button(
                icon,
                item.label,
                item.on_press,
                options,
                position,
            ),
        };

        group = group.push(control);
    }

    group.into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_group_assigns_joined_edge_positions() {
        assert_eq!(
            ActionButtonPosition::in_group(0, 3),
            ActionButtonPosition::First
        );
        assert_eq!(
            ActionButtonPosition::in_group(1, 3),
            ActionButtonPosition::Middle
        );
        assert_eq!(
            ActionButtonPosition::in_group(2, 3),
            ActionButtonPosition::Last
        );
        assert_eq!(
            ActionButtonPosition::in_group(0, 1),
            ActionButtonPosition::Standalone
        );
    }
}

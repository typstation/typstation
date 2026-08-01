use std::ops::Range;

use typst_iced_editor::{Action, Bias, Content};

pub fn toggle_surround(content: &mut Content, open: &str, close: &str) -> bool {
    let range = content.selection();

    if range.is_empty() {
        let replacement = format!("{open}{close}");
        let caret = range.start + open.len();

        content.perform(Action::Replace {
            range,
            text: replacement,
        });
        content.perform(Action::MoveTo(caret));
        return true;
    }

    enum Change {
        RemoveOutside,
        Replace { text: String, select: Range<usize> },
    }

    let change = {
        let buffer = content.buffer();
        let text = buffer.text();
        let selected = &text[range.clone()];

        let outside_open = range.start.checked_sub(open.len()).and_then(|start| {
            text.get(start..range.start)
                .filter(|candidate| *candidate == open)
                .map(|_| start)
        });
        let outside_close = text
            .get(range.end..range.end.saturating_add(close.len()))
            .filter(|candidate| *candidate == close);

        if outside_open.is_some() && outside_close.is_some() {
            Change::RemoveOutside
        } else if let Some(inner) = selected
            .strip_prefix(open)
            .and_then(|value| value.strip_suffix(close))
        {
            Change::Replace {
                text: inner.to_owned(),
                select: range.start..range.start + inner.len(),
            }
        } else {
            Change::Replace {
                text: format!("{open}{selected}{close}"),
                select: range.start + open.len()..range.end + open.len(),
            }
        }
    };

    match change {
        Change::RemoveOutside => {
            let open_start = range.start - open.len();
            let selection = open_start..open_start + range.len();

            content.perform(Action::ApplyEdits(vec![
                (open_start..range.start, String::new()),
                (range.end..range.end + close.len(), String::new()),
            ]));
            select(content, selection);
        }
        Change::Replace {
            text,
            select: range_after,
        } => {
            content.perform(Action::Replace { range, text });
            select(content, range_after);
        }
    }

    true
}

pub fn toggle_line_prefix(content: &mut Content, prefix: &str) -> bool {
    if prefix.is_empty() {
        return false;
    }

    let selection = content.selection();

    let edits = {
        let buffer = content.buffer();
        let first_line = buffer.byte_to_line(selection.start);
        let last_line = if selection.is_empty() {
            first_line
        } else {
            buffer.byte_to_line(selection.end.saturating_sub(1))
        };

        let lines = (first_line..=last_line)
            .filter_map(|line| {
                let line_range = buffer.line_content_range(line);
                let line_text = &buffer.text()[line_range.clone()];

                if line_text.trim().is_empty() {
                    return None;
                }

                let indentation = line_text
                    .char_indices()
                    .find(|(_, character)| !character.is_whitespace())
                    .map_or(line_text.len(), |(offset, _)| offset);
                let prefix_at = line_range.start + indentation;
                let has_prefix = line_text[indentation..].starts_with(prefix);

                Some((prefix_at, has_prefix))
            })
            .collect::<Vec<_>>();

        let remove = !lines.is_empty() && lines.iter().all(|(_, has_prefix)| *has_prefix);

        lines
            .into_iter()
            .filter_map(|(at, has_prefix)| {
                if remove {
                    Some((at..at + prefix.len(), String::new()))
                } else if has_prefix {
                    None
                } else {
                    Some((at..at, prefix.to_owned()))
                }
            })
            .collect::<Vec<_>>()
    };

    if edits.is_empty() {
        return false;
    }

    if selection.is_empty() {
        let caret = content.create_anchor(selection.start, Bias::After);
        content.perform(Action::ApplyEdits(edits));

        if let Some(position) = content.remove_anchor(caret) {
            content.perform(Action::MoveTo(position));
        }
    } else {
        let start = content.create_anchor(selection.start, Bias::After);
        let end = content.create_anchor(selection.end, Bias::Before);
        content.perform(Action::ApplyEdits(edits));

        if let (Some(start), Some(end)) = (content.remove_anchor(start), content.remove_anchor(end))
        {
            select(content, start..end);
        }
    }

    true
}

fn select(content: &mut Content, range: Range<usize>) {
    content.perform(Action::MoveTo(range.start));
    content.perform(Action::SelectTo(range.end));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn surround_wraps_and_unwraps_the_same_selection() {
        let mut content = Content::with_text("texto");
        content.perform(Action::SelectAll);

        assert!(toggle_surround(&mut content, "*", "*"));
        assert_eq!(content.text(), "*texto*");
        assert_eq!(content.selection_text().as_deref(), Some("texto"));

        assert!(toggle_surround(&mut content, "*", "*"));
        assert_eq!(content.text(), "texto");
        assert_eq!(content.selection_text().as_deref(), Some("texto"));
    }

    #[test]
    fn surround_inserts_an_empty_pair_at_the_caret() {
        let mut content = Content::with_text("abc");
        content.perform(Action::MoveTo(1));

        assert!(toggle_surround(&mut content, "#underline[", "]"));
        assert_eq!(content.text(), "a#underline[]bc");
        assert_eq!(content.selection(), 12..12);
    }

    #[test]
    fn line_prefix_uses_every_partially_selected_line_and_toggles() {
        let mut content = Content::with_text("alpha\n  beta\ngamma");
        content.perform(Action::MoveTo(2));
        content.perform(Action::SelectTo(15));

        assert!(toggle_line_prefix(&mut content, "- "));
        assert_eq!(content.text(), "- alpha\n  - beta\n- gamma");

        assert!(toggle_line_prefix(&mut content, "- "));
        assert_eq!(content.text(), "alpha\n  beta\ngamma");
    }

    #[test]
    fn line_prefix_skips_blank_lines() {
        let mut content = Content::with_text("one\n\nthree");
        content.perform(Action::SelectAll);

        assert!(toggle_line_prefix(&mut content, "+ "));
        assert_eq!(content.text(), "+ one\n\n+ three");
    }
}

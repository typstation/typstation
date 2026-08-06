//! Key bindings of the editor.
//!
//! Every key press is turned into a [`Binding`] before it takes effect. The
//! default mapping lives in [`Binding::from_key_press`]; applications can
//! replace or extend it with
//! [`CodeEditor::key_binding`](crate::CodeEditor::key_binding):
//!
//! ```no_run
//! # use typst_iced_editor::{code_editor, Action, Binding, Content, KeyPress};
//! # use iced_core::keyboard::Key;
//! # #[derive(Clone)] enum Message { Edit(Action), Save }
//! # let content = Content::new();
//! let editor: iced::Element<'_, Message> = code_editor(&content)
//!     .on_action(Message::Edit)
//!     .key_binding(|press| {
//!         match press.key.as_ref() {
//!             // Ctrl+S produces a custom application message.
//!             Key::Character("s") if press.modifiers.command() => {
//!                 Some(Binding::Custom(Message::Save))
//!             }
//!             // Everything else keeps the default behavior.
//!             _ => Binding::from_key_press(press),
//!         }
//!     })
//!     .into();
//! ```

use iced_core::keyboard::{self, key};
use iced_core::SmolStr;

use crate::action::Action;
use crate::cursor::Motion;
use crate::style::Status;

/// A key binding function for a [`CodeEditor`](crate::CodeEditor).
pub type KeyBindingFn<'a, Message> = Box<dyn Fn(KeyPress) -> Option<Binding<Message>> + 'a>;

/// A key press, as delivered to a
/// [`key_binding`](crate::CodeEditor::key_binding) function.
#[derive(Debug, Clone, PartialEq)]
pub struct KeyPress {
    /// The key pressed, without modifiers applied.
    ///
    /// Use this for combinations (e.g. Ctrl+S).
    pub key: keyboard::Key,
    /// The key pressed with modifiers applied.
    ///
    /// Use this for single-key bindings.
    pub modified_key: keyboard::Key,
    /// The physical key pressed, for layout-independent bindings.
    pub physical_key: key::Physical,
    /// The state of the keyboard modifiers.
    pub modifiers: keyboard::Modifiers,
    /// The text produced by the key press, if any.
    pub text: Option<SmolStr>,
    /// The current [`Status`] of the editor.
    pub status: Status,
}

/// What a key press does in a [`CodeEditor`](crate::CodeEditor).
#[derive(Debug, Clone, PartialEq)]
pub enum Binding<Message> {
    /// Perform an editor [`Action`].
    Action(Action),
    /// Copy the selection to the clipboard.
    Copy,
    /// Cut the selection to the clipboard.
    Cut,
    /// Paste the clipboard contents.
    Paste,
    /// Unfocus the editor.
    Unfocus,
    /// Open the completion popup at the caret.
    Complete,
    /// Produce the given application message.
    Custom(Message),
    /// Perform a sequence of bindings.
    Sequence(Vec<Self>),
}

impl<Message> Binding<Message> {
    /// The default binding for the given key press.
    ///
    /// Custom [`key_binding`](crate::CodeEditor::key_binding) functions can
    /// call this as their fallback.
    pub fn from_key_press(press: KeyPress) -> Option<Self> {
        let KeyPress {
            key,
            modified_key,
            physical_key,
            modifiers,
            text,
            status,
        } = press;

        if !matches!(status, Status::Focused { .. }) {
            return None;
        }

        // Command shortcuts are matched on the latin equivalent of the key,
        // so they keep working on non-latin keyboard layouts.
        if modifiers.command() {
            if matches!(key.as_ref(), keyboard::Key::Named(key::Named::Space)) {
                return Some(Self::Complete);
            }

            match key.to_latin(physical_key) {
                Some('c') => return Some(Self::Copy),
                Some('x') => return Some(Self::Cut),
                Some('v') if !modifiers.alt() => return Some(Self::Paste),
                Some('a') => return Some(Self::Action(Action::SelectAll)),
                Some('/') => return Some(Self::Action(Action::ToggleLineComment)),
                Some('d') if modifiers.shift() => {
                    return Some(Self::Action(Action::DuplicateLine));
                }
                Some('k') if modifiers.shift() => return Some(Self::Action(Action::DeleteLine)),
                Some('j') => return Some(Self::Action(Action::JoinLines)),
                Some('z') if modifiers.shift() => {
                    return Some(Self::Action(Action::Redo));
                }
                Some('z') => return Some(Self::Action(Action::Undo)),
                Some('y') => return Some(Self::Action(Action::Redo)),
                _ => {}
            }
        }

        let action = |action| Some(Binding::Action(action));

        match modified_key.as_ref() {
            keyboard::Key::Named(key::Named::Enter) => action(Action::Enter),
            keyboard::Key::Named(key::Named::Backspace) => action(Action::Backspace),
            keyboard::Key::Named(key::Named::Delete)
                if text.is_none() || text.as_deref() == Some("\u{7f}") =>
            {
                action(Action::Delete)
            }
            keyboard::Key::Named(key::Named::Tab) if !modifiers.command() => {
                if modifiers.shift() {
                    action(Action::Unindent)
                } else {
                    action(Action::Indent)
                }
            }
            keyboard::Key::Named(key::Named::Escape) => Some(Self::Unfocus),
            _ => {
                if let Some(text) = text {
                    let text: String = text.chars().filter(|c| !c.is_control()).collect();

                    if text.is_empty() {
                        return None;
                    }

                    return action(Action::Insert(text));
                }

                let keyboard::Key::Named(named) = key.as_ref() else {
                    return None;
                };

                if modifiers.alt() && !modifiers.command() && !modifiers.shift() {
                    match named {
                        key::Named::ArrowUp => return action(Action::MoveLineUp),
                        key::Named::ArrowDown => return action(Action::MoveLineDown),
                        _ => {}
                    }
                }

                let motion = match named {
                    key::Named::ArrowLeft => Motion::Left,
                    key::Named::ArrowRight => Motion::Right,
                    key::Named::ArrowUp => Motion::Up,
                    key::Named::ArrowDown => Motion::Down,
                    key::Named::Home => Motion::Home,
                    key::Named::End => Motion::End,
                    key::Named::PageUp => Motion::PageUp,
                    key::Named::PageDown => Motion::PageDown,
                    _ => return None,
                };

                let motion = if modifiers.jump() {
                    motion.widen()
                } else {
                    motion
                };

                action(if modifiers.shift() {
                    Action::Select(motion)
                } else {
                    Action::Move(motion)
                })
            }
        }
    }
}

//! A code editor widget for [iced], focused on [Typst].
//!
//! The editor follows the same architecture as iced's built-in widgets: a
//! [`Content`] holds the document state in your application, and the
//! [`CodeEditor`] widget renders it and publishes [`Action`]s as messages.
//!
//! ```no_run
//! use typst_iced_editor::{code_editor, Action, Content};
//!
//! #[derive(Default)]
//! struct App {
//!     content: Content,
//! }
//!
//! #[derive(Debug, Clone)]
//! enum Message {
//!     Edit(Action),
//! }
//!
//! impl App {
//!     fn update(&mut self, message: Message) {
//!         match message {
//!             Message::Edit(action) => self.content.perform(action),
//!         }
//!     }
//!
//!     fn view(&self) -> iced::Element<'_, Message> {
//!         code_editor(&self.content).on_action(Message::Edit).into()
//!     }
//! }
//!
//! fn main() -> iced::Result {
//!     iced::run(App::update, App::view)
//! }
//! ```
//!
//! # Features
//!
//! - Incremental parsing and syntax highlighting via [`typst_syntax`], the
//!   parser used by the Typst compiler itself.
//! - Virtualized rendering: only visible lines are shaped and drawn.
//! - Line number gutter, current line highlight, selection and caret.
//! - Full keyboard and mouse editing: motions, word navigation,
//!   double/triple click, clipboard, undo/redo.
//! - Position conversions between byte offsets, line/column, and UTF-16
//!   (ready for LSP integration).
//! - Folding for headings and multi-line blocks, calls, collections,
//!   parameter lists, and imports.
//!
//! # Dependencies
//!
//! The widget itself depends only on `iced_core`, but the fold markers and
//! diagnostic squiggles are embedded SVG assets drawn through iced's SVG
//! pipeline, so the application must enable iced's `svg` feature:
//!
//! ```toml
//! iced = { version = "0.14", features = ["svg"] }
//! ```
//!
//! Without it, the build fails with a cryptic `E0277` error about
//! `iced_wgpu::Renderer` not implementing `iced_core::svg::Renderer` — the
//! message never mentions this crate or the feature. The buffer also exposes
//! [`typst_syntax`] types (e.g. [`Buffer::root`]), so an application that
//! itself depends on `typst-*` crates must resolve them to the same `0.15.x`
//! versions as this crate.
//!
//! [iced]: https://iced.rs
//! [Typst]: https://typst.app

pub mod complete;
pub mod fold;
pub mod highlight;

mod action;
mod anchor;
mod buffer;
mod content;
mod cursor;
mod diagnostic;
mod draw;
mod history;
mod keymap;
mod line_cache;
mod overlay;
mod pair;
mod scroll;
mod style;
mod widget;

pub use action::Action;
pub use anchor::{Anchor, Bias};
pub use buffer::{Buffer, Position};
pub use complete::{document_words, word_before, Completion, Hover};
pub use content::Content;
pub use cursor::{Motion, Selection};
pub use diagnostic::{Diagnostic, Severity};
pub use fold::Fold;
pub use highlight::{SyntaxStyle, SyntaxTheme};
pub use keymap::{Binding, KeyBindingFn, KeyPress};
pub use style::{
    default, Catalog, DiagnosticStyle, PopupStyle, ScrollbarStyle, Status, Style, StyleFn,
};
pub use widget::{code_editor, CodeEditor, FoldGuides};

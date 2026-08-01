use std::path::{Path, PathBuf};

use typst_iced_editor::{Action, Content, Diagnostic};

const UNTITLED_NAME: &str = "Sem título.typ";
const UNTITLED_MAIN: &str = "untitled.typ";

/// A Typst document together with the state needed by file operations.
pub struct Document {
    path: Option<PathBuf>,
    content: Content,
    saved_text: Option<String>,
    dirty: bool,
}

impl Document {
    /// Creates a new, empty document that has no pending changes.
    pub fn new() -> Self {
        Self {
            path: None,
            content: Content::new(),
            saved_text: Some(String::new()),
            dirty: false,
        }
    }

    /// Creates an unsaved document prefilled with tutorial text.
    pub fn draft(text: &str) -> Self {
        Self {
            path: None,
            content: Content::with_text(text),
            saved_text: None,
            dirty: true,
        }
    }

    /// Creates a document loaded from disk.
    pub fn opened(path: PathBuf, text: String) -> Self {
        Self {
            path: Some(path),
            content: Content::with_text(&text),
            saved_text: Some(text),
            dirty: false,
        }
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    pub fn display_name(&self) -> String {
        self.path
            .as_deref()
            .and_then(Path::file_name)
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| UNTITLED_NAME.to_owned())
    }

    pub fn directory(&self, fallback: &Path) -> PathBuf {
        self.path
            .as_deref()
            .and_then(Path::parent)
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or(fallback)
            .to_path_buf()
    }

    pub fn main_name(&self) -> String {
        self.path
            .as_deref()
            .and_then(Path::file_name)
            .and_then(|name| name.to_str())
            .unwrap_or(UNTITLED_MAIN)
            .to_owned()
    }

    pub fn content(&self) -> &Content {
        &self.content
    }

    pub fn perform(&mut self, action: Action) {
        let changed = action.is_edit();
        self.content.perform(action);

        if changed {
            self.refresh_dirty();
        }
    }

    pub fn edit(&mut self, edit: impl FnOnce(&mut Content) -> bool) -> bool {
        let changed = edit(&mut self.content);

        if changed {
            self.refresh_dirty();
        }

        changed
    }

    pub fn clear_diagnostics(&mut self) {
        self.content.clear_diagnostics();
    }

    pub fn set_diagnostics(&mut self, diagnostics: Vec<Diagnostic>) {
        self.content.set_diagnostics(diagnostics);
    }

    pub fn revision(&self) -> u64 {
        self.content.buffer().revision()
    }

    pub fn snapshot(&self) -> (u64, String) {
        let buffer = self.content.buffer();
        (buffer.revision(), buffer.text().to_owned())
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Marks the exact snapshot written to disk as saved.
    ///
    /// If the user edited the document while the write was in progress, the
    /// current text differs from `saved_text` and remains dirty.
    pub fn mark_saved(&mut self, path: PathBuf, saved_text: String) {
        self.path = Some(path);
        self.saved_text = Some(saved_text);
        self.refresh_dirty();
    }

    fn refresh_dirty(&mut self) {
        let buffer = self.content.buffer();
        self.dirty = self
            .saved_text
            .as_deref()
            .is_none_or(|saved| saved != buffer.text());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_document_becomes_dirty_after_an_edit() {
        let mut document = Document::new();
        assert!(!document.is_dirty());

        document.perform(Action::Insert("texto".to_owned()));

        assert!(document.is_dirty());
    }

    #[test]
    fn saving_an_older_snapshot_does_not_hide_newer_edits() {
        let mut document = Document::new();
        document.perform(Action::Insert("primeira".to_owned()));
        let (_, snapshot) = document.snapshot();
        document.perform(Action::Insert(" segunda".to_owned()));

        document.mark_saved(PathBuf::from("documento.typ"), snapshot);

        assert!(document.is_dirty());
        assert_eq!(document.path(), Some(Path::new("documento.typ")));
    }

    #[test]
    fn undoing_to_the_saved_text_clears_the_dirty_state() {
        let mut document = Document::opened(PathBuf::from("documento.typ"), "conteúdo".to_owned());
        let (_, text) = document.snapshot();
        document.perform(Action::MoveTo(text.len()));
        document.perform(Action::Insert(" alterado".to_owned()));
        assert!(document.is_dirty());

        document.perform(Action::Undo);

        assert!(!document.is_dirty());
    }
}

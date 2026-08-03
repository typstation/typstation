use std::{
    ops::{Deref, DerefMut, Range},
    path::{Path, PathBuf},
};

use typst_iced_editor::{Action, Content, Diagnostic};

const UNTITLED_NAME: &str = "Sem título.typ";
const UNTITLED_MAIN: &str = "untitled.typ";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DocumentId(u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExternalChangeKind {
    Modified,
    Deleted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExternalUpdate {
    Unchanged,
    Reloaded,
    Conflict,
}

enum ExternalChange {
    Modified(String),
    Deleted,
}

/// A Typst document together with the state needed by file operations.
pub struct Document {
    path: Option<PathBuf>,
    content: Content,
    saved_text: Option<String>,
    dirty: bool,
    storage_revision: u64,
    external_change: Option<ExternalChange>,
}

impl Document {
    /// Creates a new, empty document that has no pending changes.
    pub fn new() -> Self {
        Self {
            path: None,
            content: Content::new(),
            saved_text: Some(String::new()),
            dirty: false,
            storage_revision: 0,
            external_change: None,
        }
    }

    /// Creates an unsaved document prefilled with tutorial text.
    pub fn draft(text: &str) -> Self {
        Self {
            path: None,
            content: Content::with_text(text),
            saved_text: None,
            dirty: true,
            storage_revision: 0,
            external_change: None,
        }
    }

    /// Creates a document loaded from disk.
    pub fn opened(path: PathBuf, text: String) -> Self {
        Self {
            path: Some(path),
            content: Content::with_text(&text),
            saved_text: Some(text),
            dirty: false,
            storage_revision: 0,
            external_change: None,
        }
    }

    /// Restores the durable parts of a document from a previous session.
    pub fn restored(path: Option<PathBuf>, text: String, saved_text: Option<String>) -> Self {
        let dirty = saved_text
            .as_deref()
            .is_none_or(|saved| saved != text.as_str());

        Self {
            path,
            content: Content::with_text(&text),
            saved_text,
            dirty,
            storage_revision: 0,
            external_change: None,
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

    pub fn set_search_matches(&mut self, matches: Vec<Range<usize>>, current: Option<usize>) {
        self.content.set_search_matches(matches, current);
    }

    pub fn clear_search_matches(&mut self) {
        self.content.clear_search_matches();
    }

    pub fn search_matches(&self) -> Vec<Range<usize>> {
        self.content.search_matches()
    }

    pub fn current_search_match(&self) -> Option<usize> {
        self.content.current_search_match()
    }

    pub fn reveal_search_match(&mut self, index: usize) -> bool {
        self.content.reveal_search_match(index)
    }

    pub fn selection_text(&self) -> Option<String> {
        self.content.selection_text()
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

    pub fn saved_text(&self) -> Option<&str> {
        self.saved_text.as_deref()
    }

    pub fn storage_revision(&self) -> u64 {
        self.storage_revision
    }

    pub fn external_change(&self) -> Option<ExternalChangeKind> {
        self.external_change.as_ref().map(|change| match change {
            ExternalChange::Modified(_) => ExternalChangeKind::Modified,
            ExternalChange::Deleted => ExternalChangeKind::Deleted,
        })
    }

    pub fn observe_disk_source(&mut self, source: String) -> ExternalUpdate {
        if self.saved_text.as_deref() == Some(source.as_str()) {
            self.external_change = None;
            return ExternalUpdate::Unchanged;
        }

        if self.dirty {
            self.external_change = Some(ExternalChange::Modified(source));
            ExternalUpdate::Conflict
        } else {
            self.replace_from_disk(source);
            ExternalUpdate::Reloaded
        }
    }

    pub fn observe_deleted_file(&mut self) -> ExternalUpdate {
        if self.path.is_none() || self.saved_text.is_none() {
            return ExternalUpdate::Unchanged;
        }

        self.external_change = Some(ExternalChange::Deleted);
        ExternalUpdate::Conflict
    }

    pub fn reload_external_change(&mut self) -> bool {
        let Some(ExternalChange::Modified(source)) = self.external_change.take() else {
            return false;
        };

        self.replace_from_disk(source);
        true
    }

    pub fn keep_local_after_external_change(&mut self) -> bool {
        let Some(change) = self.external_change.take() else {
            return false;
        };

        self.saved_text = match change {
            ExternalChange::Modified(source) => Some(source),
            ExternalChange::Deleted => None,
        };
        self.storage_revision += 1;
        self.refresh_dirty();
        true
    }

    /// Marks the exact snapshot written to disk as saved.
    ///
    /// If the user edited the document while the write was in progress, the
    /// current text differs from `saved_text` and remains dirty.
    pub fn mark_saved(&mut self, path: PathBuf, saved_text: String) {
        self.path = Some(path);
        self.saved_text = Some(saved_text);
        self.storage_revision += 1;
        self.external_change = None;
        self.refresh_dirty();
    }

    fn replace_from_disk(&mut self, source: String) {
        self.content = Content::with_text(&source);
        self.saved_text = Some(source);
        self.dirty = false;
        self.storage_revision += 1;
        self.external_change = None;
    }

    fn refresh_dirty(&mut self) {
        let buffer = self.content.buffer();
        self.dirty = self
            .saved_text
            .as_deref()
            .is_none_or(|saved| saved != buffer.text());
    }
}

struct DocumentEntry {
    id: DocumentId,
    document: Document,
}

pub struct Documents {
    entries: Vec<DocumentEntry>,
    active: usize,
    next_id: u64,
}

impl Documents {
    pub fn new(initial: Document) -> Self {
        Self {
            entries: vec![DocumentEntry {
                id: DocumentId(0),
                document: initial,
            }],
            active: 0,
            next_id: 1,
        }
    }

    pub fn restored(mut documents: Vec<Document>, active: usize) -> Self {
        if documents.is_empty() {
            documents.push(Document::new());
        }

        let entries = documents
            .into_iter()
            .enumerate()
            .map(|(index, document)| DocumentEntry {
                id: DocumentId(index as u64),
                document,
            })
            .collect::<Vec<_>>();
        let active = active.min(entries.len().saturating_sub(1));
        let next_id = entries.len() as u64;

        Self {
            entries,
            active,
            next_id,
        }
    }

    pub fn active_id(&self) -> DocumentId {
        self.entries[self.active].id
    }

    pub fn active(&self) -> &Document {
        &self.entries[self.active].document
    }

    pub fn active_mut(&mut self) -> &mut Document {
        &mut self.entries[self.active].document
    }

    pub fn get(&self, id: DocumentId) -> Option<&Document> {
        self.entries
            .iter()
            .find(|entry| entry.id == id)
            .map(|entry| &entry.document)
    }

    pub fn get_mut(&mut self, id: DocumentId) -> Option<&mut Document> {
        self.entries
            .iter_mut()
            .find(|entry| entry.id == id)
            .map(|entry| &mut entry.document)
    }

    pub fn iter(&self) -> impl Iterator<Item = (DocumentId, &Document)> {
        self.entries.iter().map(|entry| (entry.id, &entry.document))
    }

    pub fn add(&mut self, document: Document) -> DocumentId {
        let id = DocumentId(self.next_id);
        self.next_id += 1;
        self.entries.push(DocumentEntry { id, document });
        self.active = self.entries.len() - 1;
        id
    }

    pub fn activate(&mut self, id: DocumentId) -> bool {
        let Some(index) = self.entries.iter().position(|entry| entry.id == id) else {
            return false;
        };

        let changed = self.active != index;
        self.active = index;
        changed
    }

    pub fn find_path(&self, path: &Path) -> Option<DocumentId> {
        self.iter()
            .find_map(|(id, document)| (document.path() == Some(path)).then_some(id))
    }

    pub fn remove(&mut self, id: DocumentId) -> Option<Document> {
        let index = self.entries.iter().position(|entry| entry.id == id)?;
        let removed = self.entries.remove(index).document;

        if self.entries.is_empty() {
            let _ = self.add(Document::new());
        } else if self.active > index {
            self.active -= 1;
        } else if self.active >= self.entries.len() {
            self.active = self.entries.len() - 1;
        }

        Some(removed)
    }
}

impl Deref for Documents {
    type Target = Document;

    fn deref(&self) -> &Self::Target {
        self.active()
    }
}

impl DerefMut for Documents {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.active_mut()
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

    #[test]
    fn clean_external_edits_are_reloaded_automatically() {
        let mut document = Document::opened(PathBuf::from("documento.typ"), "original".to_owned());

        let update = document.observe_disk_source("externo".to_owned());

        assert_eq!(update, ExternalUpdate::Reloaded);
        assert_eq!(document.snapshot().1, "externo");
        assert!(!document.is_dirty());
        assert_eq!(document.external_change(), None);
    }

    #[test]
    fn dirty_external_edits_require_an_explicit_decision() {
        let mut document = Document::opened(PathBuf::from("documento.typ"), "original".to_owned());
        document.perform(Action::MoveTo("original".len()));
        document.perform(Action::Insert(" local".to_owned()));

        let update = document.observe_disk_source("externo".to_owned());

        assert_eq!(update, ExternalUpdate::Conflict);
        assert_eq!(document.snapshot().1, "original local");
        assert_eq!(
            document.external_change(),
            Some(ExternalChangeKind::Modified)
        );

        assert!(document.keep_local_after_external_change());
        assert!(document.is_dirty());
        assert_eq!(document.external_change(), None);
    }

    #[test]
    fn acknowledged_deletion_is_not_reported_repeatedly() {
        let mut document = Document::opened(PathBuf::from("documento.typ"), "conteúdo".to_owned());

        assert_eq!(document.observe_deleted_file(), ExternalUpdate::Conflict);
        assert!(document.keep_local_after_external_change());
        assert_eq!(document.observe_deleted_file(), ExternalUpdate::Unchanged);
        assert!(document.is_dirty());
    }

    #[test]
    fn documents_can_be_added_activated_and_removed() {
        let mut documents = Documents::new(Document::new());
        let first = documents.active_id();
        let second = documents.add(Document::draft("segundo"));

        assert_eq!(documents.iter().count(), 2);
        assert_eq!(documents.active_id(), second);
        assert!(documents.activate(first));
        assert_eq!(documents.snapshot().1, "");

        documents.remove(first);

        assert_eq!(documents.iter().count(), 1);
        assert_eq!(documents.active_id(), second);
        assert_eq!(documents.snapshot().1, "segundo");
    }

    #[test]
    fn restored_document_rebuilds_its_dirty_state() {
        let clean = Document::restored(
            Some(PathBuf::from("documento.typ")),
            "salvo".to_owned(),
            Some("salvo".to_owned()),
        );
        let dirty = Document::restored(
            Some(PathBuf::from("documento.typ")),
            "rascunho".to_owned(),
            Some("salvo".to_owned()),
        );

        assert!(!clean.is_dirty());
        assert!(dirty.is_dirty());
        assert_eq!(dirty.saved_text(), Some("salvo"));
    }
}

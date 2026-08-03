mod compiler;
mod document;
mod formatting;
mod project;
mod search;
mod session;
mod settings;
mod watcher;

use std::{
    collections::HashSet,
    fs,
    io::{self, Write},
    ops::Range,
    path::{Path, PathBuf},
    time::Duration,
};

use document::{Document, DocumentId, Documents, ExternalChangeKind, ExternalUpdate};
use iced::{
    Alignment, Element,
    Length::{Fill, FillPortion},
    Subscription, Task, Theme, event, keyboard,
    time::{self, Instant},
    widget::{
        Id, button, checkbox, column, container, operation, pane_grid, row, scrollable, slider,
        svg, text, text_input, tooltip,
    },
    window,
};
use rfd::{AsyncFileDialog, AsyncMessageDialog, MessageButtons, MessageDialogResult, MessageLevel};
use typst_iced_editor::{Action, code_editor};
use typstation::world::SourceOverlay;

const DEBOUNCE: Duration = Duration::from_millis(250);
const DEBOUNCE_TICK: Duration = Duration::from_millis(50);
const SESSION_DEBOUNCE: Duration = Duration::from_millis(750);
const SESSION_TICK: Duration = Duration::from_millis(200);
const WATCHER_DEBOUNCE: Duration = Duration::from_millis(150);
const WATCHER_TICK: Duration = Duration::from_millis(50);
const DEMO: &str = include_str!("demo.typ");

fn main() -> iced::Result {
    iced::application(App::boot, App::update, App::view)
        .subscription(App::subscription)
        .title(App::title)
        .theme(App::theme)
        .window_size([1200.0, 800.0])
        .centered()
        .exit_on_close_request(false)
        .run()
}

struct App {
    document: Documents,
    panes: pane_grid::State<Pane>,
    workspace_root: PathBuf,
    compiler: Option<compiler::Sender>,
    pending_compile: Option<PendingCompile>,
    next_request_id: u64,
    latest_request_id: Option<u64>,
    preview: Vec<PreviewPage>,
    preview_status: PreviewStatus,
    file_busy: bool,
    pending_after_save: Option<DestructiveFileAction>,
    pending_pdf_export: Option<PendingPdfExport>,
    search: SearchState,
    search_input_id: Id,
    project_files: Vec<PathBuf>,
    selected_project_file: Option<PathBuf>,
    diagnostics: Vec<compiler::ReportedDiagnostic>,
    pending_diagnostic_reveal: Option<(PathBuf, Range<usize>)>,
    project_scan_busy: bool,
    external_check_busy: bool,
    watcher_deadline: Option<Instant>,
    discarded_on_close: HashSet<DocumentId>,
    session: SessionTracker,
    settings: settings::Settings,
    settings_visible: bool,
    file_status: Option<String>,
}

struct PreviewPage {
    handle: svg::Handle,
    width: f32,
    height: f32,
}

#[derive(Debug, Clone)]
enum Message {
    Editor(Action),
    PaneDragged(pane_grid::DragEvent),
    PaneResized(pane_grid::ResizeEvent),
    CompileNow,
    DebounceTick(Instant),
    Compiler(compiler::Event),
    Bold,
    Italic,
    Underline,
    PrefixLines(String),
    ToggleSettings,
    TabWidthChanged(u16),
    AutoPairsChanged(bool),
    AutoIndentChanged(bool),
    WrapLinesChanged(bool),
    ShowGutterChanged(bool),
    EditorFontSizeChanged(u16),
    LightThemeChanged(bool),
    PreviewZoomIn,
    PreviewZoomOut,
    PreviewZoomReset,
    PreviewZoomChanged(u16),
    OpenSearch,
    OpenReplace,
    CloseSearch,
    ShowReplace,
    SearchQueryChanged(String),
    SearchReplacementChanged(String),
    SearchCaseChanged(bool),
    SearchWholeWordChanged(bool),
    SearchNext,
    SearchPrevious,
    ReplaceCurrent,
    ReplaceAll,
    ActivateDocument(DocumentId),
    CloseDocument(DocumentId),
    NewDocument,
    OpenDocument,
    OpenProject,
    OpenProjectFile(PathBuf),
    CreateProjectFile,
    RenameProjectFile,
    DeleteProjectFile,
    ProjectOperationFinished(project::OperationOutcome),
    OpenDiagnostic(compiler::DiagnosticTarget, Range<usize>),
    ProjectFolderSelected(Option<PathBuf>),
    ProjectScanned(project::ScanOutcome),
    Watcher(watcher::Event),
    WatcherTick(Instant),
    ExternalFilesChecked(Vec<ExternalFileResult>),
    ReloadExternal,
    KeepLocalAfterExternal,
    SessionTick(Instant),
    SessionWriteFinished(SessionWriteOutcome),
    SaveDocument,
    SaveDocumentAs,
    ExportPdf,
    PdfPathSelected(PdfPathOutcome),
    PdfWriteFinished(PdfWriteOutcome),
    CloseRequested(window::Id),
    UnsavedDecision {
        action: DestructiveFileAction,
        decision: UnsavedDecision,
    },
    OpenFinished(OpenOutcome),
    SaveFinished(SaveOutcome),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pane {
    Editor,
    Preview,
}

#[derive(Debug, Clone, Copy)]
struct PendingCompile {
    deadline: Instant,
    reset_files: bool,
}

#[derive(Debug, Clone)]
struct PendingPdfExport {
    request_id: u64,
    revision: u64,
    path: PathBuf,
}

#[derive(Debug)]
struct SessionTracker {
    path: Option<PathBuf>,
    revision: u64,
    deadline: Option<Instant>,
    write_busy: bool,
    close_after_write: Option<window::Id>,
}

impl SessionTracker {
    fn new(path: Option<PathBuf>) -> Self {
        Self {
            path,
            revision: 0,
            deadline: None,
            write_busy: false,
            close_after_write: None,
        }
    }
}

#[derive(Debug, Default)]
struct SearchState {
    visible: bool,
    replace_visible: bool,
    query: String,
    replacement: String,
    case_sensitive: bool,
    whole_word: bool,
}

enum PreviewStatus {
    Waiting,
    Compiling,
    Ready { pages: usize, warnings: usize },
    Failed { errors: usize, summary: String },
}

#[derive(Debug, Clone, Copy)]
enum DestructiveFileAction {
    CloseDocument(DocumentId),
    Close(window::Id),
}

#[derive(Debug, Clone, Copy)]
enum UnsavedDecision {
    Save,
    Discard,
    Cancel,
}

#[derive(Debug, Clone)]
enum OpenOutcome {
    Cancelled,
    Loaded { path: PathBuf, source: String },
    Failed(String),
}

#[derive(Debug, Clone)]
enum SaveOutcome {
    Cancelled {
        document_id: DocumentId,
    },
    Saved {
        document_id: DocumentId,
        path: PathBuf,
        source: String,
    },
    Failed {
        document_id: DocumentId,
        error: String,
    },
}

#[derive(Debug, Clone)]
enum ExternalFileResult {
    Source {
        document_id: DocumentId,
        storage_revision: u64,
        source: String,
    },
    Deleted {
        document_id: DocumentId,
        storage_revision: u64,
    },
    Failed {
        document_id: DocumentId,
        storage_revision: u64,
        error: String,
    },
}

#[derive(Debug, Clone)]
struct SessionWriteOutcome {
    revision: u64,
    result: Result<(), String>,
}

#[derive(Debug, Clone)]
enum PdfPathOutcome {
    Cancelled,
    Selected(PathBuf),
}

#[derive(Debug, Clone)]
enum PdfWriteOutcome {
    Saved(PathBuf),
    Failed(String),
}

impl App {
    fn boot() -> (Self, Task<Message>) {
        let session_path = session::default_path();
        let stored = match session_path.as_deref() {
            Some(path) => session::load(path),
            None => Ok(None),
        };
        let mut app = match stored {
            Ok(Some(stored)) => Self::restore(stored, session_path),
            Ok(None) => Self::fresh(session_path),
            Err(error) => {
                eprintln!("erro ao restaurar sessão: {error}");
                let mut app = Self::fresh(session_path);
                app.file_status = Some(format!("Não foi possível restaurar a sessão: {error}"));
                app
            }
        };
        let project_scan = app.refresh_project_files();
        let external_check = app.check_external_files();

        (app, Task::batch([project_scan, external_check]))
    }

    #[cfg(test)]
    fn new() -> Self {
        Self::fresh(None)
    }

    fn fresh(session_path: Option<PathBuf>) -> Self {
        let workspace_root = std::env::current_dir().unwrap_or_else(|error| {
            eprintln!("erro ao obter diretório atual: {error}");
            PathBuf::from(".")
        });
        let panes = panes_from_layout(session::PaneLayout::default());

        Self::build(
            Documents::new(Document::draft(DEMO)),
            panes,
            workspace_root,
            session_path,
            settings::Settings::default(),
            "O tutorial inicial ainda não foi salvo".to_owned(),
        )
    }

    fn restore(stored: session::Session, session_path: Option<PathBuf>) -> Self {
        let documents = stored
            .documents
            .into_iter()
            .map(|document| Document::restored(document.path, document.text, document.saved_text))
            .collect();
        let document = Documents::restored(documents, stored.active_document);
        let panes = panes_from_layout(stored.pane_layout);

        Self::build(
            document,
            panes,
            stored.workspace_root,
            session_path,
            stored.settings,
            "Sessão anterior restaurada".to_owned(),
        )
    }

    fn build(
        document: Documents,
        panes: pane_grid::State<Pane>,
        workspace_root: PathBuf,
        session_path: Option<PathBuf>,
        settings: settings::Settings,
        file_status: String,
    ) -> Self {
        let mut app = Self {
            document,
            panes,
            workspace_root,
            compiler: None,
            pending_compile: Some(PendingCompile {
                deadline: Instant::now(),
                reset_files: true,
            }),
            next_request_id: 0,
            latest_request_id: None,
            preview: Vec::new(),
            preview_status: PreviewStatus::Waiting,
            file_busy: false,
            pending_after_save: None,
            pending_pdf_export: None,
            search: SearchState::default(),
            search_input_id: Id::unique(),
            project_files: Vec::new(),
            selected_project_file: None,
            diagnostics: Vec::new(),
            pending_diagnostic_reveal: None,
            project_scan_busy: false,
            external_check_busy: false,
            watcher_deadline: None,
            discarded_on_close: HashSet::new(),
            session: SessionTracker::new(session_path),
            settings: settings.validate(),
            settings_visible: false,
            file_status: Some(file_status),
        };
        app.apply_editor_settings();
        app
    }

    fn title(&self) -> String {
        let dirty = if self.document.is_dirty() { " *" } else { "" };

        format!(
            "{}{} - Typstation v{}",
            self.document.display_name(),
            dirty,
            env!("CARGO_PKG_VERSION")
        )
    }

    fn theme(&self) -> Theme {
        match self.settings.theme {
            settings::ThemeMode::Dark => Theme::Dark,
            settings::ThemeMode::Light => Theme::Light,
        }
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Editor(action) => {
                if action.is_edit() && self.file_busy {
                    return Task::none();
                }
                let changed = self.document.perform(action);

                if changed {
                    self.clear_compile_diagnostics();
                    self.file_status = None;
                    self.mark_session_changed();
                    self.schedule_compile(DEBOUNCE, false);
                    self.refresh_search_matches(None, false);
                }

                Task::none()
            }
            Message::Bold => {
                if self.file_busy {
                    return Task::none();
                }
                if self
                    .document
                    .edit(|content| formatting::toggle_surround(content, "*", "*"))
                {
                    self.after_formatting();
                }

                Task::none()
            }
            Message::Italic => {
                if self.file_busy {
                    return Task::none();
                }
                if self
                    .document
                    .edit(|content| formatting::toggle_surround(content, "_", "_"))
                {
                    self.after_formatting();
                }

                Task::none()
            }
            Message::Underline => {
                if self.file_busy {
                    return Task::none();
                }
                if self
                    .document
                    .edit(|content| formatting::toggle_surround(content, "#underline[", "]"))
                {
                    self.after_formatting();
                }

                Task::none()
            }
            Message::PrefixLines(prefix) => {
                if self.file_busy {
                    return Task::none();
                }
                if self
                    .document
                    .edit(|content| formatting::toggle_line_prefix(content, &prefix))
                {
                    self.after_formatting();
                }

                Task::none()
            }
            Message::ToggleSettings => {
                self.settings_visible = !self.settings_visible;
                Task::none()
            }
            Message::TabWidthChanged(tab_width) => {
                self.settings.tab_width = usize::from(tab_width.clamp(1, 8));
                self.settings_changed(true);
                Task::none()
            }
            Message::AutoPairsChanged(auto_pairs) => {
                self.settings.auto_pairs = auto_pairs;
                self.settings_changed(true);
                Task::none()
            }
            Message::AutoIndentChanged(auto_indent) => {
                self.settings.auto_indent = auto_indent;
                self.settings_changed(true);
                Task::none()
            }
            Message::WrapLinesChanged(wrap_lines) => {
                self.settings.wrap_lines = wrap_lines;
                self.settings_changed(false);
                Task::none()
            }
            Message::ShowGutterChanged(show_gutter) => {
                self.settings.show_gutter = show_gutter;
                self.settings_changed(false);
                Task::none()
            }
            Message::EditorFontSizeChanged(size) => {
                self.settings.editor_font_size = size.clamp(10, 30);
                self.settings_changed(false);
                Task::none()
            }
            Message::LightThemeChanged(light) => {
                self.settings.theme = if light {
                    settings::ThemeMode::Light
                } else {
                    settings::ThemeMode::Dark
                };
                self.settings_changed(false);
                Task::none()
            }
            Message::PreviewZoomIn => {
                self.change_preview_zoom(10);
                Task::none()
            }
            Message::PreviewZoomOut => {
                self.change_preview_zoom(-10);
                Task::none()
            }
            Message::PreviewZoomReset => {
                self.settings.preview_zoom = 100;
                self.settings_changed(false);
                Task::none()
            }
            Message::PreviewZoomChanged(zoom) => {
                self.settings.preview_zoom = zoom.clamp(25, 300);
                self.settings_changed(false);
                Task::none()
            }
            Message::OpenSearch => self.open_search(false),
            Message::OpenReplace => self.open_search(true),
            Message::CloseSearch => {
                self.search.visible = false;
                self.document.clear_search_matches();
                Task::none()
            }
            Message::ShowReplace => {
                self.search.replace_visible = true;
                Task::none()
            }
            Message::SearchQueryChanged(query) => {
                self.search.query = query;
                self.refresh_search_matches(Some(0), true);
                Task::none()
            }
            Message::SearchReplacementChanged(replacement) => {
                self.search.replacement = replacement;
                Task::none()
            }
            Message::SearchCaseChanged(case_sensitive) => {
                self.search.case_sensitive = case_sensitive;
                self.refresh_search_matches(Some(0), true);
                Task::none()
            }
            Message::SearchWholeWordChanged(whole_word) => {
                self.search.whole_word = whole_word;
                self.refresh_search_matches(Some(0), true);
                Task::none()
            }
            Message::SearchNext => {
                self.move_search_match(false);
                Task::none()
            }
            Message::SearchPrevious => {
                self.move_search_match(true);
                Task::none()
            }
            Message::ReplaceCurrent => {
                self.replace_current_match();
                Task::none()
            }
            Message::ReplaceAll => {
                self.replace_all_matches();
                Task::none()
            }
            Message::PaneDragged(event) => {
                if let pane_grid::DragEvent::Dropped { pane, target } = event {
                    self.panes.drop(pane, target);
                    self.mark_session_changed();
                }

                Task::none()
            }
            Message::PaneResized(event) => {
                self.panes.resize(event.split, event.ratio);
                self.mark_session_changed();
                Task::none()
            }
            Message::CompileNow => {
                self.schedule_compile(Duration::ZERO, true);
                self.dispatch_compile(Instant::now());
                Task::none()
            }
            Message::DebounceTick(now) => {
                self.dispatch_compile(now);
                Task::none()
            }
            Message::Compiler(event) => self.handle_compiler_event(event),
            Message::ActivateDocument(id) => {
                if !self.file_busy {
                    self.activate_document(id);
                }
                Task::none()
            }
            Message::CloseDocument(id) => {
                self.request_destructive_action(DestructiveFileAction::CloseDocument(id))
            }
            Message::NewDocument => {
                if !self.file_busy {
                    self.new_document();
                }
                Task::none()
            }
            Message::OpenDocument => self.start_open_document(),
            Message::OpenProject => self.start_open_project(),
            Message::OpenProjectFile(path) => self.open_project_file(path),
            Message::CreateProjectFile => self.start_create_project_file(),
            Message::RenameProjectFile => self.start_rename_project_file(),
            Message::DeleteProjectFile => self.start_delete_project_file(),
            Message::ProjectOperationFinished(outcome) => self.handle_project_operation(outcome),
            Message::OpenDiagnostic(target, range) => self.open_diagnostic(target, range),
            Message::ProjectFolderSelected(path) => self.handle_project_folder_selected(path),
            Message::ProjectScanned(outcome) => {
                if outcome.root != self.workspace_root {
                    return Task::none();
                }

                self.project_scan_busy = false;
                match outcome.files {
                    Ok(files) => {
                        if self
                            .selected_project_file
                            .as_ref()
                            .is_some_and(|selected| !files.contains(selected) && !selected.exists())
                        {
                            self.selected_project_file = None;
                        }
                        self.project_files = files;
                    }
                    Err(error) => {
                        eprintln!("erro ao examinar projeto: {error}");
                        self.file_status = Some(format!("Erro no projeto: {error}"));
                    }
                }
                Task::none()
            }
            Message::Watcher(event) => self.handle_watcher_event(event),
            Message::WatcherTick(now) => self.dispatch_watcher_refresh(now),
            Message::ExternalFilesChecked(results) => self.handle_external_files(results),
            Message::ReloadExternal => {
                self.reload_external_change();
                Task::none()
            }
            Message::KeepLocalAfterExternal => {
                if self.document.keep_local_after_external_change() {
                    self.file_status = Some("A versão local foi mantida".to_owned());
                    self.mark_session_changed();
                }
                Task::none()
            }
            Message::SessionTick(now) => self.dispatch_session_save(now),
            Message::SessionWriteFinished(outcome) => self.handle_session_write_finished(outcome),
            Message::SaveDocument => {
                if self.file_busy {
                    return Task::none();
                }

                self.pending_after_save = None;

                if self.document.is_dirty() {
                    self.start_save(false)
                } else {
                    Task::none()
                }
            }
            Message::SaveDocumentAs => {
                if self.file_busy {
                    return Task::none();
                }

                self.pending_after_save = None;
                self.start_save(true)
            }
            Message::ExportPdf => self.start_pdf_export(),
            Message::PdfPathSelected(outcome) => self.handle_pdf_path_selected(outcome),
            Message::PdfWriteFinished(outcome) => self.handle_pdf_write_finished(outcome),
            Message::CloseRequested(id) => {
                self.request_destructive_action(DestructiveFileAction::Close(id))
            }
            Message::UnsavedDecision { action, decision } => {
                self.file_busy = false;

                match decision {
                    UnsavedDecision::Save => {
                        self.pending_after_save = Some(action);
                        self.start_save(false)
                    }
                    UnsavedDecision::Discard => {
                        self.pending_after_save = None;
                        self.discard_destructive_action(action)
                    }
                    UnsavedDecision::Cancel => {
                        self.pending_after_save = None;
                        if matches!(action, DestructiveFileAction::Close(_)) {
                            self.discarded_on_close.clear();
                        }
                        Task::none()
                    }
                }
            }
            Message::OpenFinished(outcome) => self.handle_open_finished(outcome),
            Message::SaveFinished(outcome) => self.handle_save_finished(outcome),
        }
    }

    fn subscription(&self) -> Subscription<Message> {
        let compiler = compiler::subscription(self.compiler_config()).map(Message::Compiler);
        let close_requests = window::close_requests().map(Message::CloseRequested);
        let shortcuts = shortcut_subscription();
        let watcher = watcher::subscription(self.workspace_root.clone()).map(Message::Watcher);
        let mut subscriptions = vec![compiler, close_requests, shortcuts, watcher];

        if self.pending_compile.is_some() {
            subscriptions.push(time::every(DEBOUNCE_TICK).map(Message::DebounceTick));
        }
        if self.session.deadline.is_some() {
            subscriptions.push(time::every(SESSION_TICK).map(Message::SessionTick));
        }
        if self.watcher_deadline.is_some() {
            subscriptions.push(time::every(WATCHER_TICK).map(Message::WatcherTick));
        }

        Subscription::batch(subscriptions)
    }

    fn view(&self) -> Element<'_, Message> {
        let panes = pane_grid(&self.panes, |_id, pane, _is_maximized| {
            let content: Element<'_, Message> = match pane {
                Pane::Editor => code_editor(self.document.content())
                    .on_action(Message::Editor)
                    .wrap(self.settings.wrap_lines)
                    .gutter(self.settings.show_gutter)
                    .size(f32::from(self.settings.editor_font_size))
                    .into(),
                Pane::Preview => self.preview_view(),
            };

            pane_grid::Content::new(content)
        })
        .spacing(8)
        .min_size(200)
        .on_drag(Message::PaneDragged)
        .on_resize(10, Message::PaneResized);

        let new = file_button("Novo", Message::NewDocument, !self.file_busy);
        let open = file_button("Abrir", Message::OpenDocument, !self.file_busy);
        let open_project = file_button("Abrir pasta", Message::OpenProject, !self.file_busy);
        let save = file_button(
            "Salvar",
            Message::SaveDocument,
            !self.file_busy && self.document.is_dirty(),
        );
        let save_as = file_button("Salvar como", Message::SaveDocumentAs, !self.file_busy);
        let export_pdf = file_button(
            "Exportar PDF",
            Message::ExportPdf,
            !self.file_busy && self.compiler.is_some(),
        );

        let file_toolbar = row![
            new,
            open,
            open_project,
            save,
            save_as,
            export_pdf,
            button("▶").on_press(Message::CompileNow),
            button("Buscar").on_press(Message::OpenSearch),
            button("Configurações").on_press(Message::ToggleSettings),
        ]
        .spacing(4)
        .padding(4);
        let edit_toolbar = row![
            command_button("↶", "Desfazer", Action::Undo, !self.file_busy),
            command_button("↷", "Refazer", Action::Redo, !self.file_busy),
            command_button(
                "//",
                "Alternar comentário de linha",
                Action::ToggleLineComment,
                !self.file_busy,
            ),
            command_button(
                "⧉",
                "Duplicar linha ou seleção",
                Action::DuplicateLine,
                !self.file_busy,
            ),
            command_button(
                "↑",
                "Mover linha para cima",
                Action::MoveLineUp,
                !self.file_busy,
            ),
            command_button(
                "↓",
                "Mover linha para baixo",
                Action::MoveLineDown,
                !self.file_busy,
            ),
            file_button("B", Message::Bold, !self.file_busy),
            file_button("I", Message::Italic, !self.file_busy),
            file_button("U", Message::Underline, !self.file_busy),
            file_button("Lista", Message::PrefixLines("- ".into()), !self.file_busy,),
            file_button(
                "Numeração",
                Message::PrefixLines("+ ".into()),
                !self.file_busy,
            ),
        ]
        .spacing(4)
        .padding([0, 4]);

        let status = container(text(self.status_text()).size(13))
            .width(Fill)
            .padding([5, 8]);

        let tabs = self.tabs_view();
        let workspace = row![self.project_view(), panes].height(Fill);
        let mut content = column![file_toolbar, edit_toolbar];

        if self.settings_visible {
            content = content.push(self.settings_view());
        }

        content = content.push(tabs);

        if self.document.external_change().is_some() {
            content = content.push(self.external_change_view());
        }

        if self.search.visible {
            content = content.push(self.search_view());
        }

        content
            .push(workspace)
            .push(status)
            .width(Fill)
            .height(Fill)
            .into()
    }

    fn preview_view(&self) -> Element<'_, Message> {
        if self.preview.is_empty() {
            return container(text("Preview indisponível"))
                .center_x(Fill)
                .center_y(Fill)
                .into();
        }

        let zoom = f32::from(self.settings.preview_zoom) / 100.0;
        let controls = row![
            file_button(
                "−",
                Message::PreviewZoomOut,
                self.settings.preview_zoom > 25,
            ),
            button(text(format!("{}%", self.settings.preview_zoom)))
                .on_press(Message::PreviewZoomReset),
            file_button(
                "+",
                Message::PreviewZoomIn,
                self.settings.preview_zoom < 300,
            ),
            text(format!("{} página(s)", self.preview.len())).size(13),
        ]
        .align_y(Alignment::Center)
        .spacing(4)
        .padding([4, 8]);
        let mut pages = column![].align_x(Alignment::Center).spacing(12);

        for (index, page) in self.preview.iter().enumerate() {
            let width = (page.width * zoom).max(1.0);
            let height = (page.height * zoom).max(1.0);
            pages = pages.push(
                column![
                    text(format!("Página {}", index + 1)).size(12),
                    svg(page.handle.clone())
                        .width(iced::Length::Fixed(width))
                        .height(iced::Length::Fixed(height)),
                ]
                .align_x(Alignment::Center)
                .spacing(4),
            );
        }

        column![
            controls,
            scrollable(container(pages).padding(12))
                .direction(scrollable::Direction::Both {
                    vertical: scrollable::Scrollbar::default(),
                    horizontal: scrollable::Scrollbar::default(),
                })
                .height(Fill),
        ]
        .height(Fill)
        .into()
    }

    fn settings_view(&self) -> Element<'_, Message> {
        let editor_size = row![
            text(format!("Tabulação: {}", self.settings.tab_width)).width(110),
            slider(
                1..=8,
                self.settings.tab_width as u16,
                Message::TabWidthChanged,
            )
            .width(120),
            text(format!(
                "Fonte do editor: {}",
                self.settings.editor_font_size
            ))
            .width(150),
            slider(
                10..=30,
                self.settings.editor_font_size,
                Message::EditorFontSizeChanged,
            )
            .width(140),
        ]
        .align_y(Alignment::Center)
        .spacing(6);
        let preview_size = row![
            text(format!("Zoom: {}%", self.settings.preview_zoom)).width(90),
            slider(
                25..=300,
                self.settings.preview_zoom,
                Message::PreviewZoomChanged,
            )
            .step(5u16)
            .width(180),
        ]
        .align_y(Alignment::Center)
        .spacing(6);
        let editing = row![
            checkbox(self.settings.auto_pairs)
                .label("Fechar pares")
                .on_toggle(Message::AutoPairsChanged),
            checkbox(self.settings.auto_indent)
                .label("Indentação automática")
                .on_toggle(Message::AutoIndentChanged),
            checkbox(self.settings.wrap_lines)
                .label("Quebrar linhas")
                .on_toggle(Message::WrapLinesChanged),
        ]
        .align_y(Alignment::Center)
        .spacing(10);
        let appearance = row![
            checkbox(self.settings.show_gutter)
                .label("Mostrar números de linha")
                .on_toggle(Message::ShowGutterChanged),
            checkbox(self.settings.theme == settings::ThemeMode::Light)
                .label("Tema claro")
                .on_toggle(Message::LightThemeChanged),
            button("Fechar").on_press(Message::ToggleSettings),
        ]
        .align_y(Alignment::Center)
        .spacing(10);

        container(column![editor_size, preview_size, editing, appearance].spacing(6))
            .width(Fill)
            .padding([6, 8])
            .into()
    }

    fn tabs_view(&self) -> Element<'_, Message> {
        let active = self.document.active_id();
        let mut tabs = row![].spacing(2).padding([0, 4]);

        for (id, document) in self.document.iter() {
            let mut label = document.display_name();
            if document.is_dirty() {
                label.push_str(" *");
            }
            if document.external_change().is_some() {
                label.push_str(" !");
            }

            let select = button(text(label));
            let select = if id != active && !self.file_busy {
                select.on_press(Message::ActivateDocument(id))
            } else {
                select
            };
            let close = button("X");
            let close = if self.file_busy {
                close
            } else {
                close.on_press(Message::CloseDocument(id))
            };

            tabs = tabs.push(row![select, close].spacing(1));
        }

        scrollable(tabs)
            .direction(scrollable::Direction::Horizontal(
                scrollable::Scrollbar::default(),
            ))
            .into()
    }

    fn project_view(&self) -> Element<'_, Message> {
        let mut files = column![].spacing(2);

        for path in &self.project_files {
            let relative = path.strip_prefix(&self.workspace_root).unwrap_or(path);
            let selected = self.selected_project_file.as_deref() == Some(path.as_path());
            let marker = if selected { "› " } else { "" };
            let item = button(text(format!("{marker}{}", relative.to_string_lossy()))).width(Fill);
            let item = if self.file_busy {
                item
            } else {
                item.on_press(Message::OpenProjectFile(path.clone()))
            };
            files = files.push(item);
        }

        let mut diagnostics = column![].spacing(2);
        for diagnostic in &self.diagnostics {
            let location = match &diagnostic.target {
                compiler::DiagnosticTarget::Main => self.document.display_name(),
                compiler::DiagnosticTarget::ProjectFile(path) => path
                    .strip_prefix(&self.workspace_root)
                    .unwrap_or(path)
                    .to_string_lossy()
                    .into_owned(),
            };
            let severity = match diagnostic.severity {
                compiler::DiagnosticSeverity::Error => "Erro",
                compiler::DiagnosticSeverity::Warning => "Aviso",
            };
            let label = format!(
                "{severity} em {location}: {}",
                truncate(&diagnostic.message, 70)
            );
            diagnostics = diagnostics.push(button(text(label).size(12)).width(Fill).on_press(
                Message::OpenDiagnostic(diagnostic.target.clone(), diagnostic.range.clone()),
            ));
        }

        let root = truncate(&self.workspace_root.to_string_lossy(), 28);
        let has_selection = self.selected_project_file.is_some();
        let actions = row![
            file_button("Novo arquivo", Message::CreateProjectFile, !self.file_busy),
            file_button(
                "Renomear",
                Message::RenameProjectFile,
                !self.file_busy && has_selection,
            ),
            file_button(
                "Excluir",
                Message::DeleteProjectFile,
                !self.file_busy && has_selection,
            ),
        ]
        .spacing(2);
        let file_height = if self.diagnostics.is_empty() {
            Fill
        } else {
            FillPortion(2)
        };
        let mut content = column![
            text("Projeto").size(16),
            text(root).size(12),
            actions,
            scrollable(files).height(file_height),
        ]
        .spacing(6);

        if !self.diagnostics.is_empty() {
            content = content
                .push(text(format!("Problemas ({})", self.diagnostics.len())).size(14))
                .push(scrollable(diagnostics).height(FillPortion(1)));
        }

        container(content)
            .width(280)
            .height(Fill)
            .padding([6, 8])
            .into()
    }

    fn external_change_view(&self) -> Element<'_, Message> {
        let kind = self.document.external_change();
        let message = match kind {
            Some(ExternalChangeKind::Modified) => "O arquivo foi alterado fora do Typstation",
            Some(ExternalChangeKind::Deleted) => "O arquivo foi removido fora do Typstation",
            None => "",
        };
        let mut actions = row![text(message)]
            .spacing(8)
            .push(button("Manter local").on_press(Message::KeepLocalAfterExternal));

        if kind == Some(ExternalChangeKind::Modified) {
            actions = actions.push(button("Recarregar").on_press(Message::ReloadExternal));
        } else {
            actions = actions.push(
                button("Fechar aba").on_press(Message::CloseDocument(self.document.active_id())),
            );
        }

        container(actions).width(Fill).padding([4, 8]).into()
    }

    fn search_view(&self) -> Element<'_, Message> {
        let matches = self.document.search_matches();
        let current = self
            .document
            .current_search_match()
            .filter(|index| *index < matches.len())
            .map_or(0, |index| index + 1);
        let count = text(format!("{current}/{}", matches.len())).width(60);
        let query = text_input("Buscar", &self.search.query)
            .id(self.search_input_id.clone())
            .on_input(Message::SearchQueryChanged)
            .on_submit(Message::SearchNext)
            .width(280);
        let mut find_row = row![
            query,
            button("↑").on_press(Message::SearchPrevious),
            button("↓").on_press(Message::SearchNext),
            count,
            checkbox(self.search.case_sensitive)
                .label("Maiúsculas")
                .on_toggle(Message::SearchCaseChanged),
            checkbox(self.search.whole_word)
                .label("Palavra inteira")
                .on_toggle(Message::SearchWholeWordChanged),
        ]
        .spacing(6);

        if !self.search.replace_visible {
            find_row = find_row.push(button("Substituir").on_press(Message::ShowReplace));
        }

        find_row = find_row.push(button("X").on_press(Message::CloseSearch));

        let mut panel = column![find_row].spacing(4);

        if self.search.replace_visible {
            let replace_row = row![
                text_input("Substituir por", &self.search.replacement)
                    .on_input(Message::SearchReplacementChanged)
                    .on_submit(Message::ReplaceCurrent)
                    .width(280),
                button("Substituir").on_press(Message::ReplaceCurrent),
                button("Substituir todos").on_press(Message::ReplaceAll),
            ]
            .spacing(6);
            panel = panel.push(replace_row);
        }

        container(panel).width(Fill).padding([4, 8]).into()
    }

    fn request_destructive_action(&mut self, action: DestructiveFileAction) -> Task<Message> {
        if self.file_busy {
            return Task::none();
        }

        let dirty_document = match action {
            DestructiveFileAction::CloseDocument(id) => self
                .document
                .get(id)
                .filter(|document| document.is_dirty())
                .map(|_| id),
            DestructiveFileAction::Close(_) => self
                .document
                .iter()
                .find(|(id, document)| document.is_dirty() && !self.discarded_on_close.contains(id))
                .map(|(id, _)| id),
        };

        if let Some(id) = dirty_document {
            self.activate_document(id);
            self.file_busy = true;
            let name = self
                .document
                .get(id)
                .map(Document::display_name)
                .unwrap_or_else(|| "Documento".to_owned());

            Task::perform(confirm_unsaved(name), move |decision| {
                Message::UnsavedDecision { action, decision }
            })
        } else {
            self.execute_destructive_action(action)
        }
    }

    fn execute_destructive_action(&mut self, action: DestructiveFileAction) -> Task<Message> {
        match action {
            DestructiveFileAction::CloseDocument(id) => {
                self.close_document(id);
                Task::none()
            }
            DestructiveFileAction::Close(id) => self.close_after_session_save(id),
        }
    }

    fn discard_destructive_action(&mut self, action: DestructiveFileAction) -> Task<Message> {
        match action {
            DestructiveFileAction::CloseDocument(id) => {
                self.close_document(id);
                Task::none()
            }
            DestructiveFileAction::Close(window) => {
                self.discarded_on_close.insert(self.document.active_id());
                self.request_destructive_action(DestructiveFileAction::Close(window))
            }
        }
    }

    fn activate_document(&mut self, id: DocumentId) -> bool {
        if self.document.active_id() == id || self.document.get(id).is_none() {
            return false;
        }

        let previous_config = self.compiler_config();
        self.document.clear_search_matches();
        self.document.activate(id);
        self.document_replaced(previous_config);
        true
    }

    fn new_document(&mut self) {
        let previous_config = self.compiler_config();
        self.document.clear_search_matches();
        self.document.add(Document::new());
        self.file_status = Some("Novo documento criado".to_owned());
        self.document_replaced(previous_config);
    }

    fn close_document(&mut self, id: DocumentId) {
        let Some(document) = self.document.get(id) else {
            return;
        };
        let name = document.display_name();
        let was_active = self.document.active_id() == id;
        let previous_config = was_active.then(|| self.compiler_config());

        self.document.remove(id);
        self.discarded_on_close.remove(&id);
        self.file_status = Some(format!("Aba fechada: {name}"));

        if let Some(previous_config) = previous_config {
            self.document_replaced(previous_config);
        } else {
            self.mark_session_changed();
        }
    }

    fn start_open_document(&mut self) -> Task<Message> {
        if self.file_busy {
            return Task::none();
        }

        self.file_busy = true;
        self.file_status = Some("Aguardando a escolha de um arquivo...".to_owned());
        let directory = self.document.directory(&self.workspace_root);

        Task::perform(open_document(directory), Message::OpenFinished)
    }

    fn start_open_project(&mut self) -> Task<Message> {
        if self.file_busy {
            return Task::none();
        }

        self.file_busy = true;
        self.file_status = Some("Aguardando a escolha de uma pasta...".to_owned());
        let directory = self.workspace_root.clone();

        Task::perform(
            choose_project_folder(directory),
            Message::ProjectFolderSelected,
        )
    }

    fn handle_project_folder_selected(&mut self, path: Option<PathBuf>) -> Task<Message> {
        self.file_busy = false;

        let Some(path) = path else {
            self.file_status = Some("A abertura da pasta foi cancelada".to_owned());
            return Task::none();
        };

        let previous_config = self.compiler_config();
        self.workspace_root = path;
        self.project_files.clear();
        self.selected_project_file = None;
        self.project_scan_busy = false;
        self.file_status = Some(format!("Projeto aberto: {}", self.workspace_root.display()));
        self.mark_session_changed();
        self.refresh_compiler_config(previous_config);
        self.schedule_compile(Duration::ZERO, true);

        let scan = self.refresh_project_files();
        self.dispatch_compile(Instant::now());
        scan
    }

    fn open_project_file(&mut self, path: PathBuf) -> Task<Message> {
        if self.file_busy {
            return Task::none();
        }

        self.selected_project_file = Some(path.clone());

        if let Some(id) = self.document.find_path(&path) {
            self.activate_document(id);
            self.file_status = Some(format!("Aba ativada: {}", path.display()));
            return Task::none();
        }

        self.file_busy = true;
        self.file_status = Some(format!("Abrindo {}...", path.display()));
        Task::perform(read_document(path), Message::OpenFinished)
    }

    fn start_create_project_file(&mut self) -> Task<Message> {
        if self.file_busy {
            return Task::none();
        }

        self.file_busy = true;
        self.file_status = Some("Escolha o nome do novo arquivo Typst...".to_owned());
        Task::perform(
            project::create_file(self.workspace_root.clone()),
            Message::ProjectOperationFinished,
        )
    }

    fn start_rename_project_file(&mut self) -> Task<Message> {
        if self.file_busy {
            return Task::none();
        }
        let Some(path) = self.selected_project_file.clone() else {
            self.file_status = Some("Selecione um arquivo do projeto para renomear".to_owned());
            return Task::none();
        };

        self.file_busy = true;
        self.file_status = Some(format!("Renomeando {}...", path.display()));
        Task::perform(
            project::rename_file(self.workspace_root.clone(), path),
            Message::ProjectOperationFinished,
        )
    }

    fn start_delete_project_file(&mut self) -> Task<Message> {
        if self.file_busy {
            return Task::none();
        }
        let Some(path) = self.selected_project_file.clone() else {
            self.file_status = Some("Selecione um arquivo do projeto para excluir".to_owned());
            return Task::none();
        };
        if self
            .document
            .find_path(&path)
            .and_then(|id| self.document.get(id))
            .is_some_and(Document::is_dirty)
        {
            self.file_status =
                Some("Salve ou feche a aba antes de excluir esse arquivo".to_owned());
            return Task::none();
        }

        self.file_busy = true;
        self.file_status = Some(format!("Confirmando exclusão de {}...", path.display()));
        Task::perform(
            project::delete_file(path),
            Message::ProjectOperationFinished,
        )
    }

    fn handle_project_operation(&mut self, outcome: project::OperationOutcome) -> Task<Message> {
        self.file_busy = false;

        match outcome {
            project::OperationOutcome::Cancelled => {
                self.file_status = Some("A operação de projeto foi cancelada".to_owned());
            }
            project::OperationOutcome::Failed(error) => {
                eprintln!("erro em operação de projeto: {error}");
                self.file_status = Some(format!("Erro no projeto: {error}"));
            }
            project::OperationOutcome::Created(path) => {
                let previous_config = self.compiler_config();
                self.selected_project_file = Some(path.clone());
                self.document.clear_search_matches();
                self.document
                    .add(Document::opened(path.clone(), String::new()));
                self.file_status = Some(format!("Arquivo criado: {}", path.display()));
                self.document_replaced(previous_config);
            }
            project::OperationOutcome::Renamed { from, to } => {
                let previous_config = self.compiler_config();
                let renamed = self.document.find_path(&from);
                let active_renamed = renamed == Some(self.document.active_id());

                if let Some(id) = renamed
                    && let Some(document) = self.document.get_mut(id)
                {
                    document.relocate(to.clone());
                }

                self.selected_project_file = Some(to.clone());
                self.file_status = Some(format!(
                    "Arquivo renomeado: {} -> {}",
                    from.display(),
                    to.display()
                ));
                if active_renamed {
                    self.document_replaced(previous_config);
                } else {
                    self.mark_session_changed();
                    self.schedule_compile(Duration::ZERO, true);
                    self.dispatch_compile(Instant::now());
                }
            }
            project::OperationOutcome::Deleted(path) => {
                if let Some(id) = self.document.find_path(&path) {
                    self.close_document(id);
                } else {
                    self.mark_session_changed();
                    self.schedule_compile(Duration::ZERO, true);
                    self.dispatch_compile(Instant::now());
                }
                self.selected_project_file = None;
                self.file_status = Some(format!("Arquivo excluído: {}", path.display()));
            }
        }

        self.refresh_project_files()
    }

    fn open_diagnostic(
        &mut self,
        target: compiler::DiagnosticTarget,
        range: Range<usize>,
    ) -> Task<Message> {
        match target {
            compiler::DiagnosticTarget::Main => {
                self.document.reveal_range(range);
                Task::none()
            }
            compiler::DiagnosticTarget::ProjectFile(path) => {
                if let Some(id) = self.document.find_path(&path) {
                    self.activate_document(id);
                    if let Some(document) = self.document.get_mut(id) {
                        document.reveal_range(range);
                    }
                    Task::none()
                } else if self.file_busy {
                    Task::none()
                } else {
                    self.pending_diagnostic_reveal = Some((path.clone(), range));
                    self.open_project_file(path)
                }
            }
        }
    }

    fn refresh_project_files(&mut self) -> Task<Message> {
        if self.project_scan_busy {
            return Task::none();
        }

        self.project_scan_busy = true;
        Task::perform(
            project::scan(self.workspace_root.clone()),
            Message::ProjectScanned,
        )
    }

    fn handle_watcher_event(&mut self, event: watcher::Event) -> Task<Message> {
        match event {
            watcher::Event::Ready { root } => {
                if root == self.workspace_root {
                    self.watcher_deadline = None;
                }
            }
            watcher::Event::Changed { root, paths } => {
                if root != self.workspace_root || paths.is_empty() {
                    return Task::none();
                }

                self.latest_request_id = None;
                self.preview_status = PreviewStatus::Waiting;
                self.watcher_deadline = Some(Instant::now() + WATCHER_DEBOUNCE);
            }
            watcher::Event::Failed { root, error } => {
                if root != self.workspace_root {
                    return Task::none();
                }

                eprintln!("erro ao observar projeto: {error}");
                self.file_status = Some(format!("Watcher do projeto indisponível: {error}"));
            }
        }

        Task::none()
    }

    fn dispatch_watcher_refresh(&mut self, now: Instant) -> Task<Message> {
        let Some(deadline) = self.watcher_deadline else {
            return Task::none();
        };
        if now < deadline {
            return Task::none();
        }
        if self.file_busy || self.external_check_busy || self.project_scan_busy {
            self.watcher_deadline = Some(now + WATCHER_DEBOUNCE);
            return Task::none();
        }

        self.watcher_deadline = None;
        self.schedule_compile(Duration::ZERO, true);
        self.dispatch_compile(now);

        Task::batch([self.refresh_project_files(), self.check_external_files()])
    }

    fn check_external_files(&mut self) -> Task<Message> {
        if self.file_busy || self.external_check_busy {
            return Task::none();
        }

        let files = self
            .document
            .iter()
            .filter_map(|(id, document)| {
                document
                    .path()
                    .map(|path| (id, document.storage_revision(), path.to_path_buf()))
            })
            .collect::<Vec<_>>();

        if files.is_empty() {
            return Task::none();
        }

        self.external_check_busy = true;
        Task::perform(check_external_files(files), Message::ExternalFilesChecked)
    }

    fn handle_external_files(&mut self, results: Vec<ExternalFileResult>) -> Task<Message> {
        self.external_check_busy = false;

        // A leitura pode ter começado antes de um salvamento ou diálogo. Nesse
        // caso, o snapshot do disco já não é confiável e será consultado de novo.
        if self.file_busy {
            return Task::none();
        }

        let active = self.document.active_id();
        let previous_config = self.compiler_config();
        let mut active_reloaded = false;
        let mut imported_file_reloaded = false;
        let mut status = None;

        for result in results {
            match result {
                ExternalFileResult::Source {
                    document_id,
                    storage_revision,
                    source,
                } => {
                    let Some(document) = self.document.get_mut(document_id) else {
                        continue;
                    };
                    if document.storage_revision() != storage_revision {
                        continue;
                    }

                    match document.observe_disk_source(source) {
                        ExternalUpdate::Unchanged => {}
                        ExternalUpdate::Reloaded => {
                            active_reloaded |= document_id == active;
                            imported_file_reloaded |= document_id != active;
                            status = Some(format!(
                                "Recarregado após alteração externa: {}",
                                document.display_name()
                            ));
                        }
                        ExternalUpdate::Conflict => {
                            status = Some(format!(
                                "Conflito com alteração externa: {}",
                                document.display_name()
                            ));
                        }
                    }
                }
                ExternalFileResult::Deleted {
                    document_id,
                    storage_revision,
                } => {
                    let Some(document) = self.document.get_mut(document_id) else {
                        continue;
                    };
                    if document.storage_revision() != storage_revision {
                        continue;
                    }

                    if document.observe_deleted_file() == ExternalUpdate::Conflict {
                        status = Some(format!(
                            "Arquivo removido externamente: {}",
                            document.display_name()
                        ));
                    }
                }
                ExternalFileResult::Failed {
                    document_id,
                    storage_revision,
                    error,
                } => {
                    if self
                        .document
                        .get(document_id)
                        .is_some_and(|document| document.storage_revision() == storage_revision)
                    {
                        eprintln!("erro ao verificar alteração externa: {error}");
                        status = Some(format!("Erro ao verificar arquivo: {error}"));
                    }
                }
            }
        }

        if let Some(status) = status {
            self.file_status = Some(status);
        }

        if active_reloaded {
            self.document_replaced(previous_config);
        } else if imported_file_reloaded {
            self.apply_editor_settings();
            self.mark_session_changed();
            self.schedule_compile(Duration::ZERO, true);
            self.dispatch_compile(Instant::now());
        }

        Task::none()
    }

    fn reload_external_change(&mut self) {
        let previous_config = self.compiler_config();

        if self.document.reload_external_change() {
            self.file_status = Some("A versão externa foi recarregada".to_owned());
            self.document_replaced(previous_config);
        }
    }

    fn mark_session_changed(&mut self) {
        if self.session.path.is_none() {
            return;
        }

        self.session.revision += 1;
        self.session.deadline = Some(if self.session.close_after_write.is_some() {
            Instant::now()
        } else {
            Instant::now() + SESSION_DEBOUNCE
        });
    }

    fn close_after_session_save(&mut self, window: window::Id) -> Task<Message> {
        if self.session.path.is_none() {
            self.discarded_on_close.clear();
            return window::close(window);
        }

        self.file_busy = true;
        self.session.close_after_write = Some(window);
        self.session.revision += 1;
        self.session.deadline = Some(Instant::now());
        self.dispatch_session_save(Instant::now())
    }

    fn dispatch_session_save(&mut self, now: Instant) -> Task<Message> {
        if self.session.write_busy {
            return Task::none();
        }

        let Some(deadline) = self.session.deadline else {
            return Task::none();
        };
        if now < deadline {
            return Task::none();
        }

        let Some(path) = self.session.path.clone() else {
            self.session.deadline = None;
            return Task::none();
        };
        let revision = self.session.revision;
        let stored = self.session_snapshot(self.session.close_after_write.is_some());

        self.session.deadline = None;
        self.session.write_busy = true;

        Task::perform(session::save(path, stored), move |result| {
            Message::SessionWriteFinished(SessionWriteOutcome { revision, result })
        })
    }

    fn handle_session_write_finished(&mut self, outcome: SessionWriteOutcome) -> Task<Message> {
        self.session.write_busy = false;

        if let Err(error) = outcome.result {
            eprintln!("erro ao salvar sessão: {error}");
            self.file_status = Some(format!("Erro ao salvar sessão: {error}"));

            if let Some(window) = self.session.close_after_write.take() {
                self.file_busy = false;
                self.discarded_on_close.clear();
                return window::close(window);
            }

            return Task::none();
        }

        if let Some(window) = self.session.close_after_write {
            if outcome.revision == self.session.revision {
                self.session.close_after_write = None;
                self.file_busy = false;
                self.discarded_on_close.clear();
                return window::close(window);
            }

            self.session.deadline = Some(Instant::now());
            return self.dispatch_session_save(Instant::now());
        }

        self.dispatch_session_save(Instant::now())
    }

    fn session_snapshot(&self, closing: bool) -> session::Session {
        let active = self.document.active_id();
        let mut active_document = None;
        let mut documents = Vec::new();

        for (id, document) in self.document.iter() {
            let stored = if closing && self.discarded_on_close.contains(&id) {
                match (document.path(), document.saved_text()) {
                    (Some(path), Some(saved_text)) => session::Document {
                        path: Some(path.to_path_buf()),
                        text: saved_text.to_owned(),
                        saved_text: Some(saved_text.to_owned()),
                    },
                    _ => continue,
                }
            } else {
                session::Document {
                    path: document.path().map(Path::to_path_buf),
                    text: document.snapshot().1,
                    saved_text: document.saved_text().map(str::to_owned),
                }
            };

            if id == active {
                active_document = Some(documents.len());
            }
            documents.push(stored);
        }

        if documents.is_empty() {
            documents.push(session::Document::blank());
        }

        session::Session::new(
            self.workspace_root.clone(),
            active_document.unwrap_or(0),
            documents,
            self.pane_layout(),
            self.settings,
        )
    }

    fn pane_layout(&self) -> session::PaneLayout {
        let pane_grid::Node::Split { axis, ratio, a, .. } = self.panes.layout() else {
            return session::PaneLayout::default();
        };
        let first = first_pane(a)
            .and_then(|pane| self.panes.get(pane))
            .copied()
            .unwrap_or(Pane::Editor);

        session::PaneLayout {
            axis: match axis {
                pane_grid::Axis::Horizontal => session::Axis::Horizontal,
                pane_grid::Axis::Vertical => session::Axis::Vertical,
            },
            ratio: *ratio,
            first: match first {
                Pane::Editor => session::Pane::Editor,
                Pane::Preview => session::Pane::Preview,
            },
        }
    }

    fn start_save(&mut self, save_as: bool) -> Task<Message> {
        if self.file_busy {
            return Task::none();
        }

        self.file_busy = true;
        self.file_status = Some("Salvando documento...".to_owned());

        let document_id = self.document.active_id();
        let (_, source) = self.document.snapshot();

        if !save_as && let Some(path) = self.document.path() {
            let path = path.to_path_buf();
            return Task::perform(
                write_document(document_id, path, source),
                Message::SaveFinished,
            );
        }

        let directory = self.document.directory(&self.workspace_root);
        let file_name = self.document.display_name();

        Task::perform(
            save_document_as(document_id, directory, file_name, source),
            Message::SaveFinished,
        )
    }

    fn start_pdf_export(&mut self) -> Task<Message> {
        if self.file_busy || self.compiler.is_none() {
            return Task::none();
        }

        self.file_busy = true;
        self.file_status = Some("Aguardando o destino do PDF...".to_owned());
        let directory = self.document.directory(&self.workspace_root);
        let file_name = pdf_file_name(&self.document.display_name());

        Task::perform(
            choose_pdf_path(directory, file_name),
            Message::PdfPathSelected,
        )
    }

    fn handle_pdf_path_selected(&mut self, outcome: PdfPathOutcome) -> Task<Message> {
        let PdfPathOutcome::Selected(path) = outcome else {
            self.file_busy = false;
            self.file_status = Some("A exportação de PDF foi cancelada".to_owned());
            return Task::none();
        };
        let Some(sender) = self.compiler.clone() else {
            self.file_busy = false;
            self.file_status = Some("O worker de compilação não está disponível".to_owned());
            return Task::none();
        };
        let (revision, source) = self.document.snapshot();

        self.next_request_id += 1;
        let request_id = self.next_request_id;
        let request = compiler::Request {
            id: request_id,
            revision,
            source,
            overlays: self.source_overlays(),
            reset_files: true,
            purpose: compiler::Purpose::ExportPdf,
        };

        if sender.unbounded_send(request).is_err() {
            self.compiler = None;
            self.file_busy = false;
            self.file_status = Some("O worker de compilação foi encerrado".to_owned());
            return Task::none();
        }

        self.pending_pdf_export = Some(PendingPdfExport {
            request_id,
            revision,
            path,
        });
        self.file_status = Some("Gerando PDF...".to_owned());
        Task::none()
    }

    fn handle_pdf_write_finished(&mut self, outcome: PdfWriteOutcome) -> Task<Message> {
        self.file_busy = false;

        match outcome {
            PdfWriteOutcome::Saved(path) => {
                self.file_status = Some(format!("PDF exportado: {}", path.display()));
            }
            PdfWriteOutcome::Failed(error) => {
                eprintln!("erro ao exportar PDF: {error}");
                self.file_status = Some(format!("Erro ao exportar PDF: {error}"));
            }
        }

        Task::none()
    }

    fn handle_open_finished(&mut self, outcome: OpenOutcome) -> Task<Message> {
        self.file_busy = false;

        match outcome {
            OpenOutcome::Cancelled => {
                self.pending_diagnostic_reveal = None;
                self.file_status = Some("A abertura foi cancelada".to_owned());
            }
            OpenOutcome::Failed(error) => {
                self.pending_diagnostic_reveal = None;
                eprintln!("erro ao abrir documento: {error}");
                self.file_status = Some(format!("Erro ao abrir: {error}"));
            }
            OpenOutcome::Loaded { path, source } => {
                if let Some(id) = self.document.find_path(&path) {
                    self.activate_document(id);
                    self.file_status = Some(format!("Aba ativada: {}", path.display()));
                } else {
                    let previous_config = self.compiler_config();
                    self.document.clear_search_matches();
                    self.document.add(Document::opened(path.clone(), source));
                    self.file_status = Some(format!("Aberto: {}", path.display()));
                    self.document_replaced(previous_config);
                }

                if let Some((reveal_path, range)) = self.pending_diagnostic_reveal.take()
                    && reveal_path == path
                    && let Some(id) = self.document.find_path(&path)
                    && let Some(document) = self.document.get_mut(id)
                {
                    document.reveal_range(range);
                }
            }
        }

        Task::none()
    }

    fn handle_save_finished(&mut self, outcome: SaveOutcome) -> Task<Message> {
        self.file_busy = false;

        match outcome {
            SaveOutcome::Cancelled { document_id } => {
                if matches!(
                    self.pending_after_save,
                    Some(DestructiveFileAction::Close(_))
                ) {
                    self.discarded_on_close.clear();
                }
                self.pending_after_save = None;
                let name = self
                    .document
                    .get(document_id)
                    .map(Document::display_name)
                    .unwrap_or_else(|| "documento".to_owned());
                self.file_status = Some(format!("O salvamento de {name} foi cancelado"));
                Task::none()
            }
            SaveOutcome::Failed { document_id, error } => {
                if matches!(
                    self.pending_after_save,
                    Some(DestructiveFileAction::Close(_))
                ) {
                    self.discarded_on_close.clear();
                }
                self.pending_after_save = None;
                eprintln!("erro ao salvar documento: {error}");
                let name = self
                    .document
                    .get(document_id)
                    .map(Document::display_name)
                    .unwrap_or_else(|| "documento".to_owned());
                self.file_status = Some(format!("Erro ao salvar {name}: {error}"));
                Task::none()
            }
            SaveOutcome::Saved {
                document_id,
                path,
                source,
            } => {
                let is_active = self.document.active_id() == document_id;
                let previous_config = is_active.then(|| self.compiler_config());
                let Some(document) = self.document.get_mut(document_id) else {
                    self.pending_after_save = None;
                    return Task::none();
                };

                document.mark_saved(path.clone(), source);
                let still_dirty = document.is_dirty();
                self.mark_session_changed();

                self.file_status = Some(if still_dirty {
                    format!(
                        "Versão salva em {}; há alterações mais recentes",
                        path.display()
                    )
                } else {
                    format!("Salvo em {}", path.display())
                });

                if let Some(previous_config) = previous_config {
                    self.refresh_compiler_config(previous_config);
                }

                if let Some(action) = self.pending_after_save.take() {
                    self.request_destructive_action(action)
                } else {
                    Task::none()
                }
            }
        }
    }

    fn document_replaced(&mut self, previous_config: compiler::Config) {
        self.search.visible = false;
        self.clear_compile_diagnostics();
        self.apply_editor_settings();
        self.replace_editor_pane_identity();
        self.preview.clear();
        self.latest_request_id = None;
        self.mark_session_changed();
        self.refresh_compiler_config(previous_config);
        self.schedule_compile(Duration::ZERO, true);
        self.dispatch_compile(Instant::now());
    }

    fn replace_editor_pane_identity(&mut self) {
        let Some(editor) = self
            .panes
            .iter()
            .find_map(|(id, pane)| matches!(pane, Pane::Editor).then_some(*id))
        else {
            return;
        };

        // PaneGrid owns the CodeEditor widget tree, including its line cache.
        // Replacing the pane ID makes Iced create a fresh tree for new Content.
        let Some((replacement, _split)) =
            self.panes
                .split(pane_grid::Axis::Vertical, editor, Pane::Editor)
        else {
            return;
        };

        let removed = self.panes.close(editor);
        debug_assert!(removed.is_some_and(|(_, sibling)| sibling == replacement));
    }

    fn refresh_compiler_config(&mut self, previous_config: compiler::Config) {
        if previous_config != self.compiler_config() {
            self.compiler = None;
            self.latest_request_id = None;
            self.schedule_compile(Duration::ZERO, true);
        }
    }

    fn compiler_config(&self) -> compiler::Config {
        let (root, main_name) = self.document.compiler_location(&self.workspace_root);
        compiler::Config::new(root, main_name)
    }

    fn apply_editor_settings(&mut self) {
        for (_id, document) in self.document.iter_mut() {
            document.configure_editor(
                self.settings.tab_width,
                self.settings.auto_pairs,
                self.settings.auto_indent,
            );
        }
    }

    fn settings_changed(&mut self, apply_to_content: bool) {
        if apply_to_content {
            self.apply_editor_settings();
        }
        self.mark_session_changed();
    }

    fn change_preview_zoom(&mut self, delta: i16) {
        let zoom = i32::from(self.settings.preview_zoom) + i32::from(delta);
        self.settings.preview_zoom = zoom.clamp(25, 300) as u16;
        self.settings_changed(false);
    }

    fn source_overlays(&self) -> Vec<SourceOverlay> {
        let active = self.document.active_id();
        let config = self.compiler_config();

        self.document
            .iter()
            .filter(|(id, document)| *id != active && document.is_dirty())
            .filter_map(|(_id, document)| {
                let path = document.path()?.to_path_buf();
                path.starts_with(config.root()).then(|| SourceOverlay {
                    path,
                    text: document.snapshot().1,
                })
            })
            .collect()
    }

    fn after_formatting(&mut self) {
        self.clear_compile_diagnostics();
        self.file_status = None;
        self.mark_session_changed();
        self.schedule_compile(Duration::ZERO, false);
        self.dispatch_compile(Instant::now());
        self.refresh_search_matches(None, false);
    }

    fn clear_compile_diagnostics(&mut self) {
        self.diagnostics.clear();
        for (_id, document) in self.document.iter_mut() {
            document.clear_diagnostics();
        }
    }

    fn install_diagnostics(&mut self, diagnostics: Vec<compiler::ReportedDiagnostic>) {
        let active = self.document.active_id();

        for (id, document) in self.document.iter_mut() {
            let editor_diagnostics = diagnostics
                .iter()
                .filter(|diagnostic| match &diagnostic.target {
                    compiler::DiagnosticTarget::Main => id == active,
                    compiler::DiagnosticTarget::ProjectFile(path) => {
                        document.path() == Some(path.as_path())
                    }
                })
                .map(compiler::ReportedDiagnostic::editor_diagnostic)
                .collect::<Vec<_>>();
            document.set_diagnostics(editor_diagnostics);
        }

        self.diagnostics = diagnostics;
    }

    fn open_search(&mut self, replace_visible: bool) -> Task<Message> {
        if let Some(selection) = self.document.selection_text()
            && !selection.is_empty()
            && !selection.contains(['\n', '\r'])
        {
            self.search.query = selection;
        }

        self.search.visible = true;
        self.search.replace_visible = replace_visible;
        self.refresh_search_matches(Some(0), true);
        operation::focus(self.search_input_id.clone())
    }

    fn refresh_search_matches(&mut self, preferred: Option<usize>, reveal: bool) {
        if !self.search.visible {
            return;
        }

        let previous = self.document.current_search_match();
        let matches = {
            let buffer = self.document.content().buffer();
            search::find_matches(
                buffer.text(),
                &self.search.query,
                search::Options {
                    case_sensitive: self.search.case_sensitive,
                    whole_word: self.search.whole_word,
                },
            )
        };
        let current = (!matches.is_empty())
            .then(|| preferred.or(previous).unwrap_or(0).min(matches.len() - 1));

        self.document.set_search_matches(matches, current);

        if reveal && let Some(current) = current {
            self.document.reveal_search_match(current);
        }
    }

    fn move_search_match(&mut self, reverse: bool) {
        let count = self.document.search_matches().len();

        if count == 0 {
            return;
        }

        let current = self.document.current_search_match();
        let next = match (current, reverse) {
            (Some(0), true) | (None, true) => count - 1,
            (Some(index), true) => index - 1,
            (Some(index), false) => (index + 1) % count,
            (None, false) => 0,
        };

        self.document.reveal_search_match(next);
    }

    fn replace_current_match(&mut self) {
        if self.file_busy {
            return;
        }

        let matches = self.document.search_matches();
        let Some(index) = self.document.current_search_match() else {
            return;
        };
        let Some(range) = matches.get(index).cloned() else {
            return;
        };
        let resume_at = range.start + self.search.replacement.len();

        self.document.perform(Action::Replace {
            range,
            text: self.search.replacement.clone(),
        });
        self.after_formatting();

        let matches = self.document.search_matches();
        if let Some(next) = matches
            .iter()
            .position(|range| range.start >= resume_at)
            .or_else(|| (!matches.is_empty()).then_some(0))
        {
            self.document.reveal_search_match(next);
        }
    }

    fn replace_all_matches(&mut self) {
        if self.file_busy {
            return;
        }

        let matches = self.document.search_matches();

        if matches.is_empty() {
            return;
        }

        let count = matches.len();
        let replacement = self.search.replacement.clone();
        let edits = matches
            .into_iter()
            .map(|range| (range, replacement.clone()))
            .collect();

        self.document.perform(Action::ApplyEdits(edits));
        self.after_formatting();
        self.file_status = Some(format!("{count} ocorrência(s) substituída(s)"));
    }

    fn schedule_compile(&mut self, delay: Duration, reset_files: bool) {
        let reset_files = self
            .pending_compile
            .is_some_and(|pending| pending.reset_files)
            || reset_files;

        self.pending_compile = Some(PendingCompile {
            deadline: Instant::now() + delay,
            reset_files,
        });
        self.preview_status = PreviewStatus::Waiting;
    }

    fn dispatch_compile(&mut self, now: Instant) {
        let Some(pending) = self.pending_compile else {
            return;
        };

        if now < pending.deadline {
            return;
        }

        let Some(sender) = self.compiler.clone() else {
            return;
        };

        let (revision, source) = self.document.snapshot();

        self.next_request_id += 1;
        let request_id = self.next_request_id;
        let request = compiler::Request {
            id: request_id,
            revision,
            source,
            overlays: self.source_overlays(),
            reset_files: pending.reset_files,
            purpose: compiler::Purpose::Preview,
        };

        if sender.unbounded_send(request).is_ok() {
            self.pending_compile = None;
            self.latest_request_id = Some(request_id);
            self.preview_status = PreviewStatus::Compiling;
        } else {
            self.compiler = None;
            self.preview_status = PreviewStatus::Failed {
                errors: 1,
                summary: "O worker de compilação foi encerrado".to_owned(),
            };
        }
    }

    fn handle_compiler_event(&mut self, event: compiler::Event) -> Task<Message> {
        match event {
            compiler::Event::Ready { config, sender } => {
                if config != self.compiler_config() {
                    return Task::none();
                }

                self.compiler = Some(sender);
                self.dispatch_compile(Instant::now());
                Task::none()
            }
            compiler::Event::Finished { config, output } => {
                if config != self.compiler_config() {
                    return Task::none();
                }

                match output.purpose {
                    compiler::Purpose::Preview => self.handle_preview_output(output),
                    compiler::Purpose::ExportPdf => self.handle_pdf_output(output),
                }
            }
        }
    }

    fn handle_preview_output(&mut self, output: compiler::Output) -> Task<Message> {
        let current_revision = self.document.revision();

        if self.latest_request_id != Some(output.id) || current_revision != output.revision {
            return Task::none();
        }

        self.install_diagnostics(output.diagnostics);

        if output.error_count > 0 {
            self.preview_status = PreviewStatus::Failed {
                errors: output.error_count,
                summary: output
                    .summary
                    .unwrap_or_else(|| "Falha ao compilar o documento".to_owned()),
            };
            return Task::none();
        }

        if output.pages.is_empty() {
            self.preview_status = PreviewStatus::Failed {
                errors: 1,
                summary: "A compilação não produziu um preview".to_owned(),
            };
            return Task::none();
        }

        self.preview = output
            .pages
            .into_iter()
            .map(|page| PreviewPage {
                handle: svg::Handle::from_memory(page.svg),
                width: page.width,
                height: page.height,
            })
            .collect();
        self.preview_status = PreviewStatus::Ready {
            pages: output.page_count,
            warnings: output.warning_count,
        };
        Task::none()
    }

    fn handle_pdf_output(&mut self, output: compiler::Output) -> Task<Message> {
        let Some(pending) = self.pending_pdf_export.take() else {
            return Task::none();
        };

        if pending.request_id != output.id || pending.revision != output.revision {
            self.pending_pdf_export = Some(pending);
            return Task::none();
        }

        if self.document.revision() == output.revision {
            self.install_diagnostics(output.diagnostics);
        }

        if output.error_count > 0 {
            self.file_busy = false;
            self.file_status = Some(format!(
                "Falha ao gerar PDF: {}",
                output
                    .summary
                    .unwrap_or_else(|| format!("{} erro(s)", output.error_count))
            ));
            return Task::none();
        }

        let Some(pdf) = output.pdf else {
            self.file_busy = false;
            self.file_status = Some("A compilação não produziu um PDF".to_owned());
            return Task::none();
        };

        self.file_status = Some("Gravando PDF...".to_owned());
        Task::perform(write_pdf(pending.path, pdf), Message::PdfWriteFinished)
    }

    fn status_text(&self) -> String {
        let preview = match &self.preview_status {
            PreviewStatus::Waiting => "Alterações pendentes; preview desatualizado".to_owned(),
            PreviewStatus::Compiling => "Compilando preview...".to_owned(),
            PreviewStatus::Ready { pages, warnings } => {
                let page_label = if *pages == 1 { "página" } else { "páginas" };
                let warning_label = if *warnings == 1 { "aviso" } else { "avisos" };

                format!("Preview atualizado: {pages} {page_label}, {warnings} {warning_label}")
            }
            PreviewStatus::Failed { errors, summary } => {
                let error_label = if *errors == 1 { "erro" } else { "erros" };
                format!(
                    "Preview desatualizado: {errors} {error_label} - {}",
                    truncate(summary, 100)
                )
            }
        };

        match &self.file_status {
            Some(file) => format!("{} | {preview}", truncate(file, 100)),
            None => preview,
        }
    }
}

fn panes_from_layout(layout: session::PaneLayout) -> pane_grid::State<Pane> {
    let first = match layout.first {
        session::Pane::Editor => Pane::Editor,
        session::Pane::Preview => Pane::Preview,
    };
    let second = match first {
        Pane::Editor => Pane::Preview,
        Pane::Preview => Pane::Editor,
    };
    let axis = match layout.axis {
        session::Axis::Horizontal => pane_grid::Axis::Horizontal,
        session::Axis::Vertical => pane_grid::Axis::Vertical,
    };

    pane_grid::State::with_configuration(pane_grid::Configuration::Split {
        axis,
        ratio: layout.ratio.clamp(0.1, 0.9),
        a: Box::new(pane_grid::Configuration::Pane(first)),
        b: Box::new(pane_grid::Configuration::Pane(second)),
    })
}

fn first_pane(node: &pane_grid::Node) -> Option<pane_grid::Pane> {
    match node {
        pane_grid::Node::Pane(pane) => Some(*pane),
        pane_grid::Node::Split { a, .. } => first_pane(a),
    }
}

fn file_button<'a>(
    label: &'a str,
    message: Message,
    enabled: bool,
) -> iced::widget::Button<'a, Message> {
    let button = button(label);

    if enabled {
        button.on_press(message)
    } else {
        button
    }
}

fn command_button<'a>(
    symbol: &'a str,
    description: &'a str,
    action: Action,
    enabled: bool,
) -> Element<'a, Message> {
    let button = button(text(symbol).width(24).align_x(Alignment::Center));
    let button = if enabled {
        button.on_press(Message::Editor(action))
    } else {
        button
    };

    tooltip(
        button,
        container(text(description).size(12)).padding([4, 6]),
        tooltip::Position::Bottom,
    )
    .into()
}

async fn confirm_unsaved(name: String) -> UnsavedDecision {
    let result = AsyncMessageDialog::new()
        .set_level(MessageLevel::Warning)
        .set_title("Alterações não salvas")
        .set_description(format!(
            "{name} possui alterações não salvas. Deseja salvá-las antes de continuar?"
        ))
        .set_buttons(MessageButtons::YesNoCancel)
        .show()
        .await;

    match result {
        MessageDialogResult::Yes => UnsavedDecision::Save,
        MessageDialogResult::No => UnsavedDecision::Discard,
        MessageDialogResult::Ok | MessageDialogResult::Cancel | MessageDialogResult::Custom(_) => {
            UnsavedDecision::Cancel
        }
    }
}

async fn open_document(directory: PathBuf) -> OpenOutcome {
    let Some(file) = AsyncFileDialog::new()
        .add_filter("Documento Typst", &["typ"])
        .set_directory(directory)
        .set_title("Abrir documento Typst")
        .pick_file()
        .await
    else {
        return OpenOutcome::Cancelled;
    };

    let path = file.path().to_path_buf();

    read_document(path).await
}

async fn read_document(path: PathBuf) -> OpenOutcome {
    match tokio::fs::read_to_string(&path).await {
        Ok(source) => OpenOutcome::Loaded { path, source },
        Err(error) => OpenOutcome::Failed(format!("{}: {error}", path.display())),
    }
}

async fn choose_project_folder(directory: PathBuf) -> Option<PathBuf> {
    AsyncFileDialog::new()
        .set_directory(directory)
        .set_title("Abrir pasta de projeto")
        .pick_folder()
        .await
        .map(|folder| folder.path().to_path_buf())
}

async fn check_external_files(files: Vec<(DocumentId, u64, PathBuf)>) -> Vec<ExternalFileResult> {
    let mut results = Vec::with_capacity(files.len());

    for (document_id, storage_revision, path) in files {
        match tokio::fs::read_to_string(&path).await {
            Ok(source) => results.push(ExternalFileResult::Source {
                document_id,
                storage_revision,
                source,
            }),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                results.push(ExternalFileResult::Deleted {
                    document_id,
                    storage_revision,
                });
            }
            Err(error) => results.push(ExternalFileResult::Failed {
                document_id,
                storage_revision,
                error: format!("{}: {error}", path.display()),
            }),
        }
    }

    results
}

async fn save_document_as(
    document_id: DocumentId,
    directory: PathBuf,
    file_name: String,
    source: String,
) -> SaveOutcome {
    let Some(file) = AsyncFileDialog::new()
        .add_filter("Documento Typst", &["typ"])
        .set_directory(directory)
        .set_file_name(file_name)
        .set_title("Salvar documento Typst")
        .save_file()
        .await
    else {
        return SaveOutcome::Cancelled { document_id };
    };

    write_document(document_id, with_typst_extension(file.path()), source).await
}

async fn choose_pdf_path(directory: PathBuf, file_name: String) -> PdfPathOutcome {
    let Some(file) = AsyncFileDialog::new()
        .add_filter("Documento PDF", &["pdf"])
        .set_directory(directory)
        .set_file_name(file_name)
        .set_title("Exportar documento como PDF")
        .save_file()
        .await
    else {
        return PdfPathOutcome::Cancelled;
    };

    PdfPathOutcome::Selected(with_pdf_extension(file.path()))
}

async fn write_document(document_id: DocumentId, path: PathBuf, source: String) -> SaveOutcome {
    let destination = path.clone();
    let result = tokio::task::spawn_blocking(move || {
        atomic_write_file(&destination, source.as_bytes())?;
        Ok::<_, io::Error>(source)
    })
    .await;

    match result {
        Ok(Ok(source)) => SaveOutcome::Saved {
            document_id,
            path,
            source,
        },
        Ok(Err(error)) => SaveOutcome::Failed {
            document_id,
            error: format!("{}: {error}", path.display()),
        },
        Err(error) => SaveOutcome::Failed {
            document_id,
            error: format!(
                "{}: tarefa de salvamento interrompida: {error}",
                path.display()
            ),
        },
    }
}

async fn write_pdf(path: PathBuf, pdf: Vec<u8>) -> PdfWriteOutcome {
    let destination = path.clone();
    let result = tokio::task::spawn_blocking(move || atomic_write_file(&destination, &pdf)).await;

    match result {
        Ok(Ok(())) => PdfWriteOutcome::Saved(path),
        Ok(Err(error)) => PdfWriteOutcome::Failed(format!("{}: {error}", path.display())),
        Err(error) => PdfWriteOutcome::Failed(format!(
            "{}: tarefa de exportação interrompida: {error}",
            path.display()
        )),
    }
}

fn atomic_write_file(path: &Path, contents: &[u8]) -> io::Result<()> {
    atomic_write_file_with_mode(path, contents, None)
}

fn atomic_write_private_file(path: &Path, contents: &[u8]) -> io::Result<()> {
    atomic_write_file_with_mode(path, contents, Some(0o600))
}

fn atomic_write_file_with_mode(
    path: &Path,
    contents: &[u8],
    forced_mode: Option<u32>,
) -> io::Result<()> {
    let destination = match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => fs::canonicalize(path)?,
        Ok(_) => path.to_path_buf(),
        Err(error) if error.kind() == io::ErrorKind::NotFound => path.to_path_buf(),
        Err(error) => return Err(error),
    };
    let directory = destination
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let permissions = match fs::metadata(&destination) {
        Ok(metadata) => Some(metadata.permissions()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        Err(error) => return Err(error),
    };

    if permissions.as_ref().is_some_and(fs::Permissions::readonly) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "o arquivo de destino é somente leitura",
        ));
    }

    let mut builder = tempfile::Builder::new();
    builder.prefix(".typstation-").suffix(".tmp");

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        builder.permissions(forced_mode.map_or_else(
            || {
                permissions
                    .clone()
                    .unwrap_or_else(|| fs::Permissions::from_mode(0o666))
            },
            fs::Permissions::from_mode,
        ));
    }

    #[cfg(not(unix))]
    let _ = forced_mode;

    let mut temporary = builder.tempfile_in(directory)?;

    #[cfg(not(unix))]
    if let Some(permissions) = permissions {
        temporary.as_file().set_permissions(permissions)?;
    }

    temporary.write_all(contents)?;
    temporary.as_file().sync_all()?;
    temporary
        .persist(destination)
        .map_err(|error| error.error)?;

    #[cfg(unix)]
    if let Some(mode) = forced_mode {
        use std::os::unix::fs::PermissionsExt;

        fs::set_permissions(path, fs::Permissions::from_mode(mode))?;
    }

    Ok(())
}

fn shortcut_subscription() -> Subscription<Message> {
    event::listen_with(|event, _status, window| {
        let iced::Event::Keyboard(keyboard::Event::KeyPressed {
            key,
            physical_key,
            modifiers,
            repeat,
            ..
        }) = event
        else {
            return None;
        };

        if repeat {
            return None;
        }

        match key.as_ref() {
            keyboard::Key::Named(keyboard::key::Named::F3) => {
                return Some(if modifiers.shift() {
                    Message::SearchPrevious
                } else {
                    Message::SearchNext
                });
            }
            keyboard::Key::Named(keyboard::key::Named::Escape) => {
                return Some(Message::CloseSearch);
            }
            _ => {}
        }

        shortcut_message(key.to_latin(physical_key)?, modifiers, window)
    })
}

fn shortcut_message(
    key: char,
    modifiers: keyboard::Modifiers,
    window: window::Id,
) -> Option<Message> {
    if !modifiers.command() || modifiers.alt() {
        return None;
    }

    match (key.to_ascii_lowercase(), modifiers.shift()) {
        ('+' | '=', _) => Some(Message::PreviewZoomIn),
        ('-', _) => Some(Message::PreviewZoomOut),
        ('0', _) => Some(Message::PreviewZoomReset),
        ('n', false) => Some(Message::NewDocument),
        ('o', false) => Some(Message::OpenDocument),
        ('s', false) => Some(Message::SaveDocument),
        ('s', true) => Some(Message::SaveDocumentAs),
        ('q', false) => Some(Message::CloseRequested(window)),
        ('b', false) => Some(Message::Bold),
        ('i', false) => Some(Message::Italic),
        ('u', false) => Some(Message::Underline),
        ('f', false) => Some(Message::OpenSearch),
        ('h', false) => Some(Message::OpenReplace),
        _ => None,
    }
}

fn with_typst_extension(path: &Path) -> PathBuf {
    let mut path = path.to_path_buf();

    if path
        .extension()
        .is_none_or(|extension| extension.is_empty())
    {
        path.set_extension("typ");
    }

    path
}

fn with_pdf_extension(path: &Path) -> PathBuf {
    let mut path = path.to_path_buf();

    if path
        .extension()
        .is_none_or(|extension| extension.is_empty())
    {
        path.set_extension("pdf");
    }

    path
}

fn pdf_file_name(document_name: &str) -> String {
    let mut path = PathBuf::from(document_name);
    path.set_extension("pdf");
    path.to_string_lossy().into_owned()
}

fn truncate(text: &str, limit: usize) -> String {
    let mut characters = text.chars();
    let shortened = characters.by_ref().take(limit).collect::<String>();

    if characters.next().is_some() {
        format!("{shortened}...")
    } else {
        shortened
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_as_adds_the_typst_extension_when_missing() {
        assert_eq!(
            with_typst_extension(Path::new("documento")),
            PathBuf::from("documento.typ")
        );
        assert_eq!(
            with_typst_extension(Path::new("documento.typ")),
            PathBuf::from("documento.typ")
        );
        assert_eq!(
            with_pdf_extension(Path::new("documento")),
            PathBuf::from("documento.pdf")
        );
        assert_eq!(pdf_file_name("documento.typ"), "documento.pdf");
    }

    #[test]
    fn new_document_opens_another_tab_and_replaces_the_editor_pane() {
        let mut app = App::new();
        let first = app.document.active_id();
        let previous_editor = app
            .panes
            .iter()
            .find_map(|(id, pane)| matches!(pane, Pane::Editor).then_some(*id))
            .expect("the editor pane exists");

        app.new_document();

        let (_, source) = app.document.snapshot();
        let current_editor = app
            .panes
            .iter()
            .find_map(|(id, pane)| matches!(pane, Pane::Editor).then_some(*id))
            .expect("the replacement editor pane exists");

        assert!(source.is_empty());
        assert_ne!(app.document.active_id(), first);
        assert!(app.document.get(first).is_some_and(Document::is_dirty));
        assert_eq!(app.document.iter().count(), 2);
        assert_ne!(current_editor, previous_editor);
        assert_eq!(app.panes.len(), 2);
    }

    #[test]
    fn closing_a_dirty_document_starts_confirmation() {
        let mut app = App::new();

        let _ = app.update(Message::CloseRequested(window::Id::unique()));

        assert!(app.document.is_dirty());
        assert!(app.file_busy);
        assert!(app.pending_after_save.is_none());
    }

    #[test]
    fn saving_before_close_keeps_the_close_action_pending() {
        let mut app = App::new();
        let id = window::Id::unique();
        app.file_busy = true;

        let _ = app.update(Message::UnsavedDecision {
            action: DestructiveFileAction::Close(id),
            decision: UnsavedDecision::Save,
        });

        assert!(matches!(
            app.pending_after_save,
            Some(DestructiveFileAction::Close(pending_id)) if pending_id == id
        ));
        assert!(app.file_busy);
    }

    #[test]
    fn discarding_a_dirty_tab_closes_only_that_tab() {
        let mut app = App::new();
        let first = app.document.active_id();
        app.new_document();
        let second = app.document.active_id();
        app.document.perform(Action::Insert("rascunho".to_owned()));
        app.file_busy = true;

        let _ = app.update(Message::UnsavedDecision {
            action: DestructiveFileAction::CloseDocument(second),
            decision: UnsavedDecision::Discard,
        });

        assert_eq!(app.document.iter().count(), 1);
        assert_eq!(app.document.active_id(), first);
        assert!(app.document.get(second).is_none());
    }

    #[test]
    fn window_close_walks_through_every_dirty_tab() {
        let mut app = App::new();
        let first = app.document.active_id();
        app.new_document();
        let second = app.document.active_id();
        app.document.perform(Action::Insert("segundo".to_owned()));
        let window = window::Id::unique();
        app.file_busy = true;

        let _ = app.update(Message::UnsavedDecision {
            action: DestructiveFileAction::Close(window),
            decision: UnsavedDecision::Discard,
        });

        assert!(app.discarded_on_close.contains(&second));
        assert_eq!(app.document.active_id(), first);
        assert!(app.file_busy);

        let _ = app.update(Message::UnsavedDecision {
            action: DestructiveFileAction::Close(window),
            decision: UnsavedDecision::Discard,
        });

        assert!(app.discarded_on_close.is_empty());
        assert!(!app.file_busy);
    }

    #[test]
    fn file_operation_blocks_tab_changes_and_preserves_a_pending_close() {
        let mut app = App::new();
        let first = app.document.active_id();
        app.new_document();
        let second = app.document.active_id();
        assert!(app.activate_document(first));
        let window = window::Id::unique();
        app.pending_after_save = Some(DestructiveFileAction::Close(window));
        app.file_busy = true;

        let _ = app.update(Message::ActivateDocument(second));
        let _ = app.update(Message::NewDocument);
        let _ = app.update(Message::SaveDocument);

        assert_eq!(app.document.active_id(), first);
        assert_eq!(app.document.iter().count(), 2);
        assert!(matches!(
            app.pending_after_save,
            Some(DestructiveFileAction::Close(id)) if id == window
        ));
    }

    #[test]
    fn command_shortcuts_reuse_application_messages() {
        let window = window::Id::unique();
        let command = keyboard::Modifiers::COMMAND;

        assert!(matches!(
            shortcut_message('n', command, window),
            Some(Message::NewDocument)
        ));
        assert!(matches!(
            shortcut_message('o', command, window),
            Some(Message::OpenDocument)
        ));
        assert!(matches!(
            shortcut_message('s', command, window),
            Some(Message::SaveDocument)
        ));
        assert!(matches!(
            shortcut_message('s', command | keyboard::Modifiers::SHIFT, window),
            Some(Message::SaveDocumentAs)
        ));
        assert!(matches!(
            shortcut_message('q', command, window),
            Some(Message::CloseRequested(id)) if id == window
        ));
        assert!(matches!(
            shortcut_message('b', command, window),
            Some(Message::Bold)
        ));
        assert!(matches!(
            shortcut_message('i', command, window),
            Some(Message::Italic)
        ));
        assert!(matches!(
            shortcut_message('u', command, window),
            Some(Message::Underline)
        ));
        assert!(matches!(
            shortcut_message('f', command, window),
            Some(Message::OpenSearch)
        ));
        assert!(matches!(
            shortcut_message('h', command, window),
            Some(Message::OpenReplace)
        ));
        assert!(shortcut_message('s', keyboard::Modifiers::NONE, window).is_none());
        assert!(shortcut_message('n', command | keyboard::Modifiers::SHIFT, window).is_none());
    }

    #[test]
    fn replace_all_is_a_single_undoable_edit() {
        let mut app = App::new();
        *app.document.active_mut() =
            Document::opened(PathBuf::from("document.typ"), "cat and cat".to_owned());
        app.search.visible = true;
        app.search.query = "cat".to_owned();
        app.search.replacement = "dog".to_owned();
        app.refresh_search_matches(Some(0), true);

        app.replace_all_matches();

        assert_eq!(app.document.snapshot().1, "dog and dog");
        app.document.perform(Action::Undo);
        assert_eq!(app.document.snapshot().1, "cat and cat");
    }

    #[test]
    fn replace_current_advances_to_the_next_match() {
        let mut app = App::new();
        *app.document.active_mut() =
            Document::opened(PathBuf::from("document.typ"), "cat cat".to_owned());
        app.search.visible = true;
        app.search.query = "cat".to_owned();
        app.search.replacement = "dog".to_owned();
        app.refresh_search_matches(Some(0), true);

        app.replace_current_match();

        assert_eq!(app.document.snapshot().1, "dog cat");
        assert_eq!(app.document.current_search_match(), Some(0));
        assert_eq!(app.document.search_matches(), vec![4..7]);
    }

    #[test]
    fn stale_external_read_cannot_undo_a_completed_save() {
        let mut app = App::new();
        let document_id = app.document.active_id();
        *app.document.active_mut() =
            Document::opened(PathBuf::from("document.typ"), "novo".to_owned());
        let stale_revision = app.document.storage_revision();
        app.document
            .mark_saved(PathBuf::from("document.typ"), "novo".to_owned());

        let _ = app.handle_external_files(vec![ExternalFileResult::Source {
            document_id,
            storage_revision: stale_revision,
            source: "antigo".to_owned(),
        }]);

        assert_eq!(app.document.snapshot().1, "novo");
        assert!(!app.document.is_dirty());
    }

    #[test]
    fn stale_project_scan_does_not_replace_the_current_tree() {
        let mut app = App::new();
        app.workspace_root = PathBuf::from("/projeto/novo");
        app.project_scan_busy = true;

        let _ = app.update(Message::ProjectScanned(project::ScanOutcome {
            root: PathBuf::from("/projeto/antigo"),
            files: Ok(vec![PathBuf::from("/projeto/antigo/main.typ")]),
        }));

        assert!(app.project_files.is_empty());
        assert!(app.project_scan_busy);
    }

    #[test]
    fn watcher_refresh_waits_for_in_flight_filesystem_reads() {
        let mut app = App::new();
        let now = Instant::now();
        app.watcher_deadline = Some(now);
        app.external_check_busy = true;

        let _ = app.dispatch_watcher_refresh(now);

        assert!(app.watcher_deadline.is_some_and(|deadline| deadline > now));
    }

    #[test]
    fn session_snapshot_restores_tab_order_active_draft_and_pane_layout() {
        let directory = tempfile::tempdir().expect("a temporary directory can be created");
        let mut app = App::fresh(Some(directory.path().join("session.json")));
        *app.document.active_mut() =
            Document::opened(PathBuf::from("/project/main.typ"), "saved".to_owned());
        app.document.perform(Action::MoveTo("saved".len()));
        app.document.perform(Action::Insert(" local".to_owned()));
        app.new_document();
        app.document.perform(Action::Insert("draft".to_owned()));
        app.workspace_root = PathBuf::from("/project");
        app.panes = panes_from_layout(session::PaneLayout {
            axis: session::Axis::Horizontal,
            ratio: 0.7,
            first: session::Pane::Preview,
        });
        app.settings.wrap_lines = true;
        app.settings.preview_zoom = 135;

        let stored = app.session_snapshot(false);
        let restored = App::restore(stored, None);
        let documents = restored
            .document
            .iter()
            .map(|(_, document)| document.snapshot().1)
            .collect::<Vec<_>>();

        assert_eq!(documents, vec!["saved local", "draft"]);
        assert_eq!(restored.document.snapshot().1, "draft");
        assert_eq!(restored.workspace_root, PathBuf::from("/project"));
        assert!(restored.settings.wrap_lines);
        assert_eq!(restored.settings.preview_zoom, 135);
        assert_eq!(
            restored.pane_layout(),
            session::PaneLayout {
                axis: session::Axis::Horizontal,
                ratio: 0.7,
                first: session::Pane::Preview,
            }
        );
    }

    #[test]
    fn final_session_snapshot_removes_discarded_drafts_and_local_edits() {
        let mut app = App::new();
        *app.document.active_mut() =
            Document::opened(PathBuf::from("/project/main.typ"), "saved".to_owned());
        let saved_document = app.document.active_id();
        app.document.perform(Action::MoveTo("saved".len()));
        app.document.perform(Action::Insert(" local".to_owned()));
        app.new_document();
        let draft = app.document.active_id();
        app.document
            .perform(Action::Insert("discard me".to_owned()));
        app.discarded_on_close.extend([saved_document, draft]);

        let stored = app.session_snapshot(true);

        assert_eq!(stored.documents.len(), 1);
        assert_eq!(
            stored.documents[0].path,
            Some(PathBuf::from("/project/main.typ"))
        );
        assert_eq!(stored.documents[0].text, "saved");
        assert_eq!(stored.documents[0].saved_text.as_deref(), Some("saved"));
        assert_eq!(stored.active_document, 0);
    }

    #[test]
    fn editing_schedules_a_debounced_session_write() {
        let directory = tempfile::tempdir().expect("a temporary directory can be created");
        let mut app = App::fresh(Some(directory.path().join("session.json")));

        let _ = app.update(Message::Editor(Action::Insert("edit".to_owned())));

        assert_eq!(app.session.revision, 1);
        assert!(app.session.deadline.is_some());
        assert!(!app.session.write_busy);
    }

    #[test]
    fn stale_session_write_is_repeated_before_window_close() {
        let directory = tempfile::tempdir().expect("a temporary directory can be created");
        let mut app = App::fresh(Some(directory.path().join("session.json")));
        let window = window::Id::unique();
        app.file_busy = true;
        app.session.revision = 2;
        app.session.write_busy = true;
        app.session.close_after_write = Some(window);

        let _ = app.handle_session_write_finished(SessionWriteOutcome {
            revision: 1,
            result: Ok(()),
        });

        assert!(app.session.write_busy);
        assert_eq!(app.session.close_after_write, Some(window));
        assert!(app.file_busy);
    }

    #[test]
    fn current_session_write_completes_window_close_cleanup() {
        let directory = tempfile::tempdir().expect("a temporary directory can be created");
        let mut app = App::fresh(Some(directory.path().join("session.json")));
        let window = window::Id::unique();
        let document = app.document.active_id();
        app.discarded_on_close.insert(document);
        app.file_busy = true;
        app.session.revision = 2;
        app.session.write_busy = true;
        app.session.close_after_write = Some(window);

        let _ = app.handle_session_write_finished(SessionWriteOutcome {
            revision: 2,
            result: Ok(()),
        });

        assert!(!app.session.write_busy);
        assert!(app.session.close_after_write.is_none());
        assert!(app.discarded_on_close.is_empty());
        assert!(!app.file_busy);
    }

    #[test]
    fn dirty_open_imports_are_sent_as_compiler_overlays() {
        let directory = tempfile::tempdir().expect("a temporary project can be created");
        let main_path = directory.path().join("main.typ");
        let part_path = directory.path().join("part.typ");
        let mut app = App::new();
        app.workspace_root = directory.path().to_path_buf();
        app.document = Documents::new(Document::opened(main_path, String::new()));
        let main = app.document.active_id();
        let part = app
            .document
            .add(Document::opened(part_path.clone(), "saved".to_owned()));
        app.document
            .get_mut(part)
            .expect("the import is open")
            .perform(Action::Insert(" unsaved".to_owned()));
        app.document.activate(main);

        let overlays = app.source_overlays();

        assert_eq!(overlays.len(), 1);
        assert_eq!(overlays[0].path, part_path);
        assert_eq!(overlays[0].text, " unsavedsaved");
    }

    #[test]
    fn imported_diagnostics_are_attached_to_the_matching_open_document() {
        let directory = tempfile::tempdir().expect("a temporary project can be created");
        let main_path = directory.path().join("main.typ");
        let part_path = directory.path().join("part.typ");
        let mut app = App::new();
        app.workspace_root = directory.path().to_path_buf();
        app.document = Documents::new(Document::opened(main_path, String::new()));
        let main = app.document.active_id();
        let part = app
            .document
            .add(Document::opened(part_path.clone(), "broken".to_owned()));
        app.document.activate(main);

        app.install_diagnostics(vec![compiler::ReportedDiagnostic {
            target: compiler::DiagnosticTarget::ProjectFile(part_path),
            range: 0..6,
            severity: compiler::DiagnosticSeverity::Error,
            message: "erro importado".to_owned(),
        }]);

        assert_eq!(
            app.document
                .get(part)
                .expect("the import is open")
                .content()
                .diagnostics()
                .len(),
            1
        );
        assert!(
            app.document
                .get(main)
                .expect("the main document is open")
                .content()
                .diagnostics()
                .is_empty()
        );
    }

    #[test]
    fn atomic_save_replaces_the_document_without_leaving_a_temporary_file() {
        let directory = tempfile::tempdir().expect("a temporary directory can be created");
        let path = directory.path().join("document.typ");
        fs::write(&path, "old").expect("the original document can be written");

        atomic_write_file(&path, b"new").expect("the document can be replaced atomically");

        assert_eq!(
            fs::read_to_string(&path).expect("the saved document can be read"),
            "new"
        );
        assert_eq!(
            fs::read_dir(directory.path())
                .expect("the directory can be read")
                .count(),
            1
        );
    }

    #[cfg(unix)]
    #[test]
    fn atomic_save_preserves_permissions_and_symbolic_links() {
        use std::os::unix::fs::{PermissionsExt, symlink};

        let directory = tempfile::tempdir().expect("a temporary directory can be created");
        let target = directory.path().join("target.typ");
        let link = directory.path().join("document.typ");
        fs::write(&target, "old").expect("the original document can be written");
        fs::set_permissions(&target, fs::Permissions::from_mode(0o640))
            .expect("the original permissions can be set");
        symlink(&target, &link).expect("the symbolic link can be created");

        atomic_write_file(&link, b"new").expect("the symbolic link target can be saved");

        assert!(
            fs::symlink_metadata(&link)
                .expect("the symbolic link still exists")
                .file_type()
                .is_symlink()
        );
        assert_eq!(
            fs::read_to_string(&target).expect("the target can be read"),
            "new"
        );
        assert_eq!(
            fs::metadata(&target)
                .expect("the target metadata can be read")
                .permissions()
                .mode()
                & 0o777,
            0o640
        );
    }
}

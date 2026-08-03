mod compiler;
mod document;
mod formatting;
mod search;

use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    time::Duration,
};

use document::Document;
use iced::{
    Element,
    Length::Fill,
    Subscription, Task, Theme, event, keyboard,
    time::{self, Instant},
    widget::{
        Id, button, checkbox, column, container, operation, pane_grid, row, scrollable, svg, text,
        text_input,
    },
    window,
};
use rfd::{AsyncFileDialog, AsyncMessageDialog, MessageButtons, MessageDialogResult, MessageLevel};
use typst_iced_editor::{Action, code_editor};

const DEBOUNCE: Duration = Duration::from_millis(250);
const DEBOUNCE_TICK: Duration = Duration::from_millis(50);
const DEMO: &str = include_str!("demo.typ");

fn main() -> iced::Result {
    iced::application(App::boot, App::update, App::view)
        .subscription(App::subscription)
        .title(App::title)
        .theme(Theme::Dark)
        .window_size([1200.0, 800.0])
        .centered()
        .exit_on_close_request(false)
        .run()
}

struct App {
    document: Document,
    panes: pane_grid::State<Pane>,
    workspace_root: PathBuf,
    compiler: Option<compiler::Sender>,
    pending_compile: Option<PendingCompile>,
    next_request_id: u64,
    latest_request_id: Option<u64>,
    preview: Option<svg::Handle>,
    preview_status: PreviewStatus,
    file_busy: bool,
    pending_after_save: Option<DestructiveFileAction>,
    pending_pdf_export: Option<PendingPdfExport>,
    search: SearchState,
    search_input_id: Id,
    file_status: Option<String>,
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
    NewDocument,
    OpenDocument,
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
    New,
    Open,
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
    Cancelled,
    Saved { path: PathBuf, source: String },
    Failed(String),
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
    fn boot() -> Self {
        let workspace_root = std::env::current_dir().unwrap_or_else(|error| {
            eprintln!("erro ao obter diretório atual: {error}");
            PathBuf::from(".")
        });

        let (mut panes, editor_pane) = pane_grid::State::new(Pane::Editor);
        panes.split(pane_grid::Axis::Vertical, editor_pane, Pane::Preview);

        Self {
            document: Document::draft(DEMO),
            panes,
            workspace_root,
            compiler: None,
            pending_compile: Some(PendingCompile {
                deadline: Instant::now(),
                reset_files: true,
            }),
            next_request_id: 0,
            latest_request_id: None,
            preview: None,
            preview_status: PreviewStatus::Waiting,
            file_busy: false,
            pending_after_save: None,
            pending_pdf_export: None,
            search: SearchState::default(),
            search_input_id: Id::unique(),
            file_status: Some("O tutorial inicial ainda não foi salvo".to_owned()),
        }
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

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Editor(action) => {
                let changed = action.is_edit();
                self.document.perform(action);

                if changed {
                    self.document.clear_diagnostics();
                    self.file_status = None;
                    self.schedule_compile(DEBOUNCE, false);
                    self.refresh_search_matches(None, false);
                }

                Task::none()
            }
            Message::Bold => {
                if self
                    .document
                    .edit(|content| formatting::toggle_surround(content, "*", "*"))
                {
                    self.after_formatting();
                }

                Task::none()
            }
            Message::Italic => {
                if self
                    .document
                    .edit(|content| formatting::toggle_surround(content, "_", "_"))
                {
                    self.after_formatting();
                }

                Task::none()
            }
            Message::Underline => {
                if self
                    .document
                    .edit(|content| formatting::toggle_surround(content, "#underline[", "]"))
                {
                    self.after_formatting();
                }

                Task::none()
            }
            Message::PrefixLines(prefix) => {
                if self
                    .document
                    .edit(|content| formatting::toggle_line_prefix(content, &prefix))
                {
                    self.after_formatting();
                }

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
                }

                Task::none()
            }
            Message::PaneResized(event) => {
                self.panes.resize(event.split, event.ratio);
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
            Message::NewDocument => self.request_destructive_action(DestructiveFileAction::New),
            Message::OpenDocument => self.request_destructive_action(DestructiveFileAction::Open),
            Message::SaveDocument => {
                self.pending_after_save = None;

                if self.document.is_dirty() {
                    self.start_save(false)
                } else {
                    Task::none()
                }
            }
            Message::SaveDocumentAs => {
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
                        self.execute_destructive_action(action)
                    }
                    UnsavedDecision::Cancel => {
                        self.pending_after_save = None;
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

        if self.pending_compile.is_some() {
            Subscription::batch([
                compiler,
                time::every(DEBOUNCE_TICK).map(Message::DebounceTick),
                close_requests,
                shortcuts,
            ])
        } else {
            Subscription::batch([compiler, close_requests, shortcuts])
        }
    }

    fn view(&self) -> Element<'_, Message> {
        let panes = pane_grid(&self.panes, |_id, pane, _is_maximized| {
            let content: Element<'_, Message> = match pane {
                Pane::Editor => code_editor(self.document.content())
                    .on_action(Message::Editor)
                    .into(),
                Pane::Preview => match &self.preview {
                    Some(handle) => scrollable(svg(handle.clone()).width(Fill)).into(),
                    None => container(text("Preview indisponível"))
                        .center_x(Fill)
                        .center_y(Fill)
                        .into(),
                },
            };

            pane_grid::Content::new(content)
        })
        .spacing(8)
        .min_size(200)
        .on_drag(Message::PaneDragged)
        .on_resize(10, Message::PaneResized);

        let new = file_button("Novo", Message::NewDocument, !self.file_busy);
        let open = file_button("Abrir", Message::OpenDocument, !self.file_busy);
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

        let toolbar = row![
            new,
            open,
            save,
            save_as,
            export_pdf,
            button("▶").on_press(Message::CompileNow),
            button("B").on_press(Message::Bold),
            button("I").on_press(Message::Italic),
            button("U").on_press(Message::Underline),
            button("Lista").on_press(Message::PrefixLines("- ".into())),
            button("Numeração").on_press(Message::PrefixLines("+ ".into())),
        ]
        .spacing(4)
        .padding(4);

        let status = container(text(self.status_text()).size(13))
            .width(Fill)
            .padding([5, 8]);

        let mut content = column![toolbar];

        if self.search.visible {
            content = content.push(self.search_view());
        }

        content
            .push(panes)
            .push(status)
            .width(Fill)
            .height(Fill)
            .into()
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

        if self.document.is_dirty() {
            self.file_busy = true;
            let name = self.document.display_name();

            Task::perform(confirm_unsaved(name), move |decision| {
                Message::UnsavedDecision { action, decision }
            })
        } else {
            self.execute_destructive_action(action)
        }
    }

    fn execute_destructive_action(&mut self, action: DestructiveFileAction) -> Task<Message> {
        match action {
            DestructiveFileAction::New => {
                let previous_config = self.compiler_config();
                self.document = Document::new();
                self.file_status = Some("Novo documento criado".to_owned());
                self.document_replaced(previous_config);
                Task::none()
            }
            DestructiveFileAction::Open => {
                self.file_busy = true;
                self.file_status = Some("Aguardando a escolha de um arquivo...".to_owned());
                let directory = self.document.directory(&self.workspace_root);

                Task::perform(open_document(directory), Message::OpenFinished)
            }
            DestructiveFileAction::Close(id) => window::close(id),
        }
    }

    fn start_save(&mut self, save_as: bool) -> Task<Message> {
        if self.file_busy {
            return Task::none();
        }

        self.file_busy = true;
        self.file_status = Some("Salvando documento...".to_owned());

        let (_, source) = self.document.snapshot();

        if !save_as && let Some(path) = self.document.path() {
            let path = path.to_path_buf();
            return Task::perform(write_document(path, source), Message::SaveFinished);
        }

        let directory = self.document.directory(&self.workspace_root);
        let file_name = self.document.display_name();

        Task::perform(
            save_document_as(directory, file_name, source),
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
                self.file_status = Some("A abertura foi cancelada".to_owned());
            }
            OpenOutcome::Failed(error) => {
                eprintln!("erro ao abrir documento: {error}");
                self.file_status = Some(format!("Erro ao abrir: {error}"));
            }
            OpenOutcome::Loaded { path, source } => {
                let previous_config = self.compiler_config();
                self.document = Document::opened(path.clone(), source);
                self.file_status = Some(format!("Aberto: {}", path.display()));
                self.document_replaced(previous_config);
            }
        }

        Task::none()
    }

    fn handle_save_finished(&mut self, outcome: SaveOutcome) -> Task<Message> {
        self.file_busy = false;

        match outcome {
            SaveOutcome::Cancelled => {
                self.pending_after_save = None;
                self.file_status = Some("O salvamento foi cancelado".to_owned());
                Task::none()
            }
            SaveOutcome::Failed(error) => {
                self.pending_after_save = None;
                eprintln!("erro ao salvar documento: {error}");
                self.file_status = Some(format!("Erro ao salvar: {error}"));
                Task::none()
            }
            SaveOutcome::Saved { path, source } => {
                let previous_config = self.compiler_config();
                self.document.mark_saved(path.clone(), source);

                self.file_status = Some(if self.document.is_dirty() {
                    format!(
                        "Versão salva em {}; há alterações mais recentes",
                        path.display()
                    )
                } else {
                    format!("Salvo em {}", path.display())
                });

                self.refresh_compiler_config(previous_config);

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
        self.replace_editor_pane_identity();
        self.preview = None;
        self.latest_request_id = None;
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
        compiler::Config::new(
            self.document.directory(&self.workspace_root),
            self.document.main_name(),
        )
    }

    fn after_formatting(&mut self) {
        self.document.clear_diagnostics();
        self.file_status = None;
        self.schedule_compile(Duration::ZERO, false);
        self.dispatch_compile(Instant::now());
        self.refresh_search_matches(None, false);
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

        self.document.set_diagnostics(output.diagnostics);

        if output.error_count > 0 {
            self.preview_status = PreviewStatus::Failed {
                errors: output.error_count,
                summary: output
                    .summary
                    .unwrap_or_else(|| "Falha ao compilar o documento".to_owned()),
            };
            return Task::none();
        }

        let Some(svg) = output.svg else {
            self.preview_status = PreviewStatus::Failed {
                errors: 1,
                summary: "A compilação não produziu um preview".to_owned(),
            };
            return Task::none();
        };

        self.preview = Some(svg::Handle::from_memory(svg));
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
            self.document.set_diagnostics(output.diagnostics);
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

    match tokio::fs::read_to_string(&path).await {
        Ok(source) => OpenOutcome::Loaded { path, source },
        Err(error) => OpenOutcome::Failed(format!("{}: {error}", path.display())),
    }
}

async fn save_document_as(directory: PathBuf, file_name: String, source: String) -> SaveOutcome {
    let Some(file) = AsyncFileDialog::new()
        .add_filter("Documento Typst", &["typ"])
        .set_directory(directory)
        .set_file_name(file_name)
        .set_title("Salvar documento Typst")
        .save_file()
        .await
    else {
        return SaveOutcome::Cancelled;
    };

    write_document(with_typst_extension(file.path()), source).await
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

async fn write_document(path: PathBuf, source: String) -> SaveOutcome {
    let destination = path.clone();
    let result = tokio::task::spawn_blocking(move || {
        atomic_write_file(&destination, source.as_bytes())?;
        Ok::<_, io::Error>(source)
    })
    .await;

    match result {
        Ok(Ok(source)) => SaveOutcome::Saved { path, source },
        Ok(Err(error)) => SaveOutcome::Failed(format!("{}: {error}", path.display())),
        Err(error) => SaveOutcome::Failed(format!(
            "{}: tarefa de salvamento interrompida: {error}",
            path.display()
        )),
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

        builder.permissions(
            permissions
                .clone()
                .unwrap_or_else(|| fs::Permissions::from_mode(0o666)),
        );
    }

    let mut temporary = builder.tempfile_in(directory)?;

    #[cfg(not(unix))]
    if let Some(permissions) = permissions {
        temporary.as_file().set_permissions(permissions)?;
    }

    temporary.write_all(contents)?;
    temporary.as_file().sync_all()?;
    temporary
        .persist(destination)
        .map(|_| ())
        .map_err(|error| error.error)
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
    fn discarding_before_new_replaces_the_editor_pane() {
        let mut app = App::boot();
        let previous_editor = app
            .panes
            .iter()
            .find_map(|(id, pane)| matches!(pane, Pane::Editor).then_some(*id))
            .expect("the editor pane exists");

        let _ = app.update(Message::UnsavedDecision {
            action: DestructiveFileAction::New,
            decision: UnsavedDecision::Discard,
        });

        let (_, source) = app.document.snapshot();
        let current_editor = app
            .panes
            .iter()
            .find_map(|(id, pane)| matches!(pane, Pane::Editor).then_some(*id))
            .expect("the replacement editor pane exists");

        assert!(source.is_empty());
        assert_ne!(current_editor, previous_editor);
        assert_eq!(app.panes.len(), 2);
    }

    #[test]
    fn closing_a_dirty_document_starts_confirmation() {
        let mut app = App::boot();

        let _ = app.update(Message::CloseRequested(window::Id::unique()));

        assert!(app.document.is_dirty());
        assert!(app.file_busy);
        assert!(app.pending_after_save.is_none());
    }

    #[test]
    fn saving_before_close_keeps_the_close_action_pending() {
        let mut app = App::boot();
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
        let mut app = App::boot();
        app.document = Document::opened(PathBuf::from("document.typ"), "cat and cat".to_owned());
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
        let mut app = App::boot();
        app.document = Document::opened(PathBuf::from("document.typ"), "cat cat".to_owned());
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

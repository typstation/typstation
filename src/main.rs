mod compiler;
mod document;
mod formatting;

use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use document::Document;
use iced::{
    Element,
    Length::Fill,
    Subscription, Task, Theme,
    time::{self, Instant},
    widget::{button, column, container, pane_grid, row, scrollable, svg, text},
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
    NewDocument,
    OpenDocument,
    SaveDocument,
    SaveDocumentAs,
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
            Message::Compiler(event) => {
                self.handle_compiler_event(event);
                Task::none()
            }
            Message::NewDocument => self.request_destructive_action(DestructiveFileAction::New),
            Message::OpenDocument => self.request_destructive_action(DestructiveFileAction::Open),
            Message::SaveDocument => {
                self.pending_after_save = None;
                self.start_save(false)
            }
            Message::SaveDocumentAs => {
                self.pending_after_save = None;
                self.start_save(true)
            }
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

        if self.pending_compile.is_some() {
            Subscription::batch([
                compiler,
                time::every(DEBOUNCE_TICK).map(Message::DebounceTick),
                close_requests,
            ])
        } else {
            Subscription::batch([compiler, close_requests])
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

        let toolbar = row![
            new,
            open,
            save,
            save_as,
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

        column![toolbar, panes, status]
            .width(Fill)
            .height(Fill)
            .into()
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

    fn handle_compiler_event(&mut self, event: compiler::Event) {
        match event {
            compiler::Event::Ready { config, sender } => {
                if config != self.compiler_config() {
                    return;
                }

                self.compiler = Some(sender);
                self.dispatch_compile(Instant::now());
            }
            compiler::Event::Finished { config, output } => {
                if config != self.compiler_config() {
                    return;
                }

                let current_revision = self.document.revision();

                if self.latest_request_id != Some(output.id) || current_revision != output.revision
                {
                    return;
                }

                self.document.set_diagnostics(output.diagnostics);

                if output.error_count > 0 {
                    self.preview_status = PreviewStatus::Failed {
                        errors: output.error_count,
                        summary: output
                            .summary
                            .unwrap_or_else(|| "Falha ao compilar o documento".to_owned()),
                    };
                    return;
                }

                let Some(svg) = output.svg else {
                    self.preview_status = PreviewStatus::Failed {
                        errors: 1,
                        summary: "A compilação não produziu um preview".to_owned(),
                    };
                    return;
                };

                self.preview = Some(svg::Handle::from_memory(svg));
                self.preview_status = PreviewStatus::Ready {
                    pages: output.page_count,
                    warnings: output.warning_count,
                };
            }
        }
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

async fn write_document(path: PathBuf, source: String) -> SaveOutcome {
    match tokio::fs::write(&path, source.as_bytes()).await {
        Ok(()) => SaveOutcome::Saved { path, source },
        Err(error) => SaveOutcome::Failed(format!("{}: {error}", path.display())),
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
}

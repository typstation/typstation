mod compiler;
mod display;
mod document;
mod formatting;
mod project;
mod project_search;
mod search;
mod session;
mod settings;
mod source_map;
pub mod ui;
mod watcher;

use std::{
    collections::HashSet,
    fs,
    io::{self, Write},
    ops::Range,
    path::{Path, PathBuf},
    process::Command as ProcessCommand,
    time::Duration,
};

use document::{
    Document, DocumentId, Documents, ExternalChangeKind, ExternalUpdate, compiler_location_for_path,
};
use iced::{
    Alignment, Border, Color, Element,
    Length::Fill,
    Point, Subscription, Task, Theme, event, keyboard, mouse,
    time::{self, Instant},
    widget::{
        Id, Space, Stack, column, container, mouse_area, operation, pane_grid, responsive, row,
        scrollable, slider, svg, text,
    },
    window,
};
use rfd::AsyncFileDialog;
use typst_iced_editor::{Action, code_editor};
use typstation::world::SourceOverlay;

const DEBOUNCE: Duration = Duration::from_millis(250);
const DEBOUNCE_TICK: Duration = Duration::from_millis(50);
const SESSION_DEBOUNCE: Duration = Duration::from_millis(750);
const SESSION_TICK: Duration = Duration::from_millis(200);
const AUTO_SAVE_DEBOUNCE: Duration = Duration::from_secs(2);
const AUTO_SAVE_TICK: Duration = Duration::from_millis(250);
const WATCHER_DEBOUNCE: Duration = Duration::from_millis(150);
const WATCHER_TICK: Duration = Duration::from_millis(50);
const PREVIEW_PADDING: f32 = 12.0;
const PREVIEW_PAGE_SPACING: f32 = 12.0;
const PREVIEW_LABEL_HEIGHT: f32 = 18.0;
const PREVIEW_LABEL_SPACING: f32 = 4.0;
const PREVIEW_HIT_DISTANCE: f32 = 12.0;
const TYPOGRAPHIC_POINTS_PER_INCH: f32 = 72.0;
const APP_BAR_HEIGHT: f32 = 40.0;
const APP_BAR_HORIZONTAL_PADDING: f32 = 4.0;
const PANE_DRAG_HANDLE_HEIGHT: f32 = 8.0;
const FILE_MENU_TRIGGER_WIDTH: f32 = 72.0;
const EDIT_MENU_TRIGGER_WIDTH: f32 = 64.0;
const VIEW_MENU_TRIGGER_WIDTH: f32 = 64.0;
const HELP_MENU_TRIGGER_WIDTH: f32 = 64.0;
const PROJECT_CONTEXT_MENU_WIDTH: f32 = 280.0;
const EXPORT_MENU_WIDTH: f32 = 280.0;
const CONTEXT_MENU_VIEWPORT_MARGIN: f32 = 4.0;
const SETTINGS_WINDOW_WIDTH: f32 = 640.0;
const SETTINGS_WINDOW_HEIGHT: f32 = 600.0;
const APP_ACTIONS_WIDTH: f32 = 328.0;
const DEMO: &str = include_str!("demo.typ");

fn preview_scale(zoom_percent: u16, logical_pixels_per_inch: f32) -> f32 {
    logical_pixels_per_inch / TYPOGRAPHIC_POINTS_PER_INCH * f32::from(zoom_percent) / 100.0
}

fn preview_canvas_width(viewport_width: f32, maximum_page_width: f32) -> f32 {
    viewport_width.max(maximum_page_width + PREVIEW_PADDING * 2.0)
}

fn main() -> iced::Result {
    iced::daemon(App::boot, App::update, App::view)
        .subscription(App::subscription)
        .title(App::title)
        .theme(App::theme)
        .run()
}

fn app_window_settings() -> window::Settings {
    window::Settings {
        size: iced::Size::new(1200.0, 800.0),
        maximized: true,
        position: window::Position::Centered,
        exit_on_close_request: false,
        ..window::Settings::default()
    }
}

fn settings_window_settings() -> window::Settings {
    window::Settings {
        size: iced::Size::new(SETTINGS_WINDOW_WIDTH, SETTINGS_WINDOW_HEIGHT),
        min_size: Some(iced::Size::new(520.0, 480.0)),
        position: window::Position::Centered,
        level: window::Level::AlwaysOnTop,
        exit_on_close_request: false,
        ..window::Settings::default()
    }
}

struct App {
    document: Documents,
    panes: pane_grid::State<Pane>,
    workspace_root: PathBuf,
    project_main: Option<PathBuf>,
    compiler: Option<compiler::Sender>,
    pending_compile: Option<PendingCompile>,
    compilation_revision: u64,
    next_request_id: u64,
    latest_request_id: Option<u64>,
    preview: Vec<PreviewPage>,
    preview_revision: Option<u64>,
    preview_status: PreviewStatus,
    preview_scroll_id: Id,
    preview_pointer: Option<PreviewPointer>,
    preview_highlight: Option<PreviewHighlight>,
    modifiers: keyboard::Modifiers,
    file_busy: bool,
    pending_after_save: Option<DestructiveFileAction>,
    pending_export: Option<PendingExport>,
    search: SearchState,
    search_input_id: Id,
    project_navigation: ProjectNavigation,
    project_search: ProjectSearchState,
    project_tree: Vec<project::ProjectEntry>,
    expanded_project_directories: HashSet<PathBuf>,
    selected_project_entry: Option<PathBuf>,
    selected_project_file: Option<PathBuf>,
    project_tree_focused: bool,
    document_outline: Vec<compiler::DocumentOutlineItem>,
    collapsed_outline_entries: HashSet<OutlineKey>,
    diagnostics: Vec<compiler::ReportedDiagnostic>,
    pending_source_reveal: Option<PendingSourceReveal>,
    project_scan_busy: bool,
    detect_project_main_on_scan: bool,
    external_check_busy: bool,
    watcher_deadline: Option<Instant>,
    discarded_on_close: HashSet<DocumentId>,
    discarded_tabs: HashSet<DocumentId>,
    closed_documents: Vec<session::Document>,
    recent_projects: Vec<PathBuf>,
    auto_save_deadline: Option<Instant>,
    auto_save_busy: bool,
    session: SessionTracker,
    settings: settings::Settings,
    settings_page: SettingsPage,
    preview_logical_ppi: f32,
    main_window: window::Id,
    settings_window: Option<window::Id>,
    pending_alert_dialog: Option<PendingAlertDialog>,
    open_menu: Option<AppMenu>,
    export_menu_visible: bool,
    menu_focus: usize,
    menu_bar_drag_active: bool,
    project_context_menu: Option<ProjectContextMenu>,
    cursor_position: Point,
    about_visible: bool,
    file_status: Option<String>,
}

struct PreviewPage {
    handle: svg::Handle,
    width: f32,
    height: f32,
    regions: Vec<source_map::SourceRegion>,
}

#[derive(Debug, Clone, Copy)]
struct PreviewPointer {
    page: usize,
    position: Point,
}

#[derive(Debug, Clone, Copy)]
struct PreviewHighlight {
    page: usize,
    bounds: source_map::SourceBounds,
}

struct PendingSourceReveal {
    path: PathBuf,
    range: Range<usize>,
    status: String,
}

#[derive(Debug, Clone)]
struct ProjectContextMenu {
    path: PathBuf,
    kind: project::EntryKind,
    position: Point,
}

#[derive(Debug, Clone)]
enum Message {
    Editor(Action),
    PaneDragged(pane_grid::DragEvent),
    PaneResized(pane_grid::ResizeEvent),
    DebounceTick(Instant),
    Compiler(compiler::Event),
    Bold,
    Italic,
    Underline,
    PrefixLines(String),
    OpenSettings,
    OpenExportSettings,
    CloseSettingsWindow,
    SettingsPageSelected(SettingsPage),
    TabWidthChanged(u16),
    AutoPairsChanged(bool),
    AutoIndentChanged(bool),
    AutoSaveChanged(bool),
    WrapLinesChanged(bool),
    ShowGutterChanged(bool),
    EditorFontSizeChanged(u16),
    LightThemeChanged(bool),
    PreviewZoomIn,
    PreviewZoomOut,
    PreviewZoomReset,
    PreviewZoomChanged(u16),
    PdfTaggedChanged(bool),
    PdfPrettyChanged(bool),
    SvgRenderBleedChanged(bool),
    SvgPrettyChanged(bool),
    SvgPageGapChanged(u16),
    HtmlPrettyChanged(bool),
    RevealInPreview,
    PreviewPointerMoved {
        page: usize,
        position: Point,
    },
    PreviewPointerLeft(usize),
    PreviewClicked(usize),
    ModifiersChanged(keyboard::Modifiers),
    OpenSearch,
    OpenProjectSearch,
    OpenReplace,
    CloseSearch,
    ToggleReplace,
    SearchQueryChanged(String),
    SearchReplacementChanged(String),
    SearchCaseChanged(bool),
    SearchWholeWordChanged(bool),
    SearchNext,
    SearchPrevious,
    ReplaceCurrent,
    ReplaceAll,
    ActivateDocument(DocumentId),
    ActivateRelativeDocument(bool),
    MoveActiveDocument(bool),
    ReopenClosedDocument,
    CloseActiveDocument,
    CloseDocument(DocumentId),
    NewDocument,
    OpenDocument,
    OpenProject,
    ProjectNavigationSelected(ProjectNavigation),
    OpenProblems,
    ProjectSearchQueryChanged(String),
    ProjectSearchReplacementChanged(String),
    ProjectSearchCaseChanged(bool),
    ProjectSearchWholeWordChanged(bool),
    ProjectSearchToggleReplace,
    ProjectSearchFinished(project_search::SearchOutcome),
    ProjectSearchResultPressed(PathBuf, Range<usize>),
    ProjectReplaceAll,
    ProjectReplaceFinished(project_search::ReplaceOutcome),
    DocumentOutlinePressed {
        target: source_map::SourceTarget,
        range: Range<usize>,
        has_children: bool,
    },
    ProjectEntryPressed(PathBuf, project::EntryKind),
    ProjectEntryContextRequested(PathBuf, project::EntryKind),
    ProjectTreeNavigate(TreeNavigation),
    FileDropped(window::Id, PathBuf),
    CreateProjectFile,
    CreateProjectDirectory,
    RefreshProjectTree,
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
    SaveAllDocuments,
    SaveAllFinished(Vec<SaveOutcome>),
    AutoSaveTick(Instant),
    AutoSaveFinished(Vec<SaveOutcome>),
    Export(compiler::ExportFormat),
    ToggleExportMenu,
    ToggleMenu(AppMenu),
    MenuBarPointerPressed(AppMenu),
    MenuBarPointerEntered(AppMenu),
    MenuBarPointerReleased,
    DismissMenu,
    MenuFocused(usize),
    MenuNavigate(MenuNavigation),
    MenuCommand(MenuCommand),
    CursorMoved(Point),
    EscapePressed,
    ExitApplication,
    CloseAbout,
    AlertDialogBlocked,
    DismissAlertDialog,
    ConfirmProjectDeletion,
    ExportPathSelected {
        format: compiler::ExportFormat,
        outcome: ExportPathOutcome,
    },
    ExportWriteFinished(ExportWriteOutcome),
    CloseRequested(window::Id),
    UnsavedDecision {
        action: DestructiveFileAction,
        decision: UnsavedDecision,
    },
    OpenFinished(OpenOutcome),
    SaveFinished(SaveOutcome),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AppMenu {
    File,
    Edit,
    View,
    Help,
}

impl AppMenu {
    const fn next(self) -> Self {
        match self {
            Self::File => Self::Edit,
            Self::Edit => Self::View,
            Self::View => Self::Help,
            Self::Help => Self::File,
        }
    }

    const fn previous(self) -> Self {
        match self {
            Self::File => Self::Help,
            Self::Edit => Self::File,
            Self::View => Self::Edit,
            Self::Help => Self::View,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MenuNavigation {
    NextItem,
    PreviousItem,
    FirstItem,
    LastItem,
    NextMenu,
    PreviousMenu,
    Activate,
}

#[derive(Debug, Clone)]
enum MenuCommand {
    NewDocument,
    OpenDocument,
    OpenProject,
    SaveDocument,
    SaveDocumentAs,
    SaveAllDocuments,
    Export(compiler::ExportFormat),
    CloseDocument(DocumentId),
    Exit,
    Editor(Action),
    OpenSearch,
    OpenProjectSearch,
    OpenReplace,
    ActivateRelativeDocument(bool),
    MoveActiveDocument(bool),
    ReopenClosedDocument,
    OpenRecentProject(PathBuf),
    ToggleSettings,
    OpenExportSettings,
    ShowGutter(bool),
    WrapLines(bool),
    ToggleProjectPane,
    PreviewZoomIn,
    PreviewZoomOut,
    PreviewZoomReset,
    OpenTypstDocumentation,
    ShowAbout,
    OpenProjectEntry(PathBuf, project::EntryKind),
    CreateProjectFileAt(PathBuf),
    CreateProjectDirectoryAt(PathBuf),
    RenameProjectEntry(PathBuf, project::EntryKind),
    MoveProjectEntry(PathBuf, project::EntryKind),
    DuplicateProjectEntry(PathBuf, project::EntryKind),
    CopyProjectPath(PathBuf),
    DeleteProjectEntry(PathBuf, project::EntryKind),
    SetProjectMain(PathBuf),
    ClearProjectMain,
    RefreshProjectTree,
}

#[derive(Debug, Clone)]
enum AppMenuEntry {
    Item {
        label: String,
        value: Option<String>,
        selected: bool,
        enabled: bool,
        command: MenuCommand,
    },
    Divider,
}

impl AppMenuEntry {
    fn item(
        label: impl Into<String>,
        value: Option<String>,
        selected: bool,
        enabled: bool,
        command: MenuCommand,
    ) -> Self {
        Self::Item {
            label: label.into(),
            value,
            selected,
            enabled,
            command,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pane {
    Project,
    Editor,
    Preview,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SettingsPage {
    Editor,
    Preview,
    Export,
    Appearance,
}

impl SettingsPage {
    const ALL: [Self; 4] = [Self::Editor, Self::Preview, Self::Export, Self::Appearance];

    const fn title(self) -> &'static str {
        match self {
            Self::Editor => "Editor",
            Self::Preview => "Preview",
            Self::Export => "Exportação",
            Self::Appearance => "Aparência",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProjectNavigation {
    Files,
    Search,
    Topics,
    Problems,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TreeNavigation {
    Previous,
    Next,
    ParentOrCollapse,
    ChildOrExpand,
    First,
    Last,
    Activate,
}

impl ProjectNavigation {
    const fn title(self) -> &'static str {
        match self {
            Self::Files => "Arquivos",
            Self::Search => "Busca",
            Self::Topics => "Sumário",
            Self::Problems => "Problemas",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct OutlineKey {
    target: source_map::SourceTarget,
    start: usize,
}

impl OutlineKey {
    fn new(target: source_map::SourceTarget, start: usize) -> Self {
        Self { target, start }
    }
}

#[derive(Debug, Clone, Copy)]
struct PendingCompile {
    deadline: Instant,
    reset_files: bool,
}

#[derive(Debug, Clone)]
struct PendingExport {
    format: compiler::ExportFormat,
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

#[derive(Debug, Default)]
struct ProjectSearchState {
    query: String,
    replacement: String,
    case_sensitive: bool,
    whole_word: bool,
    replace_visible: bool,
    revision: u64,
    busy: bool,
    results: Vec<project_search::Match>,
    skipped_files: usize,
    error: Option<String>,
    pending_replaced: usize,
    pending_changed_files: usize,
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
enum PendingAlertDialog {
    Unsaved {
        action: DestructiveFileAction,
        name: String,
    },
    DeleteProjectEntry {
        path: PathBuf,
        kind: project::EntryKind,
    },
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
struct SaveRequest {
    document_id: DocumentId,
    path: Option<PathBuf>,
    directory: PathBuf,
    file_name: String,
    source: String,
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
enum ExportPathOutcome {
    Cancelled,
    Selected(PathBuf),
}

#[derive(Debug, Clone)]
enum ExportWriteOutcome {
    Saved {
        format: compiler::ExportFormat,
        path: PathBuf,
    },
    Failed {
        format: compiler::ExportFormat,
        error: String,
    },
}

impl App {
    fn boot() -> (Self, Task<Message>) {
        let preview_logical_ppi = display::logical_pixels_per_inch();
        let session_path = session::default_path();
        let stored = match session_path.as_deref() {
            Some(path) => session::load(path),
            None => Ok(None),
        };
        let mut app = match stored {
            Ok(Some(stored)) => Self::restore(stored, session_path, preview_logical_ppi),
            Ok(None) => Self::fresh(session_path, preview_logical_ppi),
            Err(error) => {
                eprintln!("erro ao restaurar sessão: {error}");
                let mut app = Self::fresh(session_path, preview_logical_ppi);
                app.file_status = Some(format!("Não foi possível restaurar a sessão: {error}"));
                app
            }
        };
        let project_scan = app.refresh_project_tree();
        let external_check = app.check_external_files();
        let (main_window, open_main_window) = window::open(app_window_settings());
        app.main_window = main_window;

        (
            app,
            Task::batch([project_scan, external_check, open_main_window.discard()]),
        )
    }

    #[cfg(test)]
    fn new() -> Self {
        Self::fresh(None, display::FALLBACK_LOGICAL_PPI)
    }

    fn fresh(session_path: Option<PathBuf>, preview_logical_ppi: f32) -> Self {
        let workspace_root = std::env::current_dir().unwrap_or_else(|error| {
            eprintln!("erro ao obter diretório atual: {error}");
            PathBuf::from(".")
        });
        let panes = panes_from_layout(session::PaneLayout::default());

        let mut app = Self::build(
            Documents::new(Document::draft(DEMO)),
            panes,
            workspace_root,
            None,
            session_path,
            settings::Settings::default(),
            "O tutorial inicial ainda não foi salvo".to_owned(),
        );
        app.preview_logical_ppi = preview_logical_ppi;
        app
    }

    fn restore(
        stored: session::Session,
        session_path: Option<PathBuf>,
        preview_logical_ppi: f32,
    ) -> Self {
        let recent_projects = stored.recent_projects.clone();
        let documents = stored
            .documents
            .into_iter()
            .map(|document| Document::restored(document.path, document.text, document.saved_text))
            .collect();
        let document = Documents::restored(documents, stored.active_document);
        let panes = panes_from_layout(stored.pane_layout);

        let mut app = Self::build(
            document,
            panes,
            stored.workspace_root,
            stored.project_main,
            session_path,
            stored.settings,
            "Sessão anterior restaurada".to_owned(),
        );
        app.recent_projects = recent_projects;
        app.preview_logical_ppi = preview_logical_ppi;
        app
    }

    fn build(
        document: Documents,
        panes: pane_grid::State<Pane>,
        workspace_root: PathBuf,
        project_main: Option<PathBuf>,
        session_path: Option<PathBuf>,
        settings: settings::Settings,
        file_status: String,
    ) -> Self {
        let expanded_project_directories = HashSet::from([workspace_root.clone()]);
        let mut app = Self {
            document,
            panes,
            workspace_root,
            project_main,
            compiler: None,
            pending_compile: Some(PendingCompile {
                deadline: Instant::now(),
                reset_files: true,
            }),
            compilation_revision: 0,
            next_request_id: 0,
            latest_request_id: None,
            preview: Vec::new(),
            preview_revision: None,
            preview_status: PreviewStatus::Waiting,
            preview_scroll_id: Id::unique(),
            preview_pointer: None,
            preview_highlight: None,
            modifiers: keyboard::Modifiers::NONE,
            file_busy: false,
            pending_after_save: None,
            pending_export: None,
            search: SearchState::default(),
            search_input_id: Id::unique(),
            project_navigation: ProjectNavigation::Files,
            project_search: ProjectSearchState::default(),
            project_tree: Vec::new(),
            expanded_project_directories,
            selected_project_entry: None,
            selected_project_file: None,
            project_tree_focused: false,
            document_outline: Vec::new(),
            collapsed_outline_entries: HashSet::new(),
            diagnostics: Vec::new(),
            pending_source_reveal: None,
            project_scan_busy: false,
            detect_project_main_on_scan: false,
            external_check_busy: false,
            watcher_deadline: None,
            discarded_on_close: HashSet::new(),
            discarded_tabs: HashSet::new(),
            closed_documents: Vec::new(),
            recent_projects: Vec::new(),
            auto_save_deadline: None,
            auto_save_busy: false,
            session: SessionTracker::new(session_path),
            settings: settings.validate(),
            settings_page: SettingsPage::Editor,
            preview_logical_ppi: display::FALLBACK_LOGICAL_PPI,
            main_window: window::Id::unique(),
            settings_window: None,
            pending_alert_dialog: None,
            open_menu: None,
            export_menu_visible: false,
            menu_focus: 0,
            menu_bar_drag_active: false,
            project_context_menu: None,
            cursor_position: Point::ORIGIN,
            about_visible: false,
            file_status: Some(file_status),
        };
        let active_project_path = app
            .document
            .path()
            .filter(|path| path.starts_with(&app.workspace_root))
            .map(Path::to_path_buf);
        if let Some(path) = active_project_path {
            app.reveal_project_entry(&path);
            if path.extension().is_some_and(|extension| extension == "typ") {
                app.selected_project_file = Some(path);
            }
        }
        app.apply_editor_settings();
        app
    }

    fn title(&self, window: window::Id) -> String {
        if self.settings_window == Some(window) {
            return "Configurações - Typstation".to_owned();
        }

        let dirty = if self.document.is_dirty() { " *" } else { "" };

        format!(
            "{}{} - Typstation v{}",
            self.document.display_name(),
            dirty,
            env!("CARGO_PKG_VERSION")
        )
    }

    fn theme(&self, _window: window::Id) -> Theme {
        let scheme = match self.settings.theme {
            settings::ThemeMode::Dark => ui::tokens::ColorScheme::Dark,
            settings::ThemeMode::Light => ui::tokens::ColorScheme::Light,
        };

        ui::spectrum_theme(scheme)
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::ToggleMenu(menu) => {
                self.menu_bar_drag_active = false;
                self.toggle_menu(menu);
                Task::none()
            }
            Message::ToggleExportMenu => {
                self.open_menu = None;
                self.project_context_menu = None;
                self.menu_bar_drag_active = false;
                self.export_menu_visible = !self.export_menu_visible;
                if self.export_menu_visible {
                    self.menu_focus =
                        first_enabled_menu_item(&self.export_menu_entries()).unwrap_or(0);
                }
                Task::none()
            }
            Message::MenuBarPointerPressed(menu) => {
                self.menu_bar_drag_active = true;
                self.toggle_menu(menu);
                Task::none()
            }
            Message::MenuBarPointerEntered(menu) => {
                if self.menu_bar_drag_active || self.open_menu.is_some() {
                    self.show_menu(menu);
                }
                Task::none()
            }
            Message::MenuBarPointerReleased => {
                self.menu_bar_drag_active = false;
                Task::none()
            }
            Message::DismissMenu => {
                self.open_menu = None;
                self.project_context_menu = None;
                self.export_menu_visible = false;
                self.menu_bar_drag_active = false;
                Task::none()
            }
            Message::MenuFocused(index) => {
                if self.open_menu.is_some()
                    || self.project_context_menu.is_some()
                    || self.export_menu_visible
                {
                    self.menu_focus = index;
                }
                Task::none()
            }
            Message::MenuNavigate(navigation) => self.navigate_menu(navigation),
            Message::MenuCommand(command) => self.run_menu_command(command),
            Message::CursorMoved(position) => {
                self.cursor_position = position;
                Task::none()
            }
            Message::EscapePressed => {
                self.menu_bar_drag_active = false;
                if self.pending_alert_dialog.is_some() {
                    return self.dismiss_alert_dialog();
                }
                if self.project_context_menu.take().is_some() {
                    return Task::none();
                }
                if self.open_menu.take().is_some() {
                    return Task::none();
                }
                if self.export_menu_visible {
                    self.export_menu_visible = false;
                    return Task::none();
                }
                if self.about_visible {
                    self.about_visible = false;
                    return Task::none();
                }
                if self.search.visible {
                    self.search.visible = false;
                    self.document.clear_search_matches();
                }
                Task::none()
            }
            Message::ExitApplication => {
                self.request_destructive_action(DestructiveFileAction::Close(self.main_window))
            }
            Message::CloseAbout => {
                self.about_visible = false;
                Task::none()
            }
            Message::Editor(action) => {
                self.project_tree_focused = false;
                if action.is_edit() && self.file_busy {
                    return Task::none();
                }
                let reveal_in_preview =
                    self.modifiers.command() && matches!(action, Action::MoveTo(_));
                let changed = self.document.perform(action);

                if changed {
                    self.clear_compile_diagnostics();
                    self.file_status = None;
                    self.mark_session_changed();
                    self.schedule_compile(DEBOUNCE, false);
                    self.refresh_search_matches(None, false);
                }

                if reveal_in_preview {
                    self.reveal_cursor_in_preview()
                } else {
                    Task::none()
                }
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
            Message::OpenSettings => self.open_settings_window(),
            Message::OpenExportSettings => {
                self.settings_page = SettingsPage::Export;
                self.open_settings_window()
            }
            Message::CloseSettingsWindow => self.close_settings_window(),
            Message::SettingsPageSelected(page) => {
                self.settings_page = page;
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
            Message::AutoSaveChanged(auto_save) => {
                self.settings.auto_save = auto_save;
                self.auto_save_deadline = auto_save
                    .then(|| Instant::now() + AUTO_SAVE_DEBOUNCE)
                    .filter(|_| self.has_auto_save_documents());
                self.settings_changed(false);
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
            Message::PdfTaggedChanged(tagged) => {
                self.settings.pdf_tagged = tagged;
                self.settings_changed(false);
                Task::none()
            }
            Message::PdfPrettyChanged(pretty) => {
                self.settings.pdf_pretty = pretty;
                self.settings_changed(false);
                Task::none()
            }
            Message::SvgRenderBleedChanged(render_bleed) => {
                self.settings.svg_render_bleed = render_bleed;
                self.settings_changed(false);
                Task::none()
            }
            Message::SvgPrettyChanged(pretty) => {
                self.settings.svg_pretty = pretty;
                self.settings_changed(false);
                Task::none()
            }
            Message::SvgPageGapChanged(page_gap) => {
                self.settings.svg_page_gap = page_gap.min(72);
                self.settings_changed(false);
                Task::none()
            }
            Message::HtmlPrettyChanged(pretty) => {
                self.settings.html_pretty = pretty;
                self.settings_changed(false);
                Task::none()
            }
            Message::RevealInPreview => self.reveal_cursor_in_preview(),
            Message::PreviewPointerMoved { page, position } => {
                self.preview_pointer = Some(PreviewPointer { page, position });
                Task::none()
            }
            Message::PreviewPointerLeft(page) => {
                if self
                    .preview_pointer
                    .is_some_and(|pointer| pointer.page == page)
                {
                    self.preview_pointer = None;
                }
                Task::none()
            }
            Message::PreviewClicked(page) => self.reveal_preview_source(page),
            Message::ModifiersChanged(modifiers) => {
                self.modifiers = modifiers;
                Task::none()
            }
            Message::OpenSearch => self.open_search(false),
            Message::OpenReplace => self.open_search(true),
            Message::CloseSearch => {
                self.search.visible = false;
                self.document.clear_search_matches();
                Task::none()
            }
            Message::ToggleReplace => {
                self.search.replace_visible = !self.search.replace_visible;
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
            Message::ActivateRelativeDocument(reverse) => {
                if !self.file_busy {
                    self.activate_relative_document(reverse);
                }
                Task::none()
            }
            Message::MoveActiveDocument(reverse) => {
                if !self.file_busy && self.document.move_active(reverse) {
                    self.mark_session_changed();
                    self.file_status = Some("Aba reordenada".to_owned());
                }
                Task::none()
            }
            Message::ReopenClosedDocument => {
                if !self.file_busy {
                    self.reopen_closed_document();
                }
                Task::none()
            }
            Message::CloseActiveDocument => self.request_destructive_action(
                DestructiveFileAction::CloseDocument(self.document.active_id()),
            ),
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
            Message::ProjectNavigationSelected(navigation) => {
                self.project_navigation = navigation;
                self.project_context_menu = None;
                Task::none()
            }
            Message::OpenProblems => {
                self.project_navigation = ProjectNavigation::Problems;
                self.project_context_menu = None;
                if !self.project_pane_visible() {
                    self.toggle_project_pane();
                }
                Task::none()
            }
            Message::OpenProjectSearch => {
                self.project_navigation = ProjectNavigation::Search;
                self.project_context_menu = None;
                if !self.project_pane_visible() {
                    self.toggle_project_pane();
                }
                self.start_project_search()
            }
            Message::ProjectSearchQueryChanged(query) => {
                self.project_search.query = query;
                self.start_project_search()
            }
            Message::ProjectSearchReplacementChanged(replacement) => {
                self.project_search.replacement = replacement;
                Task::none()
            }
            Message::ProjectSearchCaseChanged(case_sensitive) => {
                self.project_search.case_sensitive = case_sensitive;
                self.start_project_search()
            }
            Message::ProjectSearchWholeWordChanged(whole_word) => {
                self.project_search.whole_word = whole_word;
                self.start_project_search()
            }
            Message::ProjectSearchToggleReplace => {
                self.project_search.replace_visible = !self.project_search.replace_visible;
                Task::none()
            }
            Message::ProjectSearchFinished(outcome) => {
                self.handle_project_search_finished(outcome);
                Task::none()
            }
            Message::ProjectSearchResultPressed(path, range) => self.reveal_source_target(
                source_map::SourceTarget::ProjectFile(path),
                range,
                "Resultado da busca revelado no editor",
            ),
            Message::ProjectReplaceAll => self.replace_all_project_matches(),
            Message::ProjectReplaceFinished(outcome) => {
                self.handle_project_replace_finished(outcome)
            }
            Message::DocumentOutlinePressed {
                target,
                range,
                has_children,
            } => self.document_outline_pressed(target, range, has_children),
            Message::ProjectEntryPressed(path, kind) => self.project_entry_pressed(path, kind),
            Message::ProjectEntryContextRequested(path, kind) => {
                self.project_tree_focused = true;
                self.show_project_context_menu(path, kind);
                Task::none()
            }
            Message::ProjectTreeNavigate(navigation) => self.navigate_project_tree(navigation),
            Message::FileDropped(window, path) => {
                if window == self.main_window {
                    self.handle_file_dropped(path)
                } else {
                    Task::none()
                }
            }
            Message::CreateProjectFile => self.start_create_project_file(),
            Message::CreateProjectDirectory => self.start_create_project_directory(),
            Message::RefreshProjectTree => self.refresh_project_tree(),
            Message::ProjectOperationFinished(outcome) => self.handle_project_operation(outcome),
            Message::OpenDiagnostic(target, range) => self.open_diagnostic(target, range),
            Message::ProjectFolderSelected(path) => self.handle_project_folder_selected(path),
            Message::ProjectScanned(outcome) => {
                if outcome.root != self.workspace_root {
                    return Task::none();
                }

                self.project_scan_busy = false;
                match outcome.snapshot {
                    Ok(snapshot) => {
                        if self
                            .selected_project_entry
                            .as_ref()
                            .is_some_and(|selected| {
                                !snapshot.contains_path(selected) && !selected.exists()
                            })
                        {
                            self.selected_project_entry = None;
                        }
                        if self.selected_project_file.as_ref().is_some_and(|selected| {
                            !snapshot.typst_files.contains(selected) && !selected.exists()
                        }) {
                            self.selected_project_file = None;
                        }
                        if self.project_context_menu.as_ref().is_some_and(|context| {
                            context.path != self.workspace_root
                                && !snapshot.contains_path(&context.path)
                                && !context.path.exists()
                        }) {
                            self.project_context_menu = None;
                        }
                        let missing_main = self.project_main.as_ref().is_some_and(|main| {
                            !snapshot.typst_files.contains(main) && !main.exists()
                        });
                        let detected_main =
                            if self.detect_project_main_on_scan && self.project_main.is_none() {
                                let candidate = self.workspace_root.join("main.typ");
                                snapshot
                                    .typst_files
                                    .contains(&candidate)
                                    .then_some(candidate)
                            } else {
                                None
                            };
                        self.detect_project_main_on_scan = false;
                        let workspace_root = self.workspace_root.clone();
                        self.expanded_project_directories.retain(|path| {
                            path == &workspace_root || snapshot.contains_directory(path)
                        });
                        self.project_tree = snapshot.entries;

                        if missing_main {
                            self.update_project_main(
                                None,
                                "O arquivo principal não existe mais; usando a aba ativa",
                            );
                        } else if let Some(main) = detected_main {
                            self.update_project_main(
                                Some(main),
                                "main.typ detectado como documento principal",
                            );
                        }
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
            Message::SaveAllDocuments => self.start_save_all(),
            Message::SaveAllFinished(outcomes) => self.handle_save_all_finished(outcomes, false),
            Message::AutoSaveTick(now) => self.dispatch_auto_save(now),
            Message::AutoSaveFinished(outcomes) => self.handle_save_all_finished(outcomes, true),
            Message::Export(format) => {
                self.export_menu_visible = false;
                self.start_export(format)
            }
            Message::ExportPathSelected { format, outcome } => {
                self.handle_export_path_selected(format, outcome)
            }
            Message::ExportWriteFinished(outcome) => self.handle_export_write_finished(outcome),
            Message::CloseRequested(id) if self.settings_window == Some(id) => {
                self.settings_window = None;
                window::close(id)
            }
            Message::CloseRequested(id) if id == self.main_window => {
                self.request_destructive_action(DestructiveFileAction::Close(id))
            }
            Message::CloseRequested(_) => Task::none(),
            Message::AlertDialogBlocked => Task::none(),
            Message::DismissAlertDialog => self.dismiss_alert_dialog(),
            Message::ConfirmProjectDeletion => self.confirm_project_deletion(),
            Message::UnsavedDecision { action, decision } => {
                self.pending_alert_dialog = None;
                self.file_busy = false;

                match decision {
                    UnsavedDecision::Save => {
                        self.pending_after_save = Some(action);
                        self.start_save(false)
                    }
                    UnsavedDecision::Discard => {
                        self.pending_after_save = None;
                        if let DestructiveFileAction::CloseDocument(id) = action {
                            self.discarded_tabs.insert(id);
                        }
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
        let shortcuts = if self.pending_alert_dialog.is_some() {
            alert_dialog_keyboard_subscription()
        } else if self.open_menu.is_some()
            || self.project_context_menu.is_some()
            || self.export_menu_visible
        {
            menu_keyboard_subscription()
        } else {
            shortcut_subscription()
        };
        let watcher = watcher::subscription(self.workspace_root.clone()).map(Message::Watcher);
        let menu_bar_pointer = menu_bar_pointer_subscription();
        let file_drop = file_drop_subscription();
        let mut subscriptions = vec![
            compiler,
            close_requests,
            shortcuts,
            watcher,
            menu_bar_pointer,
            file_drop,
        ];

        if self.project_tree_focused && self.project_navigation == ProjectNavigation::Files {
            subscriptions.push(project_tree_keyboard_subscription());
        }

        if self.pending_compile.is_some() {
            subscriptions.push(time::every(DEBOUNCE_TICK).map(Message::DebounceTick));
        }
        if self.session.deadline.is_some() {
            subscriptions.push(time::every(SESSION_TICK).map(Message::SessionTick));
        }
        if self.auto_save_deadline.is_some() {
            subscriptions.push(time::every(AUTO_SAVE_TICK).map(Message::AutoSaveTick));
        }
        if self.watcher_deadline.is_some() {
            subscriptions.push(time::every(WATCHER_TICK).map(Message::WatcherTick));
        }

        Subscription::batch(subscriptions)
    }

    fn open_settings_window(&mut self) -> Task<Message> {
        self.open_menu = None;
        self.project_context_menu = None;
        self.export_menu_visible = false;

        if let Some(window) = self.settings_window {
            return window::gain_focus(window);
        }

        let (window, open) = window::open(settings_window_settings());
        self.settings_window = Some(window);
        open.discard()
    }

    fn close_settings_window(&mut self) -> Task<Message> {
        let Some(window) = self.settings_window.take() else {
            return Task::none();
        };

        window::close(window)
    }

    fn toggle_menu(&mut self, menu: AppMenu) {
        if self.open_menu == Some(menu) {
            self.open_menu = None;
            return;
        }

        self.show_menu(menu);
    }

    fn show_menu(&mut self, menu: AppMenu) {
        self.project_context_menu = None;
        self.export_menu_visible = false;
        self.open_menu = Some(menu);
        self.menu_focus = first_enabled_menu_item(&self.menu_entries(menu)).unwrap_or(0);
    }

    fn navigate_menu(&mut self, navigation: MenuNavigation) -> Task<Message> {
        if self.open_menu.is_none()
            && self.project_context_menu.is_none()
            && !self.export_menu_visible
        {
            return Task::none();
        }

        match navigation {
            MenuNavigation::NextMenu | MenuNavigation::PreviousMenu => {
                let Some(current) = self.open_menu else {
                    return Task::none();
                };
                let menu = if navigation == MenuNavigation::NextMenu {
                    current.next()
                } else {
                    current.previous()
                };
                self.open_menu = Some(menu);
                self.menu_focus = first_enabled_menu_item(&self.menu_entries(menu)).unwrap_or(0);
                Task::none()
            }
            MenuNavigation::Activate => {
                let entries = self.active_menu_entries();
                let command = menu_command_at(&entries, self.menu_focus);
                command.map_or_else(Task::none, |command| self.run_menu_command(command))
            }
            MenuNavigation::NextItem
            | MenuNavigation::PreviousItem
            | MenuNavigation::FirstItem
            | MenuNavigation::LastItem => {
                let enabled = enabled_menu_items(&self.active_menu_entries());
                if enabled.is_empty() {
                    return Task::none();
                }

                let current = enabled
                    .iter()
                    .position(|index| *index == self.menu_focus)
                    .unwrap_or(0);
                self.menu_focus = match navigation {
                    MenuNavigation::NextItem => enabled[(current + 1) % enabled.len()],
                    MenuNavigation::PreviousItem => {
                        enabled[(current + enabled.len() - 1) % enabled.len()]
                    }
                    MenuNavigation::FirstItem => enabled[0],
                    MenuNavigation::LastItem => enabled[enabled.len() - 1],
                    MenuNavigation::NextMenu
                    | MenuNavigation::PreviousMenu
                    | MenuNavigation::Activate => unreachable!(),
                };
                Task::none()
            }
        }
    }

    fn run_menu_command(&mut self, command: MenuCommand) -> Task<Message> {
        self.open_menu = None;
        self.project_context_menu = None;
        self.export_menu_visible = false;
        self.menu_bar_drag_active = false;

        match command {
            MenuCommand::NewDocument => self.update(Message::NewDocument),
            MenuCommand::OpenDocument => self.update(Message::OpenDocument),
            MenuCommand::OpenProject => self.update(Message::OpenProject),
            MenuCommand::SaveDocument => self.update(Message::SaveDocument),
            MenuCommand::SaveDocumentAs => self.update(Message::SaveDocumentAs),
            MenuCommand::SaveAllDocuments => self.update(Message::SaveAllDocuments),
            MenuCommand::Export(format) => self.update(Message::Export(format)),
            MenuCommand::CloseDocument(id) => self.update(Message::CloseDocument(id)),
            MenuCommand::Exit => self.update(Message::ExitApplication),
            MenuCommand::Editor(action) => self.update(Message::Editor(action)),
            MenuCommand::OpenSearch => self.update(Message::OpenSearch),
            MenuCommand::OpenProjectSearch => self.update(Message::OpenProjectSearch),
            MenuCommand::OpenReplace => self.update(Message::OpenReplace),
            MenuCommand::ActivateRelativeDocument(reverse) => {
                self.update(Message::ActivateRelativeDocument(reverse))
            }
            MenuCommand::MoveActiveDocument(reverse) => {
                self.update(Message::MoveActiveDocument(reverse))
            }
            MenuCommand::ReopenClosedDocument => self.update(Message::ReopenClosedDocument),
            MenuCommand::OpenRecentProject(path) => {
                self.update(Message::ProjectFolderSelected(Some(path)))
            }
            MenuCommand::ToggleSettings => self.update(Message::OpenSettings),
            MenuCommand::OpenExportSettings => self.update(Message::OpenExportSettings),
            MenuCommand::ShowGutter(show) => self.update(Message::ShowGutterChanged(show)),
            MenuCommand::WrapLines(wrap) => self.update(Message::WrapLinesChanged(wrap)),
            MenuCommand::ToggleProjectPane => {
                self.toggle_project_pane();
                Task::none()
            }
            MenuCommand::PreviewZoomIn => self.update(Message::PreviewZoomIn),
            MenuCommand::PreviewZoomOut => self.update(Message::PreviewZoomOut),
            MenuCommand::PreviewZoomReset => self.update(Message::PreviewZoomReset),
            MenuCommand::OpenTypstDocumentation => {
                self.file_status = Some(match open_external_url("https://typst.app/docs/") {
                    Ok(()) => "Documentação do Typst aberta no navegador".to_owned(),
                    Err(error) => {
                        eprintln!("erro ao abrir documentação do Typst: {error}");
                        format!("Não foi possível abrir a documentação: {error}")
                    }
                });
                Task::none()
            }
            MenuCommand::ShowAbout => {
                self.about_visible = true;
                Task::none()
            }
            MenuCommand::OpenProjectEntry(path, kind) => self.project_entry_pressed(path, kind),
            MenuCommand::CreateProjectFileAt(directory) => {
                self.start_create_project_file_at(directory)
            }
            MenuCommand::CreateProjectDirectoryAt(directory) => {
                self.start_create_project_directory_at(directory)
            }
            MenuCommand::RenameProjectEntry(path, kind) => {
                self.start_rename_project_entry(path, kind)
            }
            MenuCommand::MoveProjectEntry(path, kind) => self.start_move_project_entry(path, kind),
            MenuCommand::DuplicateProjectEntry(path, kind) => {
                self.start_duplicate_project_entry(path, kind)
            }
            MenuCommand::CopyProjectPath(path) => {
                self.file_status = Some(format!("Caminho copiado: {}", path.display()));
                iced::clipboard::write(path.to_string_lossy().into_owned())
            }
            MenuCommand::DeleteProjectEntry(path, kind) => {
                self.start_delete_project_entry(path, kind)
            }
            MenuCommand::SetProjectMain(path) => {
                self.set_project_main(path);
                Task::none()
            }
            MenuCommand::ClearProjectMain => {
                self.clear_project_main();
                Task::none()
            }
            MenuCommand::RefreshProjectTree => self.refresh_project_tree(),
        }
    }

    fn active_menu_entries(&self) -> Vec<AppMenuEntry> {
        if let Some(menu) = self.open_menu {
            self.menu_entries(menu)
        } else if let Some(context) = self.project_context_menu.as_ref() {
            self.project_context_entries(context)
        } else if self.export_menu_visible {
            self.export_menu_entries()
        } else {
            Vec::new()
        }
    }

    fn export_menu_entries(&self) -> Vec<AppMenuEntry> {
        let enabled = !self.file_busy && self.compiler.is_some();

        vec![
            AppMenuEntry::item(
                "Exportar como SVG…",
                None,
                false,
                enabled,
                MenuCommand::Export(compiler::ExportFormat::Svg),
            ),
            AppMenuEntry::item(
                "Exportar como HTML…",
                None,
                false,
                enabled,
                MenuCommand::Export(compiler::ExportFormat::Html),
            ),
            AppMenuEntry::Divider,
            AppMenuEntry::item(
                "Configurações de exportação…",
                None,
                false,
                true,
                MenuCommand::OpenExportSettings,
            ),
        ]
    }

    fn menu_entries(&self, menu: AppMenu) -> Vec<AppMenuEntry> {
        let can_edit = !self.file_busy;

        match menu {
            AppMenu::File => {
                let mut entries = vec![
                    AppMenuEntry::item(
                        "Novo documento",
                        Some(command_shortcut("N")),
                        false,
                        !self.file_busy,
                        MenuCommand::NewDocument,
                    ),
                    AppMenuEntry::item(
                        "Abrir arquivo…",
                        Some(command_shortcut("O")),
                        false,
                        !self.file_busy,
                        MenuCommand::OpenDocument,
                    ),
                    AppMenuEntry::item(
                        "Abrir projeto…",
                        Some(command_shift_shortcut("O")),
                        false,
                        !self.file_busy,
                        MenuCommand::OpenProject,
                    ),
                    AppMenuEntry::Divider,
                    AppMenuEntry::item(
                        "Salvar",
                        Some(command_shortcut("S")),
                        false,
                        !self.file_busy && self.document.is_dirty(),
                        MenuCommand::SaveDocument,
                    ),
                    AppMenuEntry::item(
                        "Salvar como…",
                        Some(command_shift_shortcut("S")),
                        false,
                        !self.file_busy,
                        MenuCommand::SaveDocumentAs,
                    ),
                    AppMenuEntry::item(
                        "Salvar tudo",
                        Some(command_alt_shortcut("S")),
                        false,
                        !self.file_busy
                            && self
                                .document
                                .iter()
                                .any(|(_, document)| document.is_dirty()),
                        MenuCommand::SaveAllDocuments,
                    ),
                    AppMenuEntry::item(
                        "Exportar PDF…",
                        None,
                        false,
                        !self.file_busy && self.compiler.is_some(),
                        MenuCommand::Export(compiler::ExportFormat::Pdf),
                    ),
                    AppMenuEntry::item(
                        "Exportar SVG…",
                        None,
                        false,
                        !self.file_busy && self.compiler.is_some(),
                        MenuCommand::Export(compiler::ExportFormat::Svg),
                    ),
                    AppMenuEntry::item(
                        "Exportar HTML…",
                        None,
                        false,
                        !self.file_busy && self.compiler.is_some(),
                        MenuCommand::Export(compiler::ExportFormat::Html),
                    ),
                    AppMenuEntry::Divider,
                    AppMenuEntry::item(
                        "Fechar documento",
                        Some(command_shortcut("W")),
                        false,
                        !self.file_busy,
                        MenuCommand::CloseDocument(self.document.active_id()),
                    ),
                    AppMenuEntry::item(
                        "Reabrir aba fechada",
                        Some(command_shift_shortcut("T")),
                        false,
                        !self.file_busy && !self.closed_documents.is_empty(),
                        MenuCommand::ReopenClosedDocument,
                    ),
                    AppMenuEntry::item(
                        "Próxima aba",
                        Some("Ctrl+Tab".to_owned()),
                        false,
                        !self.file_busy && self.document.len() > 1,
                        MenuCommand::ActivateRelativeDocument(false),
                    ),
                    AppMenuEntry::item(
                        "Aba anterior",
                        Some("Ctrl+Shift+Tab".to_owned()),
                        false,
                        !self.file_busy && self.document.len() > 1,
                        MenuCommand::ActivateRelativeDocument(true),
                    ),
                    AppMenuEntry::item(
                        "Mover aba para a esquerda",
                        Some("Ctrl+Shift+PageUp".to_owned()),
                        false,
                        !self.file_busy && self.document.active_index() > 0,
                        MenuCommand::MoveActiveDocument(true),
                    ),
                    AppMenuEntry::item(
                        "Mover aba para a direita",
                        Some("Ctrl+Shift+PageDown".to_owned()),
                        false,
                        !self.file_busy && self.document.active_index() + 1 < self.document.len(),
                        MenuCommand::MoveActiveDocument(false),
                    ),
                    AppMenuEntry::item(
                        "Sair",
                        Some(command_shortcut("Q")),
                        false,
                        true,
                        MenuCommand::Exit,
                    ),
                ];
                if !self.recent_projects.is_empty() {
                    let mut recent = vec![AppMenuEntry::Divider];
                    recent.extend(self.recent_projects.iter().take(5).cloned().map(|path| {
                        let label = format!("Projeto recente: {}", path.display());
                        AppMenuEntry::item(
                            label,
                            None,
                            false,
                            !self.file_busy && path.is_dir(),
                            MenuCommand::OpenRecentProject(path),
                        )
                    }));
                    entries.splice(3..3, recent);
                }
                entries
            }
            AppMenu::Edit => vec![
                AppMenuEntry::item(
                    "Desfazer",
                    Some(command_shortcut("Z")),
                    false,
                    can_edit,
                    MenuCommand::Editor(Action::Undo),
                ),
                AppMenuEntry::item(
                    "Refazer",
                    Some(command_shift_shortcut("Z")),
                    false,
                    can_edit,
                    MenuCommand::Editor(Action::Redo),
                ),
                AppMenuEntry::Divider,
                AppMenuEntry::item(
                    "Buscar…",
                    Some(command_shortcut("F")),
                    false,
                    true,
                    MenuCommand::OpenSearch,
                ),
                AppMenuEntry::item(
                    "Buscar no projeto…",
                    Some(command_shift_shortcut("F")),
                    false,
                    true,
                    MenuCommand::OpenProjectSearch,
                ),
                AppMenuEntry::item(
                    "Substituir…",
                    Some(command_shortcut("H")),
                    false,
                    true,
                    MenuCommand::OpenReplace,
                ),
                AppMenuEntry::Divider,
                AppMenuEntry::item(
                    "Alternar comentário de linha",
                    None,
                    false,
                    can_edit,
                    MenuCommand::Editor(Action::ToggleLineComment),
                ),
                AppMenuEntry::item(
                    "Duplicar linha ou seleção",
                    None,
                    false,
                    can_edit,
                    MenuCommand::Editor(Action::DuplicateLine),
                ),
                AppMenuEntry::item(
                    "Mover linha para cima",
                    None,
                    false,
                    can_edit,
                    MenuCommand::Editor(Action::MoveLineUp),
                ),
                AppMenuEntry::item(
                    "Mover linha para baixo",
                    None,
                    false,
                    can_edit,
                    MenuCommand::Editor(Action::MoveLineDown),
                ),
                AppMenuEntry::Divider,
                AppMenuEntry::item(
                    "Configurações…",
                    None,
                    false,
                    true,
                    MenuCommand::ToggleSettings,
                ),
            ],
            AppMenu::View => vec![
                AppMenuEntry::item(
                    "Mostrar árvore do projeto",
                    None,
                    self.project_pane_visible(),
                    true,
                    MenuCommand::ToggleProjectPane,
                ),
                AppMenuEntry::item(
                    "Mostrar números de linha",
                    None,
                    self.settings.show_gutter,
                    true,
                    MenuCommand::ShowGutter(!self.settings.show_gutter),
                ),
                AppMenuEntry::item(
                    "Quebrar linhas",
                    None,
                    self.settings.wrap_lines,
                    true,
                    MenuCommand::WrapLines(!self.settings.wrap_lines),
                ),
                AppMenuEntry::Divider,
                AppMenuEntry::item(
                    "Diminuir zoom do preview",
                    Some(command_shortcut("−")),
                    false,
                    self.settings.preview_zoom > 25,
                    MenuCommand::PreviewZoomOut,
                ),
                AppMenuEntry::item(
                    "Tamanho real do documento",
                    Some(command_shortcut("0")),
                    self.settings.preview_zoom == 100,
                    true,
                    MenuCommand::PreviewZoomReset,
                ),
                AppMenuEntry::item(
                    "Aumentar zoom do preview",
                    Some(command_shortcut("+")),
                    false,
                    self.settings.preview_zoom < 300,
                    MenuCommand::PreviewZoomIn,
                ),
            ],
            AppMenu::Help => vec![
                AppMenuEntry::item(
                    "Documentação do Typst",
                    None,
                    false,
                    true,
                    MenuCommand::OpenTypstDocumentation,
                ),
                AppMenuEntry::Divider,
                AppMenuEntry::item(
                    "Sobre o Typstation",
                    None,
                    false,
                    true,
                    MenuCommand::ShowAbout,
                ),
            ],
        }
    }

    fn show_project_context_menu(&mut self, path: PathBuf, kind: project::EntryKind) {
        if self.file_busy {
            return;
        }

        self.open_menu = None;
        self.export_menu_visible = false;
        self.menu_bar_drag_active = false;
        self.selected_project_entry = Some(path.clone());
        self.selected_project_file =
            (kind == project::EntryKind::TypstFile).then_some(path.clone());
        self.project_context_menu = Some(ProjectContextMenu {
            path,
            kind,
            position: self.cursor_position,
        });
        self.menu_focus = self
            .project_context_menu
            .as_ref()
            .and_then(|context| first_enabled_menu_item(&self.project_context_entries(context)))
            .unwrap_or(0);
    }

    fn project_context_entries(&self, context: &ProjectContextMenu) -> Vec<AppMenuEntry> {
        let enabled = !self.file_busy;
        let is_root = context.path == self.workspace_root;
        let target_directory = match context.kind {
            project::EntryKind::Directory => context.path.clone(),
            project::EntryKind::TypstFile | project::EntryKind::File => context
                .path
                .parent()
                .unwrap_or(&self.workspace_root)
                .to_path_buf(),
        };
        let mut entries = Vec::new();

        match context.kind {
            project::EntryKind::Directory => {
                let expanded = self.expanded_project_directories.contains(&context.path);
                entries.push(AppMenuEntry::item(
                    if expanded {
                        "Recolher pasta"
                    } else {
                        "Expandir pasta"
                    },
                    None,
                    false,
                    enabled,
                    MenuCommand::OpenProjectEntry(context.path.clone(), context.kind),
                ));
                entries.push(AppMenuEntry::Divider);
                entries.push(AppMenuEntry::item(
                    "Novo arquivo Typst…",
                    None,
                    false,
                    enabled,
                    MenuCommand::CreateProjectFileAt(target_directory.clone()),
                ));
                entries.push(AppMenuEntry::item(
                    "Nova pasta…",
                    None,
                    false,
                    enabled,
                    MenuCommand::CreateProjectDirectoryAt(target_directory),
                ));

                if !is_root {
                    entries.push(AppMenuEntry::Divider);
                    entries.push(AppMenuEntry::item(
                        "Renomear…",
                        None,
                        false,
                        enabled,
                        MenuCommand::RenameProjectEntry(context.path.clone(), context.kind),
                    ));
                    entries.push(AppMenuEntry::item(
                        "Excluir…",
                        None,
                        false,
                        enabled,
                        MenuCommand::DeleteProjectEntry(context.path.clone(), context.kind),
                    ));
                }
            }
            project::EntryKind::TypstFile => {
                entries.push(AppMenuEntry::item(
                    "Abrir",
                    None,
                    false,
                    enabled,
                    MenuCommand::OpenProjectEntry(context.path.clone(), context.kind),
                ));
                entries.push(AppMenuEntry::Divider);
                entries.push(AppMenuEntry::item(
                    "Fixar este arquivo no Preview",
                    None,
                    self.project_main.as_deref() == Some(context.path.as_path()),
                    enabled,
                    MenuCommand::SetProjectMain(context.path.clone()),
                ));
                entries.push(AppMenuEntry::item(
                    "Acompanhar a aba ativa",
                    None,
                    self.project_main.is_none(),
                    enabled && self.project_main.is_some(),
                    MenuCommand::ClearProjectMain,
                ));
                entries.push(AppMenuEntry::Divider);
                entries.push(AppMenuEntry::item(
                    "Renomear…",
                    None,
                    false,
                    enabled,
                    MenuCommand::RenameProjectEntry(context.path.clone(), context.kind),
                ));
                entries.push(AppMenuEntry::item(
                    "Excluir…",
                    None,
                    false,
                    enabled,
                    MenuCommand::DeleteProjectEntry(context.path.clone(), context.kind),
                ));
            }
            project::EntryKind::File => {
                entries.push(AppMenuEntry::item(
                    "Renomear…",
                    None,
                    false,
                    enabled,
                    MenuCommand::RenameProjectEntry(context.path.clone(), context.kind),
                ));
                entries.push(AppMenuEntry::item(
                    "Excluir…",
                    None,
                    false,
                    enabled,
                    MenuCommand::DeleteProjectEntry(context.path.clone(), context.kind),
                ));
            }
        }

        entries.push(AppMenuEntry::Divider);
        entries.push(AppMenuEntry::item(
            "Copiar caminho",
            None,
            false,
            enabled,
            MenuCommand::CopyProjectPath(context.path.clone()),
        ));
        if !is_root {
            entries.push(AppMenuEntry::item(
                "Duplicar",
                None,
                false,
                enabled,
                MenuCommand::DuplicateProjectEntry(context.path.clone(), context.kind),
            ));
            entries.push(AppMenuEntry::item(
                "Mover para…",
                None,
                false,
                enabled,
                MenuCommand::MoveProjectEntry(context.path.clone(), context.kind),
            ));
        }

        entries.push(AppMenuEntry::Divider);
        entries.push(AppMenuEntry::item(
            "Atualizar árvore",
            None,
            false,
            enabled && !self.project_scan_busy,
            MenuCommand::RefreshProjectTree,
        ));
        entries
    }

    fn project_pane_visible(&self) -> bool {
        self.panes.iter().any(|(_, pane)| *pane == Pane::Project)
    }

    fn toggle_project_pane(&mut self) {
        let project = self
            .panes
            .iter()
            .find_map(|(id, pane)| (*pane == Pane::Project).then_some(*id));
        if let Some(project) = project {
            let _ = self.panes.close(project);
            self.file_status = Some("Painel lateral ocultado".to_owned());
            self.mark_session_changed();
            return;
        }

        let editor = self
            .panes
            .iter()
            .find_map(|(id, pane)| (*pane == Pane::Editor).then_some(*id));
        let Some(editor) = editor else {
            self.file_status = Some("Não foi possível restaurar o painel lateral".to_owned());
            return;
        };

        if let Some((project, split)) =
            self.panes
                .split(pane_grid::Axis::Vertical, editor, Pane::Project)
        {
            self.panes.swap(editor, project);
            self.panes.resize(split, 0.24);
            self.file_status = Some("Painel lateral exibido".to_owned());
            self.mark_session_changed();
        }
    }

    fn view(&self, window: window::Id) -> Element<'_, Message> {
        if self.settings_window == Some(window) {
            self.settings_window_view()
        } else {
            self.main_window_view()
        }
    }

    fn main_window_view(&self) -> Element<'_, Message> {
        let panes = pane_grid(&self.panes, |_id, pane, _is_maximized| {
            let content: Element<'_, Message> = match pane {
                Pane::Project => self.project_view(),
                Pane::Editor => self.editor_view(),
                Pane::Preview => self.preview_view(),
            };
            let title_bar: Element<'_, Message> = match pane {
                Pane::Project => self.project_pane_title_bar(),
                Pane::Editor | Pane::Preview => Space::new()
                    .height(iced::Length::Fixed(PANE_DRAG_HANDLE_HEIGHT))
                    .into(),
            };

            pane_grid::Content::new(content).title_bar(pane_grid::TitleBar::new(title_bar))
        })
        .width(Fill)
        .height(Fill)
        .spacing(0)
        .min_size(200)
        .on_drag(Message::PaneDragged)
        .on_resize(10, Message::PaneResized);

        let (error_count, warning_count) = self.diagnostic_counts();
        let status = container(
            row![
                text(self.status_text())
                    .size(13)
                    .width(Fill)
                    .wrapping(text::Wrapping::None),
                ui::problem_count_indicator(
                    ui::ProblemSeverity::Error,
                    error_count,
                    Some(Message::OpenProblems),
                ),
                ui::problem_count_indicator(
                    ui::ProblemSeverity::Warning,
                    warning_count,
                    Some(Message::OpenProblems),
                ),
            ]
            .width(Fill)
            .height(iced::Length::Fixed(
                ui::tokens::dimension::STATUS_BAR_INDICATOR_HEIGHT,
            ))
            .align_y(Alignment::Center)
            .spacing(ui::tokens::spacing::STATUS_BAR_INDICATOR_GAP),
        )
        .width(Fill)
        .height(iced::Length::Fixed(
            ui::tokens::dimension::STATUS_BAR_HEIGHT,
        ))
        .padding([0.0, ui::tokens::spacing::STATUS_BAR_EDGE_TO_CONTENT])
        .align_y(Alignment::Center)
        .style(status_bar_style);

        let content = column![self.app_bar_view(), self.text_action_bar_view()];

        let base: Element<'_, Message> = content
            .push(panes)
            .push(status)
            .width(Fill)
            .height(Fill)
            .into();
        let mut layers = Stack::new().width(Fill).height(Fill).push(base);

        if let Some(menu) = self.open_menu {
            let dismiss = column![
                Space::new().height(iced::Length::Fixed(APP_BAR_HEIGHT)),
                mouse_area(Space::new().width(Fill).height(Fill)).on_press(Message::DismissMenu),
            ]
            .width(Fill)
            .height(Fill);
            layers = layers.push(dismiss).push(self.menu_overlay(menu));
        }

        if self.export_menu_visible {
            let dismiss = column![
                Space::new().height(iced::Length::Fixed(APP_BAR_HEIGHT)),
                mouse_area(Space::new().width(Fill).height(Fill)).on_press(Message::DismissMenu),
            ]
            .width(Fill)
            .height(Fill);
            layers = layers.push(dismiss).push(self.export_menu_overlay());
        }

        if let Some(context) = self.project_context_menu.as_ref() {
            let dismiss =
                mouse_area(Space::new().width(Fill).height(Fill)).on_press(Message::DismissMenu);
            layers = layers
                .push(dismiss)
                .push(self.project_context_menu_overlay(context));
        }

        if self.about_visible {
            let backdrop = mouse_area(
                container(Space::new())
                    .width(Fill)
                    .height(Fill)
                    .style(modal_backdrop_style),
            )
            .on_press(Message::CloseAbout);
            layers = layers.push(backdrop).push(self.about_overlay());
        }

        if let Some(pending) = self.pending_alert_dialog.as_ref() {
            layers = layers.push(self.alert_dialog_view(pending));
        }

        layers.into()
    }

    fn app_bar_view(&self) -> Element<'_, Message> {
        let file = ui::menu_bar_button(
            "Arquivo",
            FILE_MENU_TRIGGER_WIDTH,
            Message::ToggleMenu(AppMenu::File),
            Message::MenuBarPointerPressed(AppMenu::File),
            Message::MenuBarPointerEntered(AppMenu::File),
            self.open_menu == Some(AppMenu::File),
        );
        let edit = ui::menu_bar_button(
            "Editar",
            EDIT_MENU_TRIGGER_WIDTH,
            Message::ToggleMenu(AppMenu::Edit),
            Message::MenuBarPointerPressed(AppMenu::Edit),
            Message::MenuBarPointerEntered(AppMenu::Edit),
            self.open_menu == Some(AppMenu::Edit),
        );
        let view = ui::menu_bar_button(
            "Exibir",
            VIEW_MENU_TRIGGER_WIDTH,
            Message::ToggleMenu(AppMenu::View),
            Message::MenuBarPointerPressed(AppMenu::View),
            Message::MenuBarPointerEntered(AppMenu::View),
            self.open_menu == Some(AppMenu::View),
        );
        let help = ui::menu_bar_button(
            "Ajuda",
            HELP_MENU_TRIGGER_WIDTH,
            Message::ToggleMenu(AppMenu::Help),
            Message::MenuBarPointerPressed(AppMenu::Help),
            Message::MenuBarPointerEntered(AppMenu::Help),
            self.open_menu == Some(AppMenu::Help),
        );
        let menus = container(
            row![file, edit, view, help]
                .align_y(Alignment::Center)
                .spacing(0),
        )
        .width(iced::Length::Fixed(APP_ACTIONS_WIDTH))
        .align_x(iced::alignment::Horizontal::Left);
        let title = if self.document.is_dirty() {
            format!("{} *", self.document.display_name())
        } else {
            self.document.display_name()
        };
        let title = container(
            text(title)
                .size(ui::tokens::typography::FONT_SIZE_100)
                .wrapping(text::Wrapping::None),
        )
        .width(Fill)
        .center_x(Fill);
        let save = ui::spectrum_button(
            "Salvar",
            (!self.file_busy && self.document.is_dirty())
                .then_some(Message::MenuCommand(MenuCommand::SaveDocument)),
            ui::ButtonOptions::ACCENT.size(ui::ButtonSize::Medium),
        );
        let can_export = !self.file_busy && self.compiler.is_some();
        let additional_exports = ui::workflow_icon_button(
            ui::WorkflowIcon::More,
            "Mais opções de exportação",
            Some(Message::ToggleExportMenu),
            ui::ButtonOptions::SECONDARY.size(ui::ButtonSize::Medium),
        );
        let export_group: Element<'_, Message> =
            ui::ButtonGroup::new(vec![ui::ButtonGroupItem::new(
                "Exportar PDF",
                can_export.then_some(Message::MenuCommand(MenuCommand::Export(
                    compiler::ExportFormat::Pdf,
                ))),
                ui::ButtonOptions::ACCENT.size(ui::ButtonSize::Medium),
            )])
            .trailing(additional_exports)
            .into();
        let actions = container(
            row![save, export_group]
                .align_y(Alignment::Center)
                .spacing(12),
        )
        .width(iced::Length::Fixed(APP_ACTIONS_WIDTH))
        .align_x(iced::alignment::Horizontal::Right);
        let bar = row![menus, title, actions]
            .align_y(Alignment::Center)
            .width(Fill)
            .height(Fill);

        container(bar)
            .width(Fill)
            .height(iced::Length::Fixed(APP_BAR_HEIGHT))
            .padding([4.0, APP_BAR_HORIZONTAL_PADDING])
            .style(app_bar_style)
            .into()
    }

    fn menu_overlay(&self, menu: AppMenu) -> Element<'_, Message> {
        let popup = menu_popup(
            self.menu_entries(menu),
            self.menu_focus,
            menu_popup_width(menu),
        );

        column![
            Space::new().height(iced::Length::Fixed(APP_BAR_HEIGHT)),
            row![
                Space::new().width(iced::Length::Fixed(menu_horizontal_offset(menu))),
                popup,
            ],
        ]
        .width(Fill)
        .height(Fill)
        .into()
    }

    fn export_menu_overlay(&self) -> Element<'_, Message> {
        let popup = menu_popup(
            self.export_menu_entries(),
            self.menu_focus,
            EXPORT_MENU_WIDTH,
        );

        column![
            Space::new().height(iced::Length::Fixed(APP_BAR_HEIGHT)),
            row![
                Space::new().width(Fill),
                popup,
                Space::new().width(iced::Length::Fixed(APP_BAR_HORIZONTAL_PADDING)),
            ],
        ]
        .width(Fill)
        .height(Fill)
        .into()
    }

    fn project_context_menu_overlay(&self, context: &ProjectContextMenu) -> Element<'_, Message> {
        let definitions = self.project_context_entries(context);
        let popup_height = menu_popup_height(&definitions);
        let position = context.position;
        let menu_focus = self.menu_focus;

        responsive(move |viewport| {
            let maximum_x =
                (viewport.width - PROJECT_CONTEXT_MENU_WIDTH - CONTEXT_MENU_VIEWPORT_MARGIN)
                    .max(CONTEXT_MENU_VIEWPORT_MARGIN);
            let maximum_y = (viewport.height - popup_height - CONTEXT_MENU_VIEWPORT_MARGIN)
                .max(CONTEXT_MENU_VIEWPORT_MARGIN);
            let x = position.x.clamp(CONTEXT_MENU_VIEWPORT_MARGIN, maximum_x);
            let y = position.y.clamp(CONTEXT_MENU_VIEWPORT_MARGIN, maximum_y);
            let popup = menu_popup(definitions.clone(), menu_focus, PROJECT_CONTEXT_MENU_WIDTH);

            column![
                Space::new().height(iced::Length::Fixed(y)),
                row![Space::new().width(iced::Length::Fixed(x)), popup],
            ]
            .width(Fill)
            .height(Fill)
            .into()
        })
        .into()
    }

    fn about_overlay(&self) -> Element<'_, Message> {
        let close = ui::spectrum_button(
            "Fechar",
            Some(Message::CloseAbout),
            ui::ButtonOptions::SECONDARY.size(ui::ButtonSize::Small),
        );
        let dialog = container(
            column![
                text("Typstation")
                    .size(ui::tokens::typography::FONT_SIZE_300)
                    .font(iced::Font {
                        weight: iced::font::Weight::Bold,
                        ..iced::Font::DEFAULT
                    }),
                text(format!("Versão {}", env!("CARGO_PKG_VERSION")))
                    .size(ui::tokens::typography::FONT_SIZE_100),
                text("Editor de documentos Typst construído com Iced.")
                    .size(ui::tokens::typography::FONT_SIZE_100),
                row![Space::new().width(Fill), close],
            ]
            .spacing(12),
        )
        .width(iced::Length::Fixed(380.0))
        .padding(24)
        .style(modal_dialog_style);

        container(dialog)
            .width(Fill)
            .height(Fill)
            .center_x(Fill)
            .center_y(Fill)
            .into()
    }

    fn alert_dialog_view(&self, pending: &PendingAlertDialog) -> Element<'_, Message> {
        match pending {
            PendingAlertDialog::Unsaved { action, name } => ui::AlertDialog::new(
                ui::AlertDialogVariant::Warning,
                format!("Salvar alterações em {name}"),
                format!(
                    "{name} contém alterações que ainda não foram salvas. Se você descartar as alterações, elas não poderão ser recuperadas."
                ),
                vec![
                    ui::AlertDialogAction::new(
                        "Cancelar",
                        Some(Message::UnsavedDecision {
                            action: *action,
                            decision: UnsavedDecision::Cancel,
                        }),
                        ui::ButtonOptions::SECONDARY,
                    ),
                    ui::AlertDialogAction::new(
                        "Descartar",
                        Some(Message::UnsavedDecision {
                            action: *action,
                            decision: UnsavedDecision::Discard,
                        }),
                        ui::ButtonOptions::NEGATIVE_OUTLINE,
                    ),
                    ui::AlertDialogAction::new(
                        "Salvar",
                        Some(Message::UnsavedDecision {
                            action: *action,
                            decision: UnsavedDecision::Save,
                        }),
                        ui::ButtonOptions::ACCENT,
                    ),
                ],
                Message::AlertDialogBlocked,
            )
            .into(),
            PendingAlertDialog::DeleteProjectEntry { path, kind } => {
                let noun = match kind {
                    project::EntryKind::Directory => "pasta",
                    project::EntryKind::TypstFile | project::EntryKind::File => "arquivo",
                };
                let name = path
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| noun.to_owned());

                ui::AlertDialog::new(
                    ui::AlertDialogVariant::Destructive,
                    format!("Excluir {noun} {name}"),
                    format!(
                        "{} será removido permanentemente do projeto. Esta ação não pode ser desfeita.",
                        path.display()
                    ),
                    vec![
                        ui::AlertDialogAction::new(
                            "Cancelar",
                            Some(Message::DismissAlertDialog),
                            ui::ButtonOptions::SECONDARY,
                        ),
                        ui::AlertDialogAction::new(
                            "Excluir",
                            Some(Message::ConfirmProjectDeletion),
                            ui::ButtonOptions::NEGATIVE,
                        ),
                    ],
                    Message::AlertDialogBlocked,
                )
                .into()
            }
        }
    }

    fn preview_view(&self) -> Element<'_, Message> {
        let scale = preview_scale(self.settings.preview_zoom, self.preview_logical_ppi);
        let controls = row![
            message_icon_button(
                "−",
                "Diminuir zoom",
                Message::PreviewZoomOut,
                self.settings.preview_zoom > 25,
            ),
            ui::action_button(
                format!("{}%", self.settings.preview_zoom),
                Some(Message::PreviewZoomReset),
                ui::ActionButtonOptions::STANDARD,
            ),
            message_icon_button(
                "+",
                "Aumentar zoom",
                Message::PreviewZoomIn,
                self.settings.preview_zoom < 300,
            ),
            message_action_button(
                "Localizar",
                Message::RevealInPreview,
                self.preview_navigation_ready(),
            ),
            text(format!("Principal: {}", self.compilation_display_name())).size(13),
            text(format!("{} página(s)", self.preview.len())).size(13),
        ]
        .align_y(Alignment::Center)
        .spacing(4)
        .padding([4, 8]);
        let content: Element<'_, Message> = if self.preview.is_empty() {
            container(text("Preview indisponível"))
                .center_x(Fill)
                .center_y(Fill)
                .into()
        } else {
            responsive(move |viewport| self.preview_canvas_view(scale, viewport.width)).into()
        };

        column![controls, content].width(Fill).height(Fill).into()
    }

    fn preview_canvas_view(&self, scale: f32, viewport_width: f32) -> Element<'_, Message> {
        let maximum_page_width = self
            .preview
            .iter()
            .map(|page| (page.width * scale).max(1.0))
            .fold(0.0_f32, f32::max);
        let canvas_width = preview_canvas_width(viewport_width, maximum_page_width);
        let mut pages = column![]
            .align_x(Alignment::Center)
            .spacing(PREVIEW_PAGE_SPACING)
            .width(Fill);

        for (index, page) in self.preview.iter().enumerate() {
            let width = (page.width * scale).max(1.0);
            let height = (page.height * scale).max(1.0);
            let mut page_layers = Stack::new()
                .width(iced::Length::Fixed(width))
                .height(iced::Length::Fixed(height))
                .push(
                    svg(page.handle.clone())
                        .width(iced::Length::Fixed(width))
                        .height(iced::Length::Fixed(height)),
                );

            if let Some(highlight) = self
                .preview_highlight
                .filter(|highlight| highlight.page == index)
            {
                let bounds = highlight.bounds;
                let x = (bounds.x * scale).clamp(0.0, width);
                let y = (bounds.y * scale).clamp(0.0, height);
                let marker_width = (bounds.width * scale).max(2.0).min((width - x).max(0.0));
                let marker_height = (bounds.height * scale).max(2.0).min((height - y).max(0.0));
                let marker = container(Space::new())
                    .width(iced::Length::Fixed(marker_width))
                    .height(iced::Length::Fixed(marker_height))
                    .style(|_theme: &Theme| {
                        iced::widget::container::Style::default()
                            .background(Color::from_rgba8(0x18, 0x9A, 0xD3, 0.20))
                            .border(
                                Border::default()
                                    .color(Color::from_rgb8(0x00, 0x70, 0xA8))
                                    .width(1),
                            )
                    });
                let highlight_layer: Element<'_, Message> = column![
                    Space::new().height(iced::Length::Fixed(y)),
                    row![Space::new().width(iced::Length::Fixed(x)), marker],
                ]
                .width(iced::Length::Fixed(width))
                .height(iced::Length::Fixed(height))
                .into();
                page_layers = page_layers.push(highlight_layer);
            }

            let interactive_page = mouse_area(page_layers)
                .on_move(move |position| Message::PreviewPointerMoved {
                    page: index,
                    position,
                })
                .on_exit(Message::PreviewPointerLeft(index))
                .on_press(Message::PreviewClicked(index))
                .interaction(mouse::Interaction::Pointer);
            pages = pages.push(
                column![
                    container(text(format!("Página {}", index + 1)).size(12))
                        .height(iced::Length::Fixed(PREVIEW_LABEL_HEIGHT))
                        .center_y(iced::Length::Fill),
                    interactive_page,
                ]
                .align_x(Alignment::Center)
                .spacing(PREVIEW_LABEL_SPACING),
            );
        }

        scrollable(
            container(pages)
                .width(iced::Length::Fixed(canvas_width))
                .align_x(Alignment::Center)
                .padding(PREVIEW_PADDING),
        )
        .id(self.preview_scroll_id.clone())
        .direction(scrollable::Direction::Both {
            vertical: scrollable::Scrollbar::default(),
            horizontal: scrollable::Scrollbar::default(),
        })
        .width(Fill)
        .height(Fill)
        .into()
    }

    fn settings_window_view(&self) -> Element<'_, Message> {
        let heading = container(
            row![
                svg(ui::WorkflowIcon::Settings.handle())
                    .width(iced::Length::Fixed(ui::tokens::icon::WORKFLOW_SIZE_100))
                    .height(iced::Length::Fixed(ui::tokens::icon::WORKFLOW_SIZE_100))
                    .style(settings_icon_style),
                text("Configurações")
                    .size(ui::tokens::typography::FONT_SIZE_300)
                    .font(iced::Font {
                        weight: iced::font::Weight::Bold,
                        ..iced::Font::DEFAULT
                    }),
            ]
            .align_y(Alignment::Center)
            .spacing(12),
        )
        .width(Fill)
        .height(iced::Length::Fixed(56.0))
        .padding([0, 24])
        .align_y(Alignment::Center)
        .style(settings_band_style);

        let tab_width = settings_slider_row(
            "Largura da tabulação",
            format!("{} espaços", self.settings.tab_width),
            slider(
                1..=8,
                self.settings.tab_width as u16,
                Message::TabWidthChanged,
            )
            .width(Fill),
        );
        let editor_font_size = settings_slider_row(
            "Tamanho da fonte",
            format!("{} px", self.settings.editor_font_size),
            slider(
                10..=30,
                self.settings.editor_font_size,
                Message::EditorFontSizeChanged,
            )
            .width(Fill),
        );
        let preview_zoom = settings_slider_row(
            "Zoom padrão",
            format!("{}%", self.settings.preview_zoom),
            slider(
                25..=300,
                self.settings.preview_zoom,
                Message::PreviewZoomChanged,
            )
            .step(5u16)
            .width(Fill),
        );
        let svg_page_gap = settings_slider_row(
            "Espaço entre páginas",
            format!("{} pt", self.settings.svg_page_gap),
            slider(
                0..=72,
                self.settings.svg_page_gap,
                Message::SvgPageGapChanged,
            )
            .width(Fill),
        );

        let page: Element<'_, Message> = match self.settings_page {
            SettingsPage::Editor => column![
                tab_width,
                editor_font_size,
                ui::spectrum_checkbox(
                    "Fechar pares automaticamente",
                    self.settings.auto_pairs,
                    Message::AutoPairsChanged,
                ),
                ui::spectrum_checkbox(
                    "Aplicar indentação automática",
                    self.settings.auto_indent,
                    Message::AutoIndentChanged,
                ),
                ui::spectrum_checkbox(
                    "Quebrar linhas longas",
                    self.settings.wrap_lines,
                    Message::WrapLinesChanged,
                ),
                ui::spectrum_checkbox(
                    "Mostrar números de linha",
                    self.settings.show_gutter,
                    Message::ShowGutterChanged,
                ),
                ui::spectrum_checkbox(
                    "Salvar automaticamente arquivos já nomeados",
                    self.settings.auto_save,
                    Message::AutoSaveChanged,
                ),
            ]
            .spacing(16)
            .into(),
            SettingsPage::Preview => column![preview_zoom].spacing(16).into(),
            SettingsPage::Export => column![
                settings_subsection_title("PDF"),
                ui::spectrum_checkbox(
                    "Incluir marcação de acessibilidade",
                    self.settings.pdf_tagged,
                    Message::PdfTaggedChanged,
                ),
                ui::spectrum_checkbox(
                    "Formatar arquivo para inspeção",
                    self.settings.pdf_pretty,
                    Message::PdfPrettyChanged,
                ),
                settings_subsection_title("SVG"),
                ui::spectrum_checkbox(
                    "Renderizar área de sangria",
                    self.settings.svg_render_bleed,
                    Message::SvgRenderBleedChanged,
                ),
                ui::spectrum_checkbox(
                    "Formatar arquivo para inspeção",
                    self.settings.svg_pretty,
                    Message::SvgPrettyChanged,
                ),
                svg_page_gap,
                settings_subsection_title("HTML (experimental)"),
                ui::spectrum_checkbox(
                    "Formatar arquivo para inspeção",
                    self.settings.html_pretty,
                    Message::HtmlPrettyChanged,
                ),
            ]
            .spacing(16)
            .into(),
            SettingsPage::Appearance => column![ui::spectrum_checkbox(
                "Usar tema claro",
                self.settings.theme == settings::ThemeMode::Light,
                Message::LightThemeChanged,
            )]
            .spacing(16)
            .into(),
        };
        let panel = scrollable(container(page).width(Fill).padding([24, 32]))
            .width(Fill)
            .height(Fill);
        let tabs = SettingsPage::ALL
            .into_iter()
            .map(|page| {
                ui::TabItem::new(
                    page.title(),
                    Some(Message::SettingsPageSelected(page)),
                    None,
                )
                .selected(page == self.settings_page)
            })
            .collect();
        let body: Element<'_, Message> = ui::Tabs::new(tabs, panel).into();
        let footer = container(row![
            Space::new().width(Fill),
            ui::spectrum_button(
                "Fechar",
                Some(Message::CloseSettingsWindow),
                ui::ButtonOptions::SECONDARY,
            ),
        ])
        .width(Fill)
        .height(iced::Length::Fixed(64.0))
        .padding([16, 24])
        .align_y(Alignment::Center)
        .style(settings_band_style);

        container(column![heading, body, footer])
            .width(Fill)
            .height(Fill)
            .style(settings_window_background_style)
            .into()
    }

    fn text_action_bar_view(&self) -> Element<'_, Message> {
        let can_edit = !self.file_busy;
        let options = ui::ActionButtonOptions::STANDARD
            .size(ui::ActionButtonSize::Medium)
            .emphasized(true);
        let history = ui::compact_action_group(
            [
                ui::ActionGroupItem::symbol(
                    "↶",
                    "Desfazer",
                    can_edit.then_some(Message::Editor(Action::Undo)),
                ),
                ui::ActionGroupItem::symbol(
                    "↷",
                    "Refazer",
                    can_edit.then_some(Message::Editor(Action::Redo)),
                ),
            ],
            options,
        );
        let text_style = ui::compact_action_group(
            [
                ui::ActionGroupItem::workflow(
                    ui::WorkflowIcon::TextBold,
                    "Negrito",
                    can_edit.then_some(Message::Bold),
                ),
                ui::ActionGroupItem::workflow(
                    ui::WorkflowIcon::TextItalic,
                    "Itálico",
                    can_edit.then_some(Message::Italic),
                ),
                ui::ActionGroupItem::workflow(
                    ui::WorkflowIcon::TextUnderline,
                    "Sublinhado",
                    can_edit.then_some(Message::Underline),
                ),
            ],
            options,
        );
        let lists = ui::compact_action_group(
            [
                ui::ActionGroupItem::workflow(
                    ui::WorkflowIcon::TextBulleted,
                    "Lista com marcadores",
                    can_edit.then_some(Message::PrefixLines("- ".into())),
                ),
                ui::ActionGroupItem::workflow(
                    ui::WorkflowIcon::TextNumbered,
                    "Lista numerada",
                    can_edit.then_some(Message::PrefixLines("+ ".into())),
                ),
            ],
            options,
        );
        let comment = ui::icon_action_button(
            "//",
            "Alternar comentário de linha",
            can_edit.then_some(Message::Editor(Action::ToggleLineComment)),
            options,
        );

        container(
            row![history, text_style, lists, comment]
                .align_y(Alignment::Center)
                .spacing(ui::tokens::spacing::BASE_GAP_MEDIUM),
        )
        .width(Fill)
        .height(iced::Length::Fixed(APP_BAR_HEIGHT))
        .padding([4, 8])
        .style(action_bar_style)
        .into()
    }

    fn editor_view(&self) -> Element<'_, Message> {
        let active = self.document.active_id();
        let mut tabs = Vec::new();

        for (id, document) in self.document.iter() {
            let mut label = document.display_name();
            if self
                .project_main
                .as_deref()
                .is_some_and(|main| document.path() == Some(main))
            {
                label.push_str(" [principal]");
            }
            if document.is_dirty() {
                label.push_str(" *");
            }
            if document.external_change().is_some() {
                label.push_str(" !");
            }

            tabs.push(
                ui::TabItem::new(
                    label,
                    (!self.file_busy).then_some(Message::ActivateDocument(id)),
                    (!self.file_busy).then_some(Message::CloseDocument(id)),
                )
                .selected(id == active),
            );
        }

        let editor: Element<'_, Message> = code_editor(self.document.content())
            .on_action(Message::Editor)
            .wrap(self.settings.wrap_lines)
            .gutter(self.settings.show_gutter)
            .size(f32::from(self.settings.editor_font_size))
            .into();
        let mut panel = column![].width(Fill).height(Fill);

        if self.document.external_change().is_some() {
            panel = panel.push(self.external_change_view());
        }

        if self.search.visible {
            panel = panel.push(self.search_view());
        }

        panel = panel.push(editor);

        ui::Tabs::new(tabs, panel).into()
    }

    fn project_view(&self) -> Element<'_, Message> {
        let (error_count, warning_count) = self.diagnostic_counts();
        let problems_notification = if error_count > 0 {
            Some(ui::SideNavigationNotification::Error)
        } else if warning_count > 0 {
            Some(ui::SideNavigationNotification::Warning)
        } else {
            None
        };
        let navigation: Element<'_, Message> = ui::SideNavigation::new(vec![
            ui::SideNavigationItem::new(
                "Arquivos",
                ui::WorkflowIcon::FolderOpen,
                Some(Message::ProjectNavigationSelected(ProjectNavigation::Files)),
            )
            .selected(self.project_navigation == ProjectNavigation::Files),
            ui::SideNavigationItem::new(
                "Buscar no projeto",
                ui::WorkflowIcon::Search,
                Some(Message::ProjectNavigationSelected(
                    ProjectNavigation::Search,
                )),
            )
            .selected(self.project_navigation == ProjectNavigation::Search),
            ui::SideNavigationItem::new(
                "Sumário",
                ui::WorkflowIcon::TextBulleted,
                Some(Message::ProjectNavigationSelected(
                    ProjectNavigation::Topics,
                )),
            )
            .selected(self.project_navigation == ProjectNavigation::Topics),
            ui::SideNavigationItem::new(
                problems_navigation_label(error_count, warning_count),
                ui::WorkflowIcon::AlertCircleFilled,
                Some(Message::ProjectNavigationSelected(
                    ProjectNavigation::Problems,
                )),
            )
            .selected(self.project_navigation == ProjectNavigation::Problems)
            .notification(problems_notification),
        ])
        .into();
        let divider = container(Space::new())
            .width(iced::Length::Fixed(1.0))
            .height(Fill)
            .style(project_navigation_divider_style);
        let panel = match self.project_navigation {
            ProjectNavigation::Files => self.project_files_view(),
            ProjectNavigation::Search => self.project_search_view(),
            ProjectNavigation::Topics => self.document_outline_view(),
            ProjectNavigation::Problems => self.problems_view(),
        };

        row![navigation, divider, panel]
            .width(Fill)
            .height(Fill)
            .into()
    }

    fn project_pane_title_bar(&self) -> Element<'_, Message> {
        let title = text(self.project_navigation.title())
            .size(ui::tokens::typography::FONT_SIZE_100)
            .font(iced::Font {
                weight: iced::font::Weight::Bold,
                ..iced::Font::DEFAULT
            });
        let content = row![
            Space::new().width(iced::Length::Fixed(
                ui::tokens::dimension::SIDE_NAVIGATION_RAIL_WIDTH + 1.0
            )),
            container(title)
                .width(Fill)
                .height(iced::Length::Fixed(
                    ui::tokens::dimension::COMPONENT_HEIGHT_100
                ))
                .padding([0.0, ui::tokens::spacing::BASE_PADDING_HORIZONTAL_MEDIUM])
                .align_y(Alignment::Center),
        ]
        .width(Fill)
        .height(iced::Length::Fixed(
            ui::tokens::dimension::COMPONENT_HEIGHT_100,
        ));

        container(content)
            .width(Fill)
            .height(iced::Length::Fixed(
                ui::tokens::dimension::COMPONENT_HEIGHT_100,
            ))
            .style(project_pane_title_style)
            .into()
    }

    fn project_files_view(&self) -> Element<'_, Message> {
        let tree: Element<'_, Message> =
            ui::TreeView::new(vec![self.project_tree_root_item()]).into();

        container(scrollable(tree)).width(Fill).height(Fill).into()
    }

    fn project_search_view(&self) -> Element<'_, Message> {
        let query = ui::spectrum_text_field(
            "Buscar em todos os arquivos",
            &self.project_search.query,
            Message::ProjectSearchQueryChanged,
            None,
            Fill,
        );
        let case_sensitive = ui::icon_action_button(
            "Aa",
            "Diferenciar maiúsculas de minúsculas",
            Some(Message::ProjectSearchCaseChanged(
                !self.project_search.case_sensitive,
            )),
            ui::ActionButtonOptions::QUIET.selected(self.project_search.case_sensitive),
        );
        let whole_word = ui::icon_action_button(
            "ab",
            "Corresponder palavra inteira",
            Some(Message::ProjectSearchWholeWordChanged(
                !self.project_search.whole_word,
            )),
            ui::ActionButtonOptions::QUIET.selected(self.project_search.whole_word),
        );
        let replace_toggle = ui::workflow_icon_action_button(
            ui::WorkflowIcon::FindAndReplace,
            "Mostrar substituição no projeto",
            Some(Message::ProjectSearchToggleReplace),
            ui::ActionButtonOptions::QUIET.selected(self.project_search.replace_visible),
        );
        let summary = if self.project_search.busy {
            "Buscando...".to_owned()
        } else if let Some(error) = self.project_search.error.as_deref() {
            format!("Erro: {}", truncate(error, 100))
        } else {
            let count = self.project_search.results.len();
            let skipped = self.project_search.skipped_files;
            if skipped == 0 {
                format!("{count} resultado(s)")
            } else {
                format!("{count} resultado(s), {skipped} arquivo(s) ignorado(s)")
            }
        };
        let mut header = column![
            query,
            row![case_sensitive, whole_word, replace_toggle]
                .align_y(Alignment::Center)
                .spacing(ui::tokens::spacing::SEARCH_PANEL_CONTROL_GAP),
        ]
        .spacing(ui::tokens::spacing::SEARCH_PANEL_ROW_GAP);

        if self.project_search.replace_visible {
            let replacement = ui::spectrum_text_field(
                "Substituir por",
                &self.project_search.replacement,
                Message::ProjectSearchReplacementChanged,
                None,
                Fill,
            );
            let replace = ui::spectrum_button(
                "Substituir todos",
                (!self.file_busy && !self.project_search.results.is_empty())
                    .then_some(Message::ProjectReplaceAll),
                ui::ButtonOptions::PRIMARY,
            );
            header = header
                .push(replacement)
                .push(row![Space::new().width(Fill), replace]);
        }

        let header = container(header)
            .width(Fill)
            .padding(ui::tokens::spacing::SEARCH_PANEL_EDGE_TO_CONTENT)
            .style(search_panel_style);
        let mut results = column![
            container(
                text(summary)
                    .size(ui::tokens::typography::FONT_SIZE_75)
                    .style(search_metadata_text_style),
            )
            .width(Fill)
            .padding([8, 10])
        ];

        for found in &self.project_search.results {
            let path = found
                .path
                .strip_prefix(&self.workspace_root)
                .unwrap_or(&found.path)
                .to_string_lossy();
            let location = format!("{path}:{}:{}", found.line, found.column);
            let content = column![
                text(location)
                    .size(ui::tokens::typography::FONT_SIZE_75)
                    .wrapping(text::Wrapping::None),
                text(found.excerpt.clone())
                    .size(ui::tokens::typography::FONT_SIZE_75)
                    .wrapping(text::Wrapping::None),
            ]
            .spacing(2);
            results = results.push(
                iced::widget::button(content)
                    .on_press(Message::ProjectSearchResultPressed(
                        found.path.clone(),
                        found.range.clone(),
                    ))
                    .width(Fill)
                    .padding([7, 10])
                    .style(project_search_result_style),
            );
        }

        column![header, scrollable(results).width(Fill).height(Fill)]
            .width(Fill)
            .height(Fill)
            .into()
    }

    fn problems_view(&self) -> Element<'_, Message> {
        ui::Problems::new(self.problem_items())
            .show_header(false)
            .height(Fill)
            .into()
    }

    fn diagnostic_counts(&self) -> (usize, usize) {
        self.diagnostics
            .iter()
            .fold((0, 0), |(errors, warnings), diagnostic| {
                match diagnostic.severity {
                    compiler::DiagnosticSeverity::Error => (errors + 1, warnings),
                    compiler::DiagnosticSeverity::Warning => (errors, warnings + 1),
                }
            })
    }

    fn problem_items(&self) -> Vec<ui::ProblemItem<Message>> {
        self.diagnostics
            .iter()
            .map(|diagnostic| {
                let severity = match diagnostic.severity {
                    compiler::DiagnosticSeverity::Error => ui::ProblemSeverity::Error,
                    compiler::DiagnosticSeverity::Warning => ui::ProblemSeverity::Warning,
                };
                let source = match &diagnostic.target {
                    compiler::DiagnosticTarget::Main => self.compilation_display_name(),
                    compiler::DiagnosticTarget::ProjectFile(path) => path
                        .strip_prefix(&self.workspace_root)
                        .unwrap_or(path)
                        .to_string_lossy()
                        .into_owned(),
                };

                ui::ProblemItem::new(
                    severity,
                    diagnostic.message.clone(),
                    source,
                    Some(Message::OpenDiagnostic(
                        diagnostic.target.clone(),
                        diagnostic.range.clone(),
                    )),
                )
            })
            .collect()
    }

    fn document_outline_view(&self) -> Element<'_, Message> {
        let header = container(
            text(self.compilation_display_name())
                .size(ui::tokens::typography::FONT_SIZE_75)
                .wrapping(text::Wrapping::None),
        )
        .width(Fill)
        .padding([10, 8]);
        let body: Element<'_, Message> = if self.document_outline.is_empty() {
            let status = match self.preview_status {
                PreviewStatus::Waiting | PreviewStatus::Compiling => {
                    "Aguardando a compilação do documento"
                }
                PreviewStatus::Ready { .. } => "O documento não possui tópicos",
                PreviewStatus::Failed { .. } => "Nenhum tópico disponível",
            };

            container(text(status).size(ui::tokens::typography::FONT_SIZE_75))
                .width(Fill)
                .padding([12, 10])
                .into()
        } else {
            let selected = self.current_outline_key();
            let tree: Element<'_, Message> = ui::TreeView::new(
                self.document_outline_items(&self.document_outline, selected.as_ref()),
            )
            .reserve_icon_space(false)
            .into();

            scrollable(tree).width(Fill).height(Fill).into()
        };

        column![header, body].width(Fill).height(Fill).into()
    }

    fn document_outline_items(
        &self,
        entries: &[compiler::DocumentOutlineItem],
        selected: Option<&OutlineKey>,
    ) -> Vec<ui::TreeViewItem<Message>> {
        entries
            .iter()
            .map(|entry| {
                let key = OutlineKey::new(entry.target.clone(), entry.range.start);
                let has_children = !entry.children.is_empty();
                let expanded = has_children && !self.collapsed_outline_entries.contains(&key);
                let children = if expanded {
                    self.document_outline_items(&entry.children, selected)
                } else {
                    Vec::new()
                };

                ui::TreeViewItem::new(
                    entry.title.clone(),
                    None,
                    Some(Message::DocumentOutlinePressed {
                        target: entry.target.clone(),
                        range: entry.range.clone(),
                        has_children,
                    }),
                )
                .expanded(expanded)
                .selected(selected == Some(&key))
                .has_children(has_children)
                .children(children)
            })
            .collect()
    }

    fn current_outline_key(&self) -> Option<OutlineKey> {
        let target = self.active_source_target()?;
        let cursor = self.document.cursor_offset();
        let mut selected = None;

        find_current_outline_key(&self.document_outline, &target, cursor, &mut selected);
        selected
    }

    fn project_tree_root_item(&self) -> ui::TreeViewItem<Message> {
        let expanded = self
            .expanded_project_directories
            .contains(&self.workspace_root);
        let selected = self.selected_project_entry.as_deref() == Some(&self.workspace_root);
        let on_press = (!self.file_busy).then(|| {
            Message::ProjectEntryPressed(self.workspace_root.clone(), project::EntryKind::Directory)
        });
        let children = if !expanded {
            Vec::new()
        } else if self.project_tree.is_empty() {
            let status = if self.project_scan_busy {
                "Examinando a pasta do projeto..."
            } else {
                "A pasta do projeto está vazia"
            };
            vec![ui::TreeViewItem::new(status, None, None)]
        } else {
            self.project_tree_items(&self.project_tree)
        };

        ui::TreeViewItem::new(
            project_display_name(&self.workspace_root).to_uppercase(),
            None,
            on_press,
        )
        .reserve_icon_space(false)
        .expanded(expanded)
        .selected(selected)
        .on_context_menu(Message::ProjectEntryContextRequested(
            self.workspace_root.clone(),
            project::EntryKind::Directory,
        ))
        .actions(vec![
            ui::TreeViewAction::new(
                ui::WorkflowIcon::FileAdd,
                "Novo arquivo Typst",
                (!self.file_busy).then_some(Message::CreateProjectFile),
            ),
            ui::TreeViewAction::new(
                ui::WorkflowIcon::FolderAdd,
                "Nova pasta",
                (!self.file_busy).then_some(Message::CreateProjectDirectory),
            ),
            ui::TreeViewAction::new(
                ui::WorkflowIcon::Refresh,
                "Atualizar árvore do projeto",
                (!self.file_busy && !self.project_scan_busy).then_some(Message::RefreshProjectTree),
            ),
        ])
        .has_children(true)
        .children(children)
    }

    fn project_tree_items(
        &self,
        entries: &[project::ProjectEntry],
    ) -> Vec<ui::TreeViewItem<Message>> {
        entries
            .iter()
            .map(|entry| {
                let label = entry
                    .path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned();
                let expanded = entry.kind == project::EntryKind::Directory
                    && self.expanded_project_directories.contains(&entry.path);
                let icon = match entry.kind {
                    project::EntryKind::Directory if expanded => ui::WorkflowIcon::FolderOpen,
                    project::EntryKind::Directory => ui::WorkflowIcon::Folder,
                    project::EntryKind::TypstFile => ui::WorkflowIcon::FileCode,
                    project::EntryKind::File => ui::WorkflowIcon::Document,
                };
                let selected = self.selected_project_entry.as_deref() == Some(entry.path.as_path());
                let on_press = (!self.file_busy)
                    .then(|| Message::ProjectEntryPressed(entry.path.clone(), entry.kind));
                let children = if expanded {
                    self.project_tree_items(&entry.children)
                } else {
                    Vec::new()
                };
                let mut item = ui::TreeViewItem::new(label, Some(icon), on_press)
                    .expanded(expanded)
                    .selected(selected)
                    .has_children(!entry.children.is_empty())
                    .children(children)
                    .on_context_menu(Message::ProjectEntryContextRequested(
                        entry.path.clone(),
                        entry.kind,
                    ));

                if self.preview_project_path() == Some(entry.path.as_path()) {
                    item = item.status_icon(ui::WorkflowIcon::Visibility, "Exibido no Preview");
                }

                item
            })
            .collect()
    }

    fn preview_project_path(&self) -> Option<&Path> {
        self.project_main.as_deref().or_else(|| {
            self.document
                .path()
                .filter(|path| path.starts_with(&self.workspace_root))
        })
    }

    fn project_entry_kind(&self, path: &Path) -> Option<project::EntryKind> {
        if path == self.workspace_root {
            Some(project::EntryKind::Directory)
        } else {
            find_project_entry_kind(&self.project_tree, path)
        }
    }

    fn selected_project_target_directory(&self) -> PathBuf {
        let Some(path) = self.selected_project_entry.as_deref() else {
            return self.workspace_root.clone();
        };

        match self.project_entry_kind(path) {
            Some(project::EntryKind::Directory) => path.to_path_buf(),
            Some(project::EntryKind::TypstFile | project::EntryKind::File) => path
                .parent()
                .filter(|parent| parent.starts_with(&self.workspace_root))
                .unwrap_or(&self.workspace_root)
                .to_path_buf(),
            None => self.workspace_root.clone(),
        }
    }

    fn external_change_view(&self) -> Element<'_, Message> {
        let kind = self.document.external_change();
        let message = match kind {
            Some(ExternalChangeKind::Modified) => "O arquivo foi alterado fora do Typstation",
            Some(ExternalChangeKind::Deleted) => "O arquivo foi removido fora do Typstation",
            None => "",
        };
        let mut actions = row![text(message)].spacing(8).push(ui::spectrum_button(
            "Manter local",
            Some(Message::KeepLocalAfterExternal),
            ui::ButtonOptions::SECONDARY,
        ));

        if kind == Some(ExternalChangeKind::Modified) {
            actions = actions.push(ui::spectrum_button(
                "Recarregar",
                Some(Message::ReloadExternal),
                ui::ButtonOptions::PRIMARY,
            ));
        } else {
            actions = actions.push(ui::spectrum_button(
                "Fechar aba",
                Some(Message::CloseDocument(self.document.active_id())),
                ui::ButtonOptions::NEGATIVE,
            ));
        }

        container(actions).width(Fill).padding([4, 8]).into()
    }

    fn search_view(&self) -> Element<'_, Message> {
        responsive(|size| self.search_view_for_width(size.width)).into()
    }

    fn search_view_for_width(&self, width: f32) -> Element<'_, Message> {
        let matches = self.document.search_matches();
        let current = self
            .document
            .current_search_match()
            .filter(|index| *index < matches.len())
            .map_or(0, |index| index + 1);
        let count_label = if matches.is_empty() {
            "0 resultados".to_owned()
        } else {
            format!("{current} de {}", matches.len())
        };
        let has_matches = !matches.is_empty();
        let query = ui::search_field(
            "Buscar no documento",
            &self.search.query,
            self.search_input_id.clone(),
            Message::SearchQueryChanged,
            Message::SearchNext,
            Some(Message::SearchQueryChanged(String::new())),
            Fill,
        );
        let result = container(
            text(count_label)
                .size(ui::tokens::typography::FONT_SIZE_75)
                .style(search_metadata_text_style),
        )
        .width(iced::Length::Fixed(82.0))
        .align_x(Alignment::Center);
        let previous = ui::workflow_icon_action_button(
            ui::WorkflowIcon::ChevronUp,
            "Resultado anterior",
            has_matches.then_some(Message::SearchPrevious),
            ui::ActionButtonOptions::QUIET,
        );
        let next = ui::workflow_icon_action_button(
            ui::WorkflowIcon::ChevronDown,
            "Próximo resultado",
            has_matches.then_some(Message::SearchNext),
            ui::ActionButtonOptions::QUIET,
        );
        let case_sensitive = ui::icon_action_button(
            "Aa",
            "Diferenciar maiúsculas de minúsculas",
            Some(Message::SearchCaseChanged(!self.search.case_sensitive)),
            ui::ActionButtonOptions::QUIET.selected(self.search.case_sensitive),
        );
        let whole_word = ui::icon_action_button(
            "ab",
            "Corresponder palavra inteira",
            Some(Message::SearchWholeWordChanged(!self.search.whole_word)),
            ui::ActionButtonOptions::QUIET.selected(self.search.whole_word),
        );
        let replace_toggle = ui::workflow_icon_action_button(
            ui::WorkflowIcon::FindAndReplace,
            "Mostrar substituição",
            Some(Message::ToggleReplace),
            ui::ActionButtonOptions::QUIET.selected(self.search.replace_visible),
        );
        let close = ui::workflow_icon_action_button(
            ui::WorkflowIcon::Close,
            "Fechar busca",
            Some(Message::CloseSearch),
            ui::ActionButtonOptions::QUIET,
        );

        let controls = row![
            result,
            previous,
            next,
            case_sensitive,
            whole_word,
            replace_toggle,
        ]
        .align_y(Alignment::Center)
        .spacing(ui::tokens::spacing::SEARCH_PANEL_CONTROL_GAP);
        let mut panel = if width >= 680.0 {
            column![
                row![query, controls, close]
                    .align_y(Alignment::Center)
                    .spacing(ui::tokens::spacing::SEARCH_PANEL_CONTROL_GAP)
            ]
        } else {
            let controls = scrollable(controls)
                .direction(scrollable::Direction::Horizontal(
                    scrollable::Scrollbar::default(),
                ))
                .width(Fill)
                .height(iced::Length::Fixed(40.0));
            column![
                row![query, close]
                    .align_y(Alignment::Center)
                    .spacing(ui::tokens::spacing::SEARCH_PANEL_CONTROL_GAP),
                controls,
            ]
        }
        .spacing(ui::tokens::spacing::SEARCH_PANEL_ROW_GAP);

        if self.search.replace_visible {
            let replacement = ui::spectrum_text_field(
                "Substituir por",
                &self.search.replacement,
                Message::SearchReplacementChanged,
                Some(Message::ReplaceCurrent),
                Fill,
            );
            let actions = row![
                ui::spectrum_button(
                    "Substituir",
                    has_matches.then_some(Message::ReplaceCurrent),
                    ui::ButtonOptions::PRIMARY,
                ),
                ui::spectrum_button(
                    "Substituir todos",
                    has_matches.then_some(Message::ReplaceAll),
                    ui::ButtonOptions::SECONDARY,
                ),
            ]
            .align_y(Alignment::Center)
            .spacing(ui::tokens::spacing::SEARCH_PANEL_CONTROL_GAP);
            panel = if width >= 520.0 {
                panel.push(
                    row![replacement, actions]
                        .align_y(Alignment::Center)
                        .spacing(ui::tokens::spacing::SEARCH_PANEL_CONTROL_GAP),
                )
            } else {
                panel
                    .push(replacement)
                    .push(row![Space::new().width(Fill), actions])
            };
        }

        container(panel)
            .width(Fill)
            .padding(ui::tokens::spacing::SEARCH_PANEL_EDGE_TO_CONTENT)
            .style(search_panel_style)
            .into()
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

            self.pending_alert_dialog = Some(PendingAlertDialog::Unsaved { action, name });
            Task::none()
        } else {
            self.execute_destructive_action(action)
        }
    }

    fn dismiss_alert_dialog(&mut self) -> Task<Message> {
        let Some(pending) = self.pending_alert_dialog.take() else {
            return Task::none();
        };

        match pending {
            PendingAlertDialog::Unsaved { action, .. } => self.update(Message::UnsavedDecision {
                action,
                decision: UnsavedDecision::Cancel,
            }),
            PendingAlertDialog::DeleteProjectEntry { .. } => {
                self.file_busy = false;
                self.file_status = Some("Exclusão cancelada".to_owned());
                Task::none()
            }
        }
    }

    fn confirm_project_deletion(&mut self) -> Task<Message> {
        let Some(PendingAlertDialog::DeleteProjectEntry { path, kind }) =
            self.pending_alert_dialog.take()
        else {
            return Task::none();
        };

        self.file_status = Some(format!("Excluindo {}...", path.display()));
        Task::perform(
            project::delete_entry(self.workspace_root.clone(), path, kind),
            Message::ProjectOperationFinished,
        )
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
        let project_path = self
            .document
            .path()
            .filter(|path| path.starts_with(&self.workspace_root))
            .map(Path::to_path_buf);
        if let Some(path) = project_path {
            self.reveal_project_entry(&path);
            self.selected_project_file = Some(path);
        }
        if self.project_main.is_some() {
            self.active_document_replaced();
        } else {
            self.document_replaced(previous_config);
        }
        true
    }

    fn activate_relative_document(&mut self, reverse: bool) -> bool {
        let previous_config = self.compiler_config();
        self.document.clear_search_matches();
        if !self.document.activate_relative(reverse) {
            return false;
        }

        if let Some(path) = self
            .document
            .path()
            .filter(|path| path.starts_with(&self.workspace_root))
            .map(Path::to_path_buf)
        {
            self.reveal_project_entry(&path);
            self.selected_project_file = Some(path);
        }
        if self.project_main.is_some() {
            self.active_document_replaced();
        } else {
            self.document_replaced(previous_config);
        }
        true
    }

    fn reopen_closed_document(&mut self) {
        let Some(stored) = self.closed_documents.pop() else {
            self.file_status = Some("Não há abas fechadas para reabrir".to_owned());
            return;
        };
        let previous_config = self.compiler_config();
        let reopened = Document::restored(stored.path, stored.text, stored.saved_text);
        let replace_blank = self.document.len() == 1
            && self.document.path().is_none()
            && !self.document.is_dirty()
            && self.document.snapshot().1.is_empty();
        if replace_blank {
            *self.document.active_mut() = reopened;
        } else {
            self.document.add(reopened);
        }
        self.apply_editor_settings();
        self.file_status = Some(format!("Aba reaberta: {}", self.document.display_name()));
        if self.project_main.is_some() {
            self.active_document_replaced();
        } else {
            self.document_replaced(previous_config);
        }
    }

    fn new_document(&mut self) {
        let previous_config = self.compiler_config();
        self.document.clear_search_matches();
        self.document.add(Document::new());
        self.file_status = Some("Novo documento criado".to_owned());
        if self.project_main.is_some() {
            self.active_document_replaced();
        } else {
            self.document_replaced(previous_config);
        }
    }

    fn close_document(&mut self, id: DocumentId) {
        let Some(document) = self.document.get(id) else {
            return;
        };
        let name = document.display_name();
        let was_active = self.document.active_id() == id;
        let affects_fixed_preview = self.project_main.is_some()
            && document.is_dirty()
            && document
                .path()
                .is_some_and(|path| path.starts_with(&self.workspace_root));
        let affects_compilation = if self.project_main.is_some() {
            affects_fixed_preview
        } else {
            was_active
        };
        let previous_config = affects_compilation.then(|| self.compiler_config());

        let discarded = self.discarded_tabs.remove(&id);
        let closed = if discarded {
            match (document.path(), document.saved_text()) {
                (Some(path), Some(saved_text)) => Some(session::Document {
                    path: Some(path.to_path_buf()),
                    text: saved_text.to_owned(),
                    saved_text: Some(saved_text.to_owned()),
                }),
                _ => None,
            }
        } else {
            Some(session::Document {
                path: document.path().map(Path::to_path_buf),
                text: document.snapshot().1,
                saved_text: document.saved_text().map(str::to_owned),
            })
        };

        self.document.remove(id);
        if let Some(closed) = closed {
            self.closed_documents.push(closed);
            if self.closed_documents.len() > 20 {
                self.closed_documents.remove(0);
            }
        }
        self.discarded_on_close.remove(&id);
        self.file_status = Some(format!("Aba fechada: {name}"));

        if let Some(previous_config) = previous_config {
            if was_active {
                self.active_document_replaced();
            } else {
                self.mark_session_changed();
            }
            self.restart_compilation(previous_config, self.project_main.is_none());
        } else if was_active {
            self.active_document_replaced();
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
        let project_changed = self.workspace_root != path;
        self.workspace_root = path;
        self.recent_projects
            .retain(|recent| recent != &self.workspace_root);
        self.recent_projects.insert(0, self.workspace_root.clone());
        self.recent_projects.truncate(10);
        if project_changed {
            self.project_main = None;
        }
        self.detect_project_main_on_scan = true;
        self.project_tree.clear();
        self.expanded_project_directories.clear();
        self.expanded_project_directories
            .insert(self.workspace_root.clone());
        self.selected_project_entry = None;
        self.selected_project_file = None;
        self.project_context_menu = None;
        self.project_scan_busy = false;
        self.file_status = Some(format!("Projeto aberto: {}", self.workspace_root.display()));
        self.mark_session_changed();
        self.restart_compilation(previous_config, true);

        self.refresh_project_tree()
    }

    fn project_entry_pressed(&mut self, path: PathBuf, kind: project::EntryKind) -> Task<Message> {
        if self.file_busy {
            return Task::none();
        }

        self.project_tree_focused = true;
        self.selected_project_entry = Some(path.clone());
        match kind {
            project::EntryKind::Directory => {
                self.selected_project_file = None;
                let expanded = if self.expanded_project_directories.remove(&path) {
                    false
                } else {
                    self.expanded_project_directories.insert(path.clone());
                    true
                };
                if path == self.workspace_root {
                    self.file_status = Some(format!(
                        "Projeto {}: {}",
                        project_display_name(&path),
                        if expanded { "expandido" } else { "recolhido" }
                    ));
                } else {
                    let name = path
                        .strip_prefix(&self.workspace_root)
                        .unwrap_or(&path)
                        .to_string_lossy();
                    self.file_status = Some(format!(
                        "Pasta {name}: {}",
                        if expanded { "expandida" } else { "recolhida" }
                    ));
                }
                Task::none()
            }
            project::EntryKind::TypstFile => self.open_project_file(path),
            project::EntryKind::File => {
                self.selected_project_file = None;
                self.file_status = Some(format!(
                    "{} não é um documento Typst editável",
                    path.display()
                ));
                Task::none()
            }
        }
    }

    fn navigate_project_tree(&mut self, navigation: TreeNavigation) -> Task<Message> {
        if self.file_busy || self.project_navigation != ProjectNavigation::Files {
            return Task::none();
        }
        let entries = self.visible_project_entries();
        if entries.is_empty() {
            return Task::none();
        }
        let current = self
            .selected_project_entry
            .as_ref()
            .and_then(|selected| entries.iter().position(|(path, _)| path == selected))
            .unwrap_or(0);

        match navigation {
            TreeNavigation::Activate => {
                let (path, kind) = entries[current].clone();
                return self.project_entry_pressed(path, kind);
            }
            TreeNavigation::ParentOrCollapse => {
                let (path, kind) = &entries[current];
                if *kind == project::EntryKind::Directory
                    && self.expanded_project_directories.remove(path)
                {
                    return Task::none();
                }
                let parent = path
                    .parent()
                    .filter(|parent| parent.starts_with(&self.workspace_root))
                    .unwrap_or(&self.workspace_root);
                if let Some(index) = entries.iter().position(|(path, _)| path == parent) {
                    self.select_project_tree_entry(&entries[index]);
                }
                return Task::none();
            }
            TreeNavigation::ChildOrExpand => {
                let (path, kind) = &entries[current];
                if *kind != project::EntryKind::Directory {
                    return Task::none();
                }
                if self.expanded_project_directories.insert(path.clone()) {
                    return Task::none();
                }
                if let Some(child) = entries
                    .get(current + 1)
                    .filter(|(child, _)| child.parent().is_some_and(|parent| parent == path))
                {
                    self.select_project_tree_entry(child);
                }
                return Task::none();
            }
            TreeNavigation::Previous
            | TreeNavigation::Next
            | TreeNavigation::First
            | TreeNavigation::Last => {}
        }

        let target = match navigation {
            TreeNavigation::Previous => current.saturating_sub(1),
            TreeNavigation::Next => (current + 1).min(entries.len() - 1),
            TreeNavigation::First => 0,
            TreeNavigation::Last => entries.len() - 1,
            TreeNavigation::ParentOrCollapse
            | TreeNavigation::ChildOrExpand
            | TreeNavigation::Activate => unreachable!(),
        };
        self.select_project_tree_entry(&entries[target]);
        Task::none()
    }

    fn visible_project_entries(&self) -> Vec<(PathBuf, project::EntryKind)> {
        let mut entries = vec![(self.workspace_root.clone(), project::EntryKind::Directory)];
        if self
            .expanded_project_directories
            .contains(&self.workspace_root)
        {
            append_visible_project_entries(
                &self.project_tree,
                &self.expanded_project_directories,
                &mut entries,
            );
        }
        entries
    }

    fn select_project_tree_entry(&mut self, entry: &(PathBuf, project::EntryKind)) {
        self.project_tree_focused = true;
        self.selected_project_entry = Some(entry.0.clone());
        self.selected_project_file =
            (entry.1 == project::EntryKind::TypstFile).then(|| entry.0.clone());
    }

    fn handle_file_dropped(&mut self, path: PathBuf) -> Task<Message> {
        if self.file_busy {
            return Task::none();
        }
        if path.is_dir() {
            return self.update(Message::ProjectFolderSelected(Some(path)));
        }
        if path.extension().is_some_and(|extension| extension == "typ") {
            if path.starts_with(&self.workspace_root) {
                self.reveal_project_entry(&path);
                return self.open_project_file(path);
            }
            self.file_busy = true;
            self.file_status = Some(format!("Abrindo arquivo solto: {}", path.display()));
            return Task::perform(read_document(path), Message::OpenFinished);
        }

        self.file_status = Some("Solte uma pasta ou um arquivo .typ para abri-lo".to_owned());
        Task::none()
    }

    fn open_project_file(&mut self, path: PathBuf) -> Task<Message> {
        if self.file_busy {
            return Task::none();
        }

        self.reveal_project_entry(&path);
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

    fn reveal_project_entry(&mut self, path: &Path) {
        self.selected_project_entry = Some(path.to_path_buf());
        if path.starts_with(&self.workspace_root) {
            self.expanded_project_directories
                .insert(self.workspace_root.clone());
        }
        let mut parent = path.parent();

        while let Some(directory) = parent {
            if directory == self.workspace_root {
                break;
            }
            if !directory.starts_with(&self.workspace_root) {
                break;
            }

            self.expanded_project_directories
                .insert(directory.to_path_buf());
            parent = directory.parent();
        }
    }

    fn set_project_main(&mut self, path: PathBuf) {
        if self.file_busy {
            return;
        }
        if !path.starts_with(&self.workspace_root)
            || path.extension().is_none_or(|extension| extension != "typ")
        {
            self.file_status = Some("Selecione um arquivo Typst do projeto".to_owned());
            return;
        }
        let name = path
            .strip_prefix(&self.workspace_root)
            .unwrap_or(&path)
            .to_string_lossy()
            .into_owned();

        self.update_project_main(Some(path), &format!("Documento principal definido: {name}"));
    }

    fn clear_project_main(&mut self) {
        if self.file_busy || self.project_main.is_none() {
            return;
        }

        self.update_project_main(None, "O preview voltou a acompanhar a aba ativa");
    }

    fn update_project_main(&mut self, project_main: Option<PathBuf>, status: &str) {
        let project_main = project_main.filter(|path| {
            path.starts_with(&self.workspace_root)
                && path.extension().is_some_and(|extension| extension == "typ")
        });
        if self.project_main == project_main {
            return;
        }

        let previous_config = self.compiler_config();
        self.project_main = project_main;
        self.mark_session_changed();
        self.restart_compilation(previous_config, true);
        self.file_status = Some(status.to_owned());
    }

    fn start_create_project_file(&mut self) -> Task<Message> {
        let directory = self.selected_project_target_directory();
        self.start_create_project_file_at(directory)
    }

    fn start_create_project_file_at(&mut self, directory: PathBuf) -> Task<Message> {
        if self.file_busy {
            return Task::none();
        }

        self.file_busy = true;
        self.file_status = Some("Escolha o nome do novo arquivo Typst...".to_owned());
        Task::perform(
            project::create_file(self.workspace_root.clone(), directory),
            Message::ProjectOperationFinished,
        )
    }

    fn start_create_project_directory(&mut self) -> Task<Message> {
        let directory = self.selected_project_target_directory();
        self.start_create_project_directory_at(directory)
    }

    fn start_create_project_directory_at(&mut self, directory: PathBuf) -> Task<Message> {
        if self.file_busy {
            return Task::none();
        }

        self.file_busy = true;
        self.file_status = Some("Escolha o nome da nova pasta...".to_owned());
        Task::perform(
            project::create_directory(self.workspace_root.clone(), directory),
            Message::ProjectOperationFinished,
        )
    }

    fn start_rename_project_entry(
        &mut self,
        path: PathBuf,
        kind: project::EntryKind,
    ) -> Task<Message> {
        if self.file_busy {
            return Task::none();
        }
        if path == self.workspace_root || !path.starts_with(&self.workspace_root) {
            self.file_status = Some("A raiz do projeto não pode ser renomeada".to_owned());
            return Task::none();
        }

        self.file_busy = true;
        self.file_status = Some(format!("Renomeando {}...", path.display()));
        Task::perform(
            project::rename_entry(self.workspace_root.clone(), path, kind),
            Message::ProjectOperationFinished,
        )
    }

    fn start_move_project_entry(
        &mut self,
        path: PathBuf,
        kind: project::EntryKind,
    ) -> Task<Message> {
        if self.file_busy || path == self.workspace_root {
            return Task::none();
        }
        self.file_busy = true;
        self.file_status = Some(format!("Escolha o destino de {}...", path.display()));
        Task::perform(
            project::move_entry(self.workspace_root.clone(), path, kind),
            Message::ProjectOperationFinished,
        )
    }

    fn start_duplicate_project_entry(
        &mut self,
        path: PathBuf,
        kind: project::EntryKind,
    ) -> Task<Message> {
        if self.file_busy || path == self.workspace_root {
            return Task::none();
        }
        self.file_busy = true;
        self.file_status = Some(format!("Duplicando {}...", path.display()));
        Task::perform(
            project::duplicate_entry(self.workspace_root.clone(), path, kind),
            Message::ProjectOperationFinished,
        )
    }

    fn start_delete_project_entry(
        &mut self,
        path: PathBuf,
        kind: project::EntryKind,
    ) -> Task<Message> {
        if self.file_busy {
            return Task::none();
        }
        if path == self.workspace_root || !path.starts_with(&self.workspace_root) {
            self.file_status = Some("A raiz do projeto não pode ser excluída".to_owned());
            return Task::none();
        }
        if self.document.iter().any(|(_, document)| {
            document.is_dirty()
                && document.path().is_some_and(|document_path| {
                    project_entry_contains_path(&path, kind, document_path)
                })
        }) {
            self.file_status = Some(
                "Salve ou feche os documentos alterados antes de excluir esse item".to_owned(),
            );
            return Task::none();
        }

        self.file_busy = true;
        self.file_status = Some(format!(
            "Aguardando confirmação para excluir {}",
            path.display()
        ));
        self.pending_alert_dialog = Some(PendingAlertDialog::DeleteProjectEntry { path, kind });
        Task::none()
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
            project::OperationOutcome::Created { path, kind } => {
                self.reveal_project_entry(&path);
                match kind {
                    project::EntryKind::Directory => {
                        self.expanded_project_directories.insert(path.clone());
                        self.selected_project_file = None;
                        self.file_status = Some(format!("Pasta criada: {}", path.display()));
                    }
                    project::EntryKind::TypstFile => {
                        self.selected_project_file = Some(path.clone());
                        self.file_status = Some(format!("Arquivo criado: {}", path.display()));
                        let open = if let Some(id) = self.document.find_path(&path) {
                            self.activate_document(id);
                            Task::none()
                        } else {
                            self.open_project_file(path)
                        };
                        return Task::batch([open, self.refresh_project_tree()]);
                    }
                    project::EntryKind::File => {
                        self.selected_project_file = None;
                        self.file_status = Some(format!("Arquivo criado: {}", path.display()));
                    }
                }
            }
            project::OperationOutcome::Renamed { from, to, kind } => {
                let previous_config = self.compiler_config();
                let active_renamed = self
                    .document
                    .path()
                    .is_some_and(|path| project_entry_contains_path(&from, kind, path));

                for (_, document) in self.document.iter_mut() {
                    let Some(path) = document.path() else {
                        continue;
                    };
                    if let Some(relocated) = remap_project_path(path, &from, &to, kind) {
                        document.relocate(relocated);
                    }
                }
                let renamed_main = self
                    .project_main
                    .as_deref()
                    .and_then(|main| remap_project_path(main, &from, &to, kind));
                if let Some(main) = renamed_main {
                    self.project_main = Some(main);
                }
                self.expanded_project_directories =
                    std::mem::take(&mut self.expanded_project_directories)
                        .into_iter()
                        .map(|directory| {
                            remap_project_path(&directory, &from, &to, kind).unwrap_or(directory)
                        })
                        .collect();

                self.reveal_project_entry(&to);
                self.selected_project_file =
                    (kind == project::EntryKind::TypstFile).then_some(to.clone());
                let noun = if kind == project::EntryKind::Directory {
                    "Pasta"
                } else {
                    "Arquivo"
                };
                self.file_status = Some(format!(
                    "{noun} renomeado: {} -> {}",
                    from.display(),
                    to.display()
                ));
                if active_renamed {
                    self.apply_editor_settings();
                    self.replace_editor_pane_identity();
                }
                self.mark_session_changed();
                self.restart_compilation(previous_config, true);
            }
            project::OperationOutcome::Deleted { path, kind } => {
                let previous_config = self.compiler_config();
                let deleted_main = self
                    .project_main
                    .as_deref()
                    .is_some_and(|main| project_entry_contains_path(&path, kind, main));
                if deleted_main {
                    self.project_main = None;
                }

                let removed = self
                    .document
                    .iter()
                    .filter_map(|(id, document)| {
                        document
                            .path()
                            .is_some_and(|document_path| {
                                project_entry_contains_path(&path, kind, document_path)
                            })
                            .then_some(id)
                    })
                    .collect::<Vec<_>>();
                let active_removed = removed.contains(&self.document.active_id());
                for id in removed {
                    self.document.remove(id);
                    self.discarded_on_close.remove(&id);
                }
                if active_removed {
                    self.active_document_replaced();
                } else {
                    self.mark_session_changed();
                }
                self.expanded_project_directories
                    .retain(|directory| !project_entry_contains_path(&path, kind, directory));
                self.restart_compilation(previous_config, true);
                if self
                    .selected_project_entry
                    .as_deref()
                    .is_some_and(|selected| project_entry_contains_path(&path, kind, selected))
                {
                    self.selected_project_entry = None;
                }
                self.selected_project_file = None;
                let noun = if kind == project::EntryKind::Directory {
                    "Pasta excluída"
                } else {
                    "Arquivo excluído"
                };
                self.file_status = Some(if deleted_main {
                    format!(
                        "{noun}: {}; o Preview agora acompanha a aba ativa",
                        path.display()
                    )
                } else {
                    format!("{noun}: {}", path.display())
                });
            }
        }

        self.refresh_project_tree()
    }

    fn open_diagnostic(
        &mut self,
        target: compiler::DiagnosticTarget,
        range: Range<usize>,
    ) -> Task<Message> {
        self.reveal_source_target(target, range, "Diagnóstico revelado no editor")
    }

    fn document_outline_pressed(
        &mut self,
        target: source_map::SourceTarget,
        range: Range<usize>,
        has_children: bool,
    ) -> Task<Message> {
        if has_children {
            let key = OutlineKey::new(target.clone(), range.start);

            if !self.collapsed_outline_entries.insert(key.clone()) {
                self.collapsed_outline_entries.remove(&key);
            }
        }

        self.reveal_source_target(target, range, "Tópico revelado no editor")
    }

    fn reveal_source_target(
        &mut self,
        target: source_map::SourceTarget,
        range: Range<usize>,
        status: &str,
    ) -> Task<Message> {
        let target = match target {
            source_map::SourceTarget::Main => self
                .project_main
                .clone()
                .map(source_map::SourceTarget::ProjectFile)
                .unwrap_or(source_map::SourceTarget::Main),
            target => target,
        };

        match target {
            source_map::SourceTarget::Main => {
                self.document.reveal_range(range);
                self.file_status = Some(status.to_owned());
                Task::none()
            }
            source_map::SourceTarget::ProjectFile(path) => {
                if let Some(id) = self.document.find_path(&path) {
                    self.activate_document(id);
                    if let Some(document) = self.document.get_mut(id) {
                        document.reveal_range(range);
                    }
                    self.file_status = Some(status.to_owned());
                    Task::none()
                } else if self.file_busy {
                    Task::none()
                } else {
                    self.pending_source_reveal = Some(PendingSourceReveal {
                        path: path.clone(),
                        range,
                        status: status.to_owned(),
                    });
                    self.open_project_file(path)
                }
            }
        }
    }

    fn refresh_project_tree(&mut self) -> Task<Message> {
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

        Task::batch([self.refresh_project_tree(), self.check_external_files()])
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
        if self.settings.auto_save && self.has_auto_save_documents() {
            self.auto_save_deadline = Some(Instant::now() + AUTO_SAVE_DEBOUNCE);
        }
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
            return iced::exit();
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

            if self.session.close_after_write.take().is_some() {
                self.file_busy = false;
                self.discarded_on_close.clear();
                return iced::exit();
            }

            return Task::none();
        }

        if self.session.close_after_write.is_some() {
            if outcome.revision == self.session.revision {
                self.session.close_after_write = None;
                self.file_busy = false;
                self.discarded_on_close.clear();
                return iced::exit();
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

        let mut stored = session::Session::new(
            self.workspace_root.clone(),
            self.project_main.clone(),
            active_document.unwrap_or(0),
            documents,
            self.pane_layout(),
            self.settings,
        );
        stored.recent_projects = self.recent_projects.clone();
        stored
    }

    fn pane_layout(&self) -> session::PaneLayout {
        pane_node_from_layout(self.panes.layout(), &self.panes)
            .map(session::PaneLayout::from_tree)
            .unwrap_or_default()
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

    fn has_auto_save_documents(&self) -> bool {
        self.document
            .iter()
            .any(|(_, document)| document.is_dirty() && document.path().is_some())
    }

    fn save_requests(&self, include_drafts: bool) -> Vec<SaveRequest> {
        self.document
            .iter()
            .filter(|(_, document)| {
                document.is_dirty() && (include_drafts || document.path().is_some())
            })
            .map(|(document_id, document)| SaveRequest {
                document_id,
                path: document.path().map(Path::to_path_buf),
                directory: document.directory(&self.workspace_root),
                file_name: document.display_name(),
                source: document.snapshot().1,
            })
            .collect()
    }

    fn start_save_all(&mut self) -> Task<Message> {
        if self.file_busy {
            return Task::none();
        }
        let requests = self.save_requests(true);
        if requests.is_empty() {
            self.file_status = Some("Todos os documentos já estão salvos".to_owned());
            return Task::none();
        }

        self.file_busy = true;
        self.auto_save_deadline = None;
        self.file_status = Some("Salvando todos os documentos...".to_owned());
        Task::perform(save_documents(requests, true), Message::SaveAllFinished)
    }

    fn dispatch_auto_save(&mut self, now: Instant) -> Task<Message> {
        let Some(deadline) = self.auto_save_deadline else {
            return Task::none();
        };
        if now < deadline {
            return Task::none();
        }
        if self.file_busy || self.auto_save_busy {
            self.auto_save_deadline = Some(now + AUTO_SAVE_DEBOUNCE);
            return Task::none();
        }

        let requests = self.save_requests(false);
        self.auto_save_deadline = None;
        if requests.is_empty() {
            return Task::none();
        }
        self.auto_save_busy = true;
        Task::perform(save_documents(requests, false), Message::AutoSaveFinished)
    }

    fn handle_save_all_finished(
        &mut self,
        outcomes: Vec<SaveOutcome>,
        automatic: bool,
    ) -> Task<Message> {
        let previous_config = self.compiler_config();
        let mut saved = 0;
        let mut cancelled = 0;
        let mut errors = Vec::new();

        for outcome in outcomes {
            match outcome {
                SaveOutcome::Saved {
                    document_id,
                    path,
                    source,
                } => {
                    if let Some(document) = self.document.get_mut(document_id) {
                        document.mark_saved(path, source);
                        saved += 1;
                    }
                }
                SaveOutcome::Cancelled { .. } => cancelled += 1,
                SaveOutcome::Failed { error, .. } => errors.push(error),
            }
        }

        if automatic {
            self.auto_save_busy = false;
            if !errors.is_empty() {
                self.file_status = Some(format!(
                    "Falha no salvamento automático: {}",
                    truncate(&errors.join("; "), 140)
                ));
            }
        } else {
            self.file_busy = false;
            self.file_status = Some(if errors.is_empty() && cancelled == 0 {
                format!("{saved} documento(s) salvo(s)")
            } else if !errors.is_empty() {
                format!(
                    "Salvamento parcial: {saved} salvo(s); {}",
                    truncate(&errors.join("; "), 140)
                )
            } else {
                format!("{saved} documento(s) salvo(s); operação cancelada")
            });
        }

        if saved == 0 {
            return Task::none();
        }
        self.mark_session_changed();
        if previous_config != self.compiler_config() {
            self.restart_compilation(previous_config, true);
        } else {
            self.schedule_compile(Duration::ZERO, true);
            self.dispatch_compile(Instant::now());
        }
        self.refresh_project_tree()
    }

    fn start_export(&mut self, format: compiler::ExportFormat) -> Task<Message> {
        if self.file_busy || self.compiler.is_none() {
            return Task::none();
        }

        self.file_busy = true;
        self.file_status = Some(format!("Aguardando o destino do {}...", format.label()));
        let directory = self.compilation_directory();
        let file_name = export_file_name(&self.compilation_display_name(), format);

        Task::perform(
            choose_export_path(directory, file_name, format),
            move |outcome| Message::ExportPathSelected { format, outcome },
        )
    }

    fn handle_export_path_selected(
        &mut self,
        format: compiler::ExportFormat,
        outcome: ExportPathOutcome,
    ) -> Task<Message> {
        let ExportPathOutcome::Selected(path) = outcome else {
            self.file_busy = false;
            self.file_status = Some(format!("A exportação de {} foi cancelada", format.label()));
            return Task::none();
        };
        let Some(sender) = self.compiler.clone() else {
            self.file_busy = false;
            self.file_status = Some("O worker de compilação não está disponível".to_owned());
            return Task::none();
        };
        let revision = self.compilation_revision;
        let source = self.compilation_source();

        self.next_request_id += 1;
        let request_id = self.next_request_id;
        let request = compiler::Request {
            id: request_id,
            revision,
            source,
            overlays: self.source_overlays(),
            reset_files: true,
            purpose: compiler::Purpose::Export(format),
            export_options: self.export_options(),
        };

        if sender.unbounded_send(request).is_err() {
            self.compiler = None;
            self.file_busy = false;
            self.file_status = Some("O worker de compilação foi encerrado".to_owned());
            return Task::none();
        }

        self.pending_export = Some(PendingExport {
            format,
            request_id,
            revision,
            path,
        });
        self.file_status = Some(format!("Gerando {}...", format.label()));
        Task::none()
    }

    fn handle_export_write_finished(&mut self, outcome: ExportWriteOutcome) -> Task<Message> {
        self.file_busy = false;

        match outcome {
            ExportWriteOutcome::Saved { format, path } => {
                self.file_status =
                    Some(format!("{} exportado: {}", format.label(), path.display()));
            }
            ExportWriteOutcome::Failed { format, error } => {
                eprintln!("erro ao exportar {}: {error}", format.label());
                self.file_status = Some(format!("Erro ao exportar {}: {error}", format.label()));
            }
        }

        Task::none()
    }

    fn handle_open_finished(&mut self, outcome: OpenOutcome) -> Task<Message> {
        self.file_busy = false;

        match outcome {
            OpenOutcome::Cancelled => {
                self.pending_source_reveal = None;
                self.file_status = Some("A abertura foi cancelada".to_owned());
            }
            OpenOutcome::Failed(error) => {
                self.pending_source_reveal = None;
                eprintln!("erro ao abrir documento: {error}");
                self.file_status = Some(format!("Erro ao abrir: {error}"));
            }
            OpenOutcome::Loaded { path, source } => {
                if path.starts_with(&self.workspace_root) {
                    self.reveal_project_entry(&path);
                    self.selected_project_file = Some(path.clone());
                }
                if let Some(id) = self.document.find_path(&path) {
                    self.activate_document(id);
                    self.file_status = Some(format!("Aba ativada: {}", path.display()));
                } else {
                    let previous_config = self.compiler_config();
                    self.document.clear_search_matches();
                    self.document.add(Document::opened(path.clone(), source));
                    self.file_status = Some(format!("Aberto: {}", path.display()));
                    if self.project_main.is_some() {
                        self.active_document_replaced();
                    } else {
                        self.document_replaced(previous_config);
                    }
                }

                if let Some(reveal) = self.pending_source_reveal.take()
                    && reveal.path == path
                    && let Some(id) = self.document.find_path(&path)
                    && let Some(document) = self.document.get_mut(id)
                {
                    document.reveal_range(reveal.range);
                    self.file_status = Some(reveal.status);
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
                let previous_config = self.compiler_config();
                let old_path = self
                    .document
                    .get(document_id)
                    .and_then(Document::path)
                    .map(Path::to_path_buf);
                let was_main = self
                    .project_main
                    .as_deref()
                    .is_some_and(|main| old_path.as_deref() == Some(main));
                let Some(document) = self.document.get_mut(document_id) else {
                    self.pending_after_save = None;
                    return Task::none();
                };

                document.mark_saved(path.clone(), source);
                let still_dirty = document.is_dirty();
                let path_changed = old_path.as_deref() != Some(path.as_path());
                if was_main {
                    self.project_main = (path.starts_with(&self.workspace_root)
                        && path.extension().is_some_and(|extension| extension == "typ"))
                    .then_some(path.clone());
                }
                let main_released = was_main && self.project_main.is_none();
                self.mark_session_changed();

                self.file_status = Some(if main_released {
                    format!(
                        "Salvo em {}; fora do projeto, o preview agora acompanha a aba ativa",
                        path.display()
                    )
                } else if still_dirty {
                    format!(
                        "Versão salva em {}; há alterações mais recentes",
                        path.display()
                    )
                } else {
                    format!("Salvo em {}", path.display())
                });

                if path_changed {
                    self.restart_compilation(previous_config, true);
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
        self.apply_editor_settings();
        self.replace_editor_pane_identity();
        self.mark_session_changed();
        self.restart_compilation(previous_config, true);
    }

    fn active_document_replaced(&mut self) {
        self.search.visible = false;
        self.apply_editor_settings();
        self.replace_editor_pane_identity();
        self.preview_highlight = None;
        self.mark_session_changed();
    }

    fn restart_compilation(&mut self, previous_config: compiler::Config, clear_preview: bool) {
        if previous_config != self.compiler_config() {
            self.compiler = None;
        }
        self.latest_request_id = None;
        self.clear_compile_diagnostics();

        if clear_preview {
            self.preview.clear();
            self.preview_revision = None;
            self.preview_pointer = None;
            self.preview_highlight = None;
            self.document_outline.clear();
            self.collapsed_outline_entries.clear();
        }

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

    fn compiler_config(&self) -> compiler::Config {
        if let Some(path) = self.project_main.as_deref()
            && let Some((root, main_name)) = compiler_location_for_path(&self.workspace_root, path)
        {
            return compiler::Config::new(root, main_name);
        }

        let (root, main_name) = self.document.compiler_location(&self.workspace_root);
        compiler::Config::new(root, main_name)
    }

    fn compilation_main_document_id(&self) -> Option<DocumentId> {
        match self.project_main.as_deref() {
            Some(path) => self.document.find_path(path),
            None => Some(self.document.active_id()),
        }
    }

    fn compilation_source(&self) -> Option<String> {
        self.compilation_main_document_id()
            .and_then(|id| self.document.get(id))
            .map(|document| document.snapshot().1)
    }

    fn compilation_main_path(&self) -> Option<&Path> {
        self.project_main
            .as_deref()
            .or_else(|| self.document.path())
    }

    fn compilation_display_name(&self) -> String {
        self.compilation_main_path()
            .and_then(Path::file_name)
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.document.display_name())
    }

    fn compilation_directory(&self) -> PathBuf {
        self.compilation_main_path()
            .and_then(Path::parent)
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or(&self.workspace_root)
            .to_path_buf()
    }

    fn export_options(&self) -> compiler::ExportOptions {
        compiler::ExportOptions {
            pdf_tagged: self.settings.pdf_tagged,
            pdf_pretty: self.settings.pdf_pretty,
            svg_render_bleed: self.settings.svg_render_bleed,
            svg_pretty: self.settings.svg_pretty,
            svg_page_gap: self.settings.svg_page_gap,
            html_pretty: self.settings.html_pretty,
        }
    }

    fn active_source_target(&self) -> Option<source_map::SourceTarget> {
        let Some(main) = self.project_main.as_deref() else {
            return Some(source_map::SourceTarget::Main);
        };
        let path = self.document.path()?;

        if path == main {
            Some(source_map::SourceTarget::Main)
        } else if path.starts_with(&self.workspace_root) {
            Some(source_map::SourceTarget::ProjectFile(path.to_path_buf()))
        } else {
            None
        }
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

    fn preview_map_ready(&self) -> bool {
        matches!(self.preview_status, PreviewStatus::Ready { .. })
            && self.preview_revision == Some(self.compilation_revision)
            && self.preview.iter().any(|page| !page.regions.is_empty())
    }

    fn preview_navigation_ready(&self) -> bool {
        let Some(target) = self.active_source_target() else {
            return false;
        };

        self.preview_map_ready()
            && self
                .preview
                .iter()
                .flat_map(|page| &page.regions)
                .any(|region| region.target == target)
    }

    fn reveal_cursor_in_preview(&mut self) -> Task<Message> {
        if !self.preview_navigation_ready() {
            self.file_status =
                Some("Aguarde um preview atualizado antes de localizar o cursor".to_owned());
            return Task::none();
        }

        let offset = self.document.cursor_offset();
        let Some(target) = self.active_source_target() else {
            self.file_status = Some("A aba ativa não pertence ao preview principal".to_owned());
            return Task::none();
        };
        let Some((page_index, region)) = find_source_region(&self.preview, &target, offset) else {
            self.file_status = Some("O cursor não produziu uma região no preview".to_owned());
            return Task::none();
        };
        let scroll_offset = self.preview_scroll_offset(page_index, region.bounds);

        self.preview_highlight = Some(PreviewHighlight {
            page: page_index,
            bounds: region.bounds,
        });
        self.file_status = Some(format!(
            "Cursor localizado no preview, página {}",
            page_index + 1
        ));

        operation::scroll_to(self.preview_scroll_id.clone(), scroll_offset)
    }

    fn reveal_preview_source(&mut self, page_index: usize) -> Task<Message> {
        if !self.preview_map_ready() {
            self.file_status =
                Some("Aguarde um preview atualizado antes de navegar para o texto".to_owned());
            return Task::none();
        }
        let Some(pointer) = self
            .preview_pointer
            .filter(|pointer| pointer.page == page_index)
        else {
            return Task::none();
        };
        let Some(page) = self.preview.get(page_index) else {
            return Task::none();
        };
        let scale = preview_scale(self.settings.preview_zoom, self.preview_logical_ppi);
        let x = pointer.position.x / scale;
        let y = pointer.position.y / scale;
        let Some(region) = find_preview_region(page, x, y) else {
            self.file_status = Some("Nenhuma origem encontrada nessa posição".to_owned());
            return Task::none();
        };

        self.preview_highlight = Some(PreviewHighlight {
            page: page_index,
            bounds: region.bounds,
        });
        self.reveal_source_target(
            region.target,
            region.range,
            "Origem do preview revelada no editor",
        )
    }

    fn preview_scroll_offset(
        &self,
        page_index: usize,
        bounds: source_map::SourceBounds,
    ) -> scrollable::AbsoluteOffset {
        let scale = preview_scale(self.settings.preview_zoom, self.preview_logical_ppi);
        let widest_page = self
            .preview
            .iter()
            .map(|page| page.width * scale)
            .fold(0.0_f32, f32::max);
        let page = &self.preview[page_index];
        let page_left = PREVIEW_PADDING + (widest_page - page.width * scale) / 2.0;
        let previous_height = self
            .preview
            .iter()
            .take(page_index)
            .map(|page| {
                PREVIEW_LABEL_HEIGHT
                    + PREVIEW_LABEL_SPACING
                    + page.height * scale
                    + PREVIEW_PAGE_SPACING
            })
            .sum::<f32>();
        let page_top =
            PREVIEW_PADDING + previous_height + PREVIEW_LABEL_HEIGHT + PREVIEW_LABEL_SPACING;

        scrollable::AbsoluteOffset {
            x: (page_left + bounds.x * scale - 32.0).max(0.0),
            y: (page_top + bounds.y * scale - 32.0).max(0.0),
        }
    }

    fn source_overlays(&self) -> Vec<SourceOverlay> {
        let main = self.compilation_main_document_id();
        let config = self.compiler_config();

        self.document
            .iter()
            .filter(|(id, document)| Some(*id) != main && document.is_dirty())
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
        let main = self.compilation_main_document_id();

        for (id, document) in self.document.iter_mut() {
            let editor_diagnostics = diagnostics
                .iter()
                .filter(|diagnostic| match &diagnostic.target {
                    compiler::DiagnosticTarget::Main => Some(id) == main,
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

    fn install_document_outline(&mut self, outline: Vec<compiler::DocumentOutlineItem>) {
        let mut available = HashSet::new();
        collect_outline_keys(&outline, &mut available);
        self.collapsed_outline_entries
            .retain(|key| available.contains(key));
        self.document_outline = outline;
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

    fn start_project_search(&mut self) -> Task<Message> {
        self.project_search.revision = self.project_search.revision.wrapping_add(1);
        let revision = self.project_search.revision;
        self.project_search.error = None;

        if self.project_search.query.is_empty() {
            self.project_search.busy = false;
            self.project_search.results.clear();
            self.project_search.skipped_files = 0;
            return Task::none();
        }

        self.project_search.busy = true;
        let files = collect_project_files(&self.project_tree);
        let overlays = self
            .document
            .iter()
            .filter_map(|(_, document)| {
                let path = document.path()?.to_path_buf();
                path.starts_with(&self.workspace_root)
                    .then(|| (path, document.snapshot().1))
            })
            .collect();

        Task::perform(
            project_search::search(
                revision,
                self.workspace_root.clone(),
                files,
                overlays,
                self.project_search.query.clone(),
                search::Options {
                    case_sensitive: self.project_search.case_sensitive,
                    whole_word: self.project_search.whole_word,
                },
            ),
            Message::ProjectSearchFinished,
        )
    }

    fn handle_project_search_finished(&mut self, outcome: project_search::SearchOutcome) {
        if outcome.revision != self.project_search.revision || outcome.root != self.workspace_root {
            return;
        }

        self.project_search.busy = false;
        match outcome.result {
            Ok(report) => {
                self.project_search.results = report.matches;
                self.project_search.skipped_files = report.skipped_files;
                self.project_search.error = None;
            }
            Err(error) => {
                self.project_search.results.clear();
                self.project_search.skipped_files = 0;
                self.project_search.error = Some(error);
            }
        }
    }

    fn replace_all_project_matches(&mut self) -> Task<Message> {
        if self.file_busy || self.project_search.results.is_empty() {
            return Task::none();
        }

        let options = search::Options {
            case_sensitive: self.project_search.case_sensitive,
            whole_word: self.project_search.whole_word,
        };
        let query = self.project_search.query.clone();
        let replacement = self.project_search.replacement.clone();
        let result_paths = self
            .project_search
            .results
            .iter()
            .map(|found| found.path.clone())
            .collect::<HashSet<_>>();
        let open_paths = self
            .document
            .iter()
            .filter_map(|(_, document)| document.path().map(Path::to_path_buf))
            .collect::<HashSet<_>>();
        let closed_files = result_paths
            .difference(&open_paths)
            .cloned()
            .collect::<Vec<_>>();
        let mut replaced = 0;
        let mut changed_files = 0;

        for (_, document) in self.document.iter_mut() {
            if !document
                .path()
                .is_some_and(|path| result_paths.contains(path))
            {
                continue;
            }
            let matches = search::find_matches(&document.snapshot().1, &query, options);
            if matches.is_empty() {
                continue;
            }
            replaced += matches.len();
            changed_files += 1;
            let edits = matches
                .into_iter()
                .map(|range| (range, replacement.clone()))
                .collect();
            document.perform(Action::ApplyEdits(edits));
        }

        if replaced > 0 {
            self.clear_compile_diagnostics();
            self.mark_session_changed();
            self.schedule_compile(Duration::ZERO, false);
            self.dispatch_compile(Instant::now());
        }

        self.file_busy = true;
        self.project_search.pending_replaced = replaced;
        self.project_search.pending_changed_files = changed_files;
        self.file_status = Some("Substituindo ocorrências no projeto...".to_owned());
        Task::perform(
            project_search::replace_closed_files(
                self.project_search.revision,
                closed_files,
                query,
                replacement,
                options,
            ),
            Message::ProjectReplaceFinished,
        )
    }

    fn handle_project_replace_finished(
        &mut self,
        outcome: project_search::ReplaceOutcome,
    ) -> Task<Message> {
        self.file_busy = false;
        let replaced = self.project_search.pending_replaced + outcome.replaced;
        let changed_files = self.project_search.pending_changed_files + outcome.changed_files;
        self.project_search.pending_replaced = 0;
        self.project_search.pending_changed_files = 0;

        if outcome.errors.is_empty() {
            self.file_status = Some(format!(
                "{replaced} ocorrência(s) substituída(s) em {changed_files} arquivo(s)"
            ));
        } else {
            let error = outcome.errors.join("; ");
            self.file_status = Some(format!(
                "Substituição parcial: {replaced} ocorrência(s); {}",
                truncate(&error, 140)
            ));
            self.project_search.error = Some(error);
        }

        if outcome.revision != self.project_search.revision {
            return Task::none();
        }
        self.schedule_compile(Duration::ZERO, true);
        self.dispatch_compile(Instant::now());
        self.start_project_search()
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
        self.compilation_revision = self.compilation_revision.wrapping_add(1);
        self.preview_highlight = None;
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

        let revision = self.compilation_revision;
        let source = self.compilation_source();

        self.next_request_id += 1;
        let request_id = self.next_request_id;
        let request = compiler::Request {
            id: request_id,
            revision,
            source,
            overlays: self.source_overlays(),
            reset_files: pending.reset_files,
            purpose: compiler::Purpose::Preview,
            export_options: self.export_options(),
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
                    compiler::Purpose::Export(_) => self.handle_export_output(output),
                }
            }
        }
    }

    fn handle_preview_output(&mut self, output: compiler::Output) -> Task<Message> {
        let current_revision = self.compilation_revision;

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

        self.install_document_outline(output.outline);
        self.preview = output
            .pages
            .into_iter()
            .map(|page| PreviewPage {
                handle: svg::Handle::from_memory(page.svg),
                width: page.width,
                height: page.height,
                regions: page.regions,
            })
            .collect();
        self.preview_revision = Some(output.revision);
        self.preview_pointer = None;
        self.preview_highlight = None;
        self.preview_status = PreviewStatus::Ready {
            pages: output.page_count,
            warnings: output.warning_count,
        };
        Task::none()
    }

    fn handle_export_output(&mut self, output: compiler::Output) -> Task<Message> {
        let Some(pending) = self.pending_export.take() else {
            return Task::none();
        };

        if pending.request_id != output.id || pending.revision != output.revision {
            self.pending_export = Some(pending);
            return Task::none();
        }

        let compiler::Purpose::Export(format) = output.purpose else {
            self.pending_export = Some(pending);
            return Task::none();
        };
        if format != pending.format {
            self.pending_export = Some(pending);
            return Task::none();
        }

        if self.compilation_revision == output.revision {
            self.install_diagnostics(output.diagnostics);
        }

        if output.error_count > 0 {
            self.file_busy = false;
            self.file_status = Some(format!(
                "Falha ao gerar {}: {}",
                format.label(),
                output
                    .summary
                    .unwrap_or_else(|| format!("{} erro(s)", output.error_count))
            ));
            return Task::none();
        }

        let Some(artifact) = output.artifact else {
            self.file_busy = false;
            self.file_status = Some(format!(
                "A compilação não produziu um arquivo {}",
                format.label()
            ));
            return Task::none();
        };

        self.file_status = Some(format!("Gravando {}...", format.label()));
        Task::perform(
            write_export(pending.path, artifact, format),
            Message::ExportWriteFinished,
        )
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

fn find_source_region(
    pages: &[PreviewPage],
    target: &source_map::SourceTarget,
    offset: usize,
) -> Option<(usize, source_map::SourceRegion)> {
    pages
        .iter()
        .enumerate()
        .flat_map(|(page_index, page)| {
            page.regions
                .iter()
                .filter(move |region| &region.target == target)
                .map(move |region| (page_index, region))
        })
        .min_by(|(_, left), (_, right)| source_map::compare_source_candidates(left, right, offset))
        .map(|(page_index, region)| (page_index, region.clone()))
}

fn collect_outline_keys(entries: &[compiler::DocumentOutlineItem], keys: &mut HashSet<OutlineKey>) {
    for entry in entries {
        keys.insert(OutlineKey::new(entry.target.clone(), entry.range.start));
        collect_outline_keys(&entry.children, keys);
    }
}

fn find_current_outline_key(
    entries: &[compiler::DocumentOutlineItem],
    target: &source_map::SourceTarget,
    cursor: usize,
    selected: &mut Option<OutlineKey>,
) {
    for entry in entries {
        if &entry.target == target
            && entry.range.start <= cursor
            && selected
                .as_ref()
                .is_none_or(|current| current.start <= entry.range.start)
        {
            *selected = Some(OutlineKey::new(entry.target.clone(), entry.range.start));
        }

        find_current_outline_key(&entry.children, target, cursor, selected);
    }
}

fn find_preview_region(page: &PreviewPage, x: f32, y: f32) -> Option<source_map::SourceRegion> {
    let maximum_distance = PREVIEW_HIT_DISTANCE * PREVIEW_HIT_DISTANCE;

    page.regions
        .iter()
        .filter(|region| region.bounds.distance_squared(x, y) <= maximum_distance)
        .min_by(|left, right| {
            let left_exact = left.bounds.contains(x, y, 1.5);
            let right_exact = right.bounds.contains(x, y, 1.5);

            right_exact
                .cmp(&left_exact)
                .then_with(|| {
                    left.bounds
                        .distance_squared(x, y)
                        .partial_cmp(&right.bounds.distance_squared(x, y))
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .then_with(|| left.kind.hit_priority().cmp(&right.kind.hit_priority()))
                .then_with(|| {
                    left.bounds
                        .area()
                        .partial_cmp(&right.bounds.area())
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
        })
        .cloned()
}

fn panes_from_layout(layout: session::PaneLayout) -> pane_grid::State<Pane> {
    pane_grid::State::with_configuration(pane_configuration(layout.into_tree()))
}

fn pane_configuration(node: session::PaneNode) -> pane_grid::Configuration<Pane> {
    match node {
        session::PaneNode::Pane { pane } => pane_grid::Configuration::Pane(match pane {
            session::Pane::Project => Pane::Project,
            session::Pane::Editor => Pane::Editor,
            session::Pane::Preview => Pane::Preview,
        }),
        session::PaneNode::Split { axis, ratio, a, b } => pane_grid::Configuration::Split {
            axis: match axis {
                session::Axis::Horizontal => pane_grid::Axis::Horizontal,
                session::Axis::Vertical => pane_grid::Axis::Vertical,
            },
            ratio,
            a: Box::new(pane_configuration(*a)),
            b: Box::new(pane_configuration(*b)),
        },
    }
}

fn pane_node_from_layout(
    node: &pane_grid::Node,
    panes: &pane_grid::State<Pane>,
) -> Option<session::PaneNode> {
    match node {
        pane_grid::Node::Pane(id) => panes.get(*id).copied().map(|pane| {
            session::PaneNode::pane(match pane {
                Pane::Project => session::Pane::Project,
                Pane::Editor => session::Pane::Editor,
                Pane::Preview => session::Pane::Preview,
            })
        }),
        pane_grid::Node::Split {
            axis, ratio, a, b, ..
        } => Some(session::PaneNode::split(
            match axis {
                pane_grid::Axis::Horizontal => session::Axis::Horizontal,
                pane_grid::Axis::Vertical => session::Axis::Vertical,
            },
            *ratio,
            pane_node_from_layout(a, panes)?,
            pane_node_from_layout(b, panes)?,
        )),
    }
}

fn menu_popup(
    definitions: Vec<AppMenuEntry>,
    menu_focus: usize,
    width: f32,
) -> Element<'static, Message> {
    let mut item_index = 0;
    let mut entries = Vec::new();

    for entry in definitions {
        match entry {
            AppMenuEntry::Divider => entries.push(ui::MenuEntry::Divider),
            AppMenuEntry::Item {
                label,
                value,
                selected,
                enabled,
                command,
            } => {
                let mut item =
                    ui::MenuItem::new(label, enabled.then_some(Message::MenuCommand(command)))
                        .selected(selected)
                        .focused(item_index == menu_focus);
                if enabled {
                    item = item.on_focus(Message::MenuFocused(item_index));
                }
                if let Some(value) = value {
                    item = item.value(value);
                }
                entries.push(ui::MenuEntry::Item(item));
                item_index += 1;
            }
        }
    }

    ui::Menu::new(entries).width(width).into()
}

fn menu_popup_height(entries: &[AppMenuEntry]) -> f32 {
    let content = entries
        .iter()
        .map(|entry| match entry {
            AppMenuEntry::Item { .. } => ui::tokens::dimension::MENU_ITEM_HEIGHT_MEDIUM,
            AppMenuEntry::Divider => ui::tokens::dimension::MENU_SECTION_DIVIDER_HEIGHT,
        })
        .sum::<f32>();

    content + ui::tokens::spacing::MENU_POPOVER_PADDING * 2.0
}

fn enabled_menu_items(entries: &[AppMenuEntry]) -> Vec<usize> {
    let mut item_index = 0;
    let mut enabled = Vec::new();

    for entry in entries {
        if let AppMenuEntry::Item {
            enabled: item_enabled,
            ..
        } = entry
        {
            if *item_enabled {
                enabled.push(item_index);
            }
            item_index += 1;
        }
    }

    enabled
}

fn first_enabled_menu_item(entries: &[AppMenuEntry]) -> Option<usize> {
    enabled_menu_items(entries).into_iter().next()
}

fn menu_command_at(entries: &[AppMenuEntry], target: usize) -> Option<MenuCommand> {
    entries
        .iter()
        .filter_map(|entry| match entry {
            AppMenuEntry::Item {
                enabled: true,
                command,
                ..
            } => Some(Some(command.clone())),
            AppMenuEntry::Item { .. } => Some(None),
            AppMenuEntry::Divider => None,
        })
        .nth(target)
        .flatten()
}

fn menu_horizontal_offset(menu: AppMenu) -> f32 {
    APP_BAR_HORIZONTAL_PADDING
        + match menu {
            AppMenu::File => 0.0,
            AppMenu::Edit => FILE_MENU_TRIGGER_WIDTH,
            AppMenu::View => FILE_MENU_TRIGGER_WIDTH + EDIT_MENU_TRIGGER_WIDTH,
            AppMenu::Help => {
                FILE_MENU_TRIGGER_WIDTH + EDIT_MENU_TRIGGER_WIDTH + VIEW_MENU_TRIGGER_WIDTH
            }
        }
}

fn menu_popup_width(menu: AppMenu) -> f32 {
    match menu {
        AppMenu::File => 280.0,
        AppMenu::Edit => 304.0,
        AppMenu::View => 320.0,
        AppMenu::Help => 256.0,
    }
}

fn command_shortcut(key: &str) -> String {
    #[cfg(target_os = "macos")]
    let modifier = "⌘";
    #[cfg(not(target_os = "macos"))]
    let modifier = "Ctrl+";

    format!("{modifier}{key}")
}

fn command_shift_shortcut(key: &str) -> String {
    #[cfg(target_os = "macos")]
    let modifier = "⇧⌘";
    #[cfg(not(target_os = "macos"))]
    let modifier = "Ctrl+Shift+";

    format!("{modifier}{key}")
}

fn command_alt_shortcut(key: &str) -> String {
    #[cfg(target_os = "macos")]
    let modifier = "⌥⌘";
    #[cfg(not(target_os = "macos"))]
    let modifier = "Ctrl+Alt+";

    format!("{modifier}{key}")
}

fn settings_slider_row<'a>(
    label: &'a str,
    value: String,
    control: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    column![
        row![
            text(label).size(ui::tokens::typography::FONT_SIZE_100),
            Space::new().width(Fill),
            text(value)
                .size(ui::tokens::typography::FONT_SIZE_75)
                .style(search_metadata_text_style),
        ]
        .align_y(Alignment::Center),
        control.into(),
    ]
    .spacing(8)
    .into()
}

fn settings_subsection_title<'a>(title: &'a str) -> Element<'a, Message> {
    text(title)
        .size(ui::tokens::typography::FONT_SIZE_100)
        .font(iced::Font {
            weight: iced::font::Weight::Bold,
            ..iced::Font::DEFAULT
        })
        .into()
}

fn settings_icon_style(theme: &Theme, _status: svg::Status) -> svg::Style {
    svg::Style {
        color: Some(
            ui::tokens::SpectrumColors::from_theme(theme)
                .neutral_content
                .default,
        ),
    }
}

fn settings_band_style(theme: &Theme) -> iced::widget::container::Style {
    let colors = ui::tokens::SpectrumColors::from_theme(theme);

    iced::widget::container::Style::default()
        .background(colors.gray.gray_50)
        .border(Border {
            color: colors.gray.gray_300,
            width: 1.0,
            ..Border::default()
        })
}

fn settings_window_background_style(theme: &Theme) -> iced::widget::container::Style {
    iced::widget::container::Style::default()
        .background(ui::tokens::SpectrumColors::from_theme(theme).gray.gray_25)
}

fn search_panel_style(theme: &Theme) -> iced::widget::container::Style {
    let colors = ui::tokens::SpectrumColors::from_theme(theme);

    iced::widget::container::Style::default()
        .background(colors.gray.gray_50)
        .border(Border {
            color: colors.gray.gray_300,
            width: 1.0,
            ..Border::default()
        })
}

fn search_metadata_text_style(theme: &Theme) -> text::Style {
    text::Style {
        color: Some(ui::tokens::SpectrumColors::from_theme(theme).gray.gray_600),
    }
}

fn project_search_result_style(
    theme: &Theme,
    status: iced::widget::button::Status,
) -> iced::widget::button::Style {
    let colors = ui::tokens::SpectrumColors::from_theme(theme);
    let background = match status {
        iced::widget::button::Status::Hovered => {
            Some(iced::Background::Color(colors.gray.gray_100))
        }
        iced::widget::button::Status::Pressed => {
            Some(iced::Background::Color(colors.gray.gray_200))
        }
        iced::widget::button::Status::Active | iced::widget::button::Status::Disabled => None,
    };

    iced::widget::button::Style {
        background,
        text_color: colors.gray.gray_800,
        border: Border::default(),
        ..iced::widget::button::Style::default()
    }
}

fn app_bar_style(theme: &Theme) -> iced::widget::container::Style {
    let colors = ui::tokens::SpectrumColors::from_theme(theme);

    iced::widget::container::Style::default()
        .background(colors.gray.gray_50)
        .border(Border {
            color: colors.gray.gray_300,
            width: 1.0,
            ..Border::default()
        })
}

fn action_bar_style(theme: &Theme) -> iced::widget::container::Style {
    let colors = ui::tokens::SpectrumColors::from_theme(theme);

    iced::widget::container::Style::default()
        .background(colors.gray.gray_50)
        .border(Border {
            color: colors.gray.gray_300,
            width: 1.0,
            ..Border::default()
        })
}

fn project_navigation_divider_style(theme: &Theme) -> iced::widget::container::Style {
    let colors = ui::tokens::SpectrumColors::from_theme(theme);

    iced::widget::container::Style::default().background(colors.gray.gray_300)
}

fn project_pane_title_style(theme: &Theme) -> iced::widget::container::Style {
    let colors = ui::tokens::SpectrumColors::from_theme(theme);

    iced::widget::container::Style::default()
        .background(colors.gray.gray_50)
        .border(Border {
            color: colors.gray.gray_300,
            width: 1.0,
            ..Border::default()
        })
}

fn status_bar_style(theme: &Theme) -> iced::widget::container::Style {
    let colors = ui::tokens::SpectrumColors::from_theme(theme);

    iced::widget::container::Style::default()
        .background(colors.gray.gray_50)
        .border(Border {
            color: colors.gray.gray_300,
            width: 1.0,
            ..Border::default()
        })
}

fn modal_backdrop_style(_theme: &Theme) -> iced::widget::container::Style {
    iced::widget::container::Style::default().background(Color::from_rgba(0.0, 0.0, 0.0, 0.32))
}

fn modal_dialog_style(theme: &Theme) -> iced::widget::container::Style {
    let colors = ui::tokens::SpectrumColors::from_theme(theme);

    iced::widget::container::Style {
        background: Some(iced::Background::Color(colors.gray.gray_50)),
        border: Border {
            color: colors.gray.gray_300,
            width: 1.0,
            radius: ui::tokens::dimension::CORNER_RADIUS_500.into(),
        },
        shadow: iced::Shadow {
            color: Color::from_rgba(0.0, 0.0, 0.0, 0.24),
            offset: iced::Vector::new(0.0, 4.0),
            blur_radius: 12.0,
        },
        ..iced::widget::container::Style::default()
    }
}

fn message_action_button<'a>(
    label: &'a str,
    message: Message,
    enabled: bool,
) -> iced::widget::Button<'a, Message> {
    ui::action_button(
        label,
        enabled.then_some(message),
        ui::ActionButtonOptions::STANDARD,
    )
}

fn message_icon_button<'a>(
    symbol: &'a str,
    description: &'a str,
    message: Message,
    enabled: bool,
) -> Element<'a, Message> {
    ui::icon_action_button(
        symbol,
        description,
        enabled.then_some(message),
        ui::ActionButtonOptions::STANDARD,
    )
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

async fn save_documents(requests: Vec<SaveRequest>, prompt_for_drafts: bool) -> Vec<SaveOutcome> {
    let mut outcomes = Vec::with_capacity(requests.len());

    for request in requests {
        let outcome = if let Some(path) = request.path {
            write_document(request.document_id, path, request.source).await
        } else if prompt_for_drafts {
            save_document_as(
                request.document_id,
                request.directory,
                request.file_name,
                request.source,
            )
            .await
        } else {
            continue;
        };
        let cancelled = matches!(outcome, SaveOutcome::Cancelled { .. });
        outcomes.push(outcome);
        if cancelled {
            break;
        }
    }

    outcomes
}

async fn choose_export_path(
    directory: PathBuf,
    file_name: String,
    format: compiler::ExportFormat,
) -> ExportPathOutcome {
    let title = format!("Exportar documento como {}", format.label());
    let filter = format!("Documento {}", format.label());
    let Some(file) = AsyncFileDialog::new()
        .add_filter(&filter, &[format.extension()])
        .set_directory(directory)
        .set_file_name(file_name)
        .set_title(title)
        .save_file()
        .await
    else {
        return ExportPathOutcome::Cancelled;
    };

    ExportPathOutcome::Selected(with_export_extension(file.path(), format))
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

async fn write_export(
    path: PathBuf,
    artifact: Vec<u8>,
    format: compiler::ExportFormat,
) -> ExportWriteOutcome {
    let destination = path.clone();
    let result =
        tokio::task::spawn_blocking(move || atomic_write_file(&destination, &artifact)).await;

    match result {
        Ok(Ok(())) => ExportWriteOutcome::Saved { format, path },
        Ok(Err(error)) => ExportWriteOutcome::Failed {
            format,
            error: format!("{}: {error}", path.display()),
        },
        Err(error) => ExportWriteOutcome::Failed {
            format,
            error: format!(
                "{}: tarefa de exportação interrompida: {error}",
                path.display()
            ),
        },
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

fn menu_bar_pointer_subscription() -> Subscription<Message> {
    event::listen_with(|event, _status, _window| match event {
        iced::Event::Mouse(mouse::Event::CursorMoved { position }) => {
            Some(Message::CursorMoved(position))
        }
        iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
        | iced::Event::Mouse(mouse::Event::CursorLeft)
        | iced::Event::Touch(iced::touch::Event::FingerLifted { .. })
        | iced::Event::Touch(iced::touch::Event::FingerLost { .. }) => {
            Some(Message::MenuBarPointerReleased)
        }
        _ => None,
    })
}

fn file_drop_subscription() -> Subscription<Message> {
    event::listen_with(|event, _status, window| match event {
        iced::Event::Window(window::Event::FileDropped(path)) => {
            Some(Message::FileDropped(window, path))
        }
        _ => None,
    })
}

fn project_tree_keyboard_subscription() -> Subscription<Message> {
    event::listen_with(|event, status, _window| {
        if status == event::Status::Captured {
            return None;
        }
        let iced::Event::Keyboard(keyboard::Event::KeyPressed { key, .. }) = event else {
            return None;
        };
        let keyboard::Key::Named(key) = key else {
            return None;
        };
        let navigation = match key {
            keyboard::key::Named::ArrowUp => TreeNavigation::Previous,
            keyboard::key::Named::ArrowDown => TreeNavigation::Next,
            keyboard::key::Named::ArrowLeft => TreeNavigation::ParentOrCollapse,
            keyboard::key::Named::ArrowRight => TreeNavigation::ChildOrExpand,
            keyboard::key::Named::Home => TreeNavigation::First,
            keyboard::key::Named::End => TreeNavigation::Last,
            keyboard::key::Named::Enter | keyboard::key::Named::Space => TreeNavigation::Activate,
            _ => return None,
        };
        Some(Message::ProjectTreeNavigate(navigation))
    })
}

fn shortcut_subscription() -> Subscription<Message> {
    event::listen_with(|event, _status, _window| {
        let iced::Event::Keyboard(event) = event else {
            return None;
        };
        let keyboard::Event::KeyPressed {
            key,
            physical_key,
            modifiers,
            repeat,
            ..
        } = event
        else {
            return match event {
                keyboard::Event::ModifiersChanged(modifiers) => {
                    Some(Message::ModifiersChanged(modifiers))
                }
                _ => None,
            };
        };

        if repeat {
            return None;
        }

        match key.as_ref() {
            keyboard::Key::Named(keyboard::key::Named::Tab) if modifiers.control() => {
                return Some(Message::ActivateRelativeDocument(modifiers.shift()));
            }
            keyboard::Key::Named(keyboard::key::Named::PageUp)
                if modifiers.command() && modifiers.shift() =>
            {
                return Some(Message::MoveActiveDocument(true));
            }
            keyboard::Key::Named(keyboard::key::Named::PageDown)
                if modifiers.command() && modifiers.shift() =>
            {
                return Some(Message::MoveActiveDocument(false));
            }
            keyboard::Key::Named(keyboard::key::Named::F3) => {
                return Some(if modifiers.shift() {
                    Message::SearchPrevious
                } else {
                    Message::SearchNext
                });
            }
            keyboard::Key::Named(keyboard::key::Named::Escape) => {
                return Some(Message::EscapePressed);
            }
            _ => {}
        }

        shortcut_message(key.to_latin(physical_key)?, modifiers)
    })
}

fn menu_keyboard_subscription() -> Subscription<Message> {
    event::listen_with(|event, _status, _window| {
        let iced::Event::Keyboard(event) = event else {
            return None;
        };
        let keyboard::Event::KeyPressed { key, repeat, .. } = event else {
            return match event {
                keyboard::Event::ModifiersChanged(modifiers) => {
                    Some(Message::ModifiersChanged(modifiers))
                }
                _ => None,
            };
        };
        let keyboard::Key::Named(key) = key else {
            return None;
        };

        let navigation = match key {
            keyboard::key::Named::ArrowDown => MenuNavigation::NextItem,
            keyboard::key::Named::ArrowUp => MenuNavigation::PreviousItem,
            keyboard::key::Named::Home => MenuNavigation::FirstItem,
            keyboard::key::Named::End => MenuNavigation::LastItem,
            keyboard::key::Named::ArrowRight => MenuNavigation::NextMenu,
            keyboard::key::Named::ArrowLeft => MenuNavigation::PreviousMenu,
            keyboard::key::Named::Enter | keyboard::key::Named::Space if !repeat => {
                MenuNavigation::Activate
            }
            keyboard::key::Named::Escape if !repeat => return Some(Message::DismissMenu),
            _ => return None,
        };

        Some(Message::MenuNavigate(navigation))
    })
}

fn alert_dialog_keyboard_subscription() -> Subscription<Message> {
    event::listen_with(|event, _status, _window| {
        let iced::Event::Keyboard(keyboard::Event::KeyPressed {
            key: keyboard::Key::Named(keyboard::key::Named::Escape),
            repeat: false,
            ..
        }) = event
        else {
            return None;
        };

        Some(Message::DismissAlertDialog)
    })
}

fn shortcut_message(key: char, modifiers: keyboard::Modifiers) -> Option<Message> {
    if modifiers.command() && modifiers.alt() && key.eq_ignore_ascii_case(&'s') {
        return Some(Message::SaveAllDocuments);
    }
    if !modifiers.command() || modifiers.alt() {
        return None;
    }

    match (key.to_ascii_lowercase(), modifiers.shift()) {
        ('+' | '=', _) => Some(Message::PreviewZoomIn),
        ('-', _) => Some(Message::PreviewZoomOut),
        ('0', _) => Some(Message::PreviewZoomReset),
        ('n', false) => Some(Message::NewDocument),
        ('o', false) => Some(Message::OpenDocument),
        ('o', true) => Some(Message::OpenProject),
        ('s', false) => Some(Message::SaveDocument),
        ('s', true) => Some(Message::SaveDocumentAs),
        ('q', false) => Some(Message::ExitApplication),
        ('w', false) => Some(Message::CloseActiveDocument),
        ('t', true) => Some(Message::ReopenClosedDocument),
        ('b', false) => Some(Message::Bold),
        ('i', false) => Some(Message::Italic),
        ('u', false) => Some(Message::Underline),
        ('f', false) => Some(Message::OpenSearch),
        ('f', true) => Some(Message::OpenProjectSearch),
        ('h', false) => Some(Message::OpenReplace),
        ('j', false) => Some(Message::RevealInPreview),
        _ => None,
    }
}

fn open_external_url(url: &str) -> io::Result<()> {
    #[cfg(target_os = "windows")]
    {
        ProcessCommand::new("cmd")
            .args(["/C", "start", "", url])
            .spawn()
            .map(|_| ())
    }

    #[cfg(target_os = "macos")]
    {
        ProcessCommand::new("open").arg(url).spawn().map(|_| ())
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        ProcessCommand::new("xdg-open").arg(url).spawn().map(|_| ())
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos", unix)))]
    {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "sistema operacional sem abridor de URL configurado",
        ))
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

fn with_export_extension(path: &Path, format: compiler::ExportFormat) -> PathBuf {
    let mut path = path.to_path_buf();

    if path
        .extension()
        .is_none_or(|extension| extension.is_empty())
    {
        path.set_extension(format.extension());
    }

    path
}

fn export_file_name(document_name: &str, format: compiler::ExportFormat) -> String {
    let mut path = PathBuf::from(document_name);
    path.set_extension(format.extension());
    path.to_string_lossy().into_owned()
}

fn project_display_name(root: &Path) -> String {
    root.file_name()
        .filter(|name| !name.is_empty())
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| root.to_string_lossy().into_owned())
}

fn problems_navigation_label(errors: usize, warnings: usize) -> String {
    let mut counts = Vec::with_capacity(2);
    if errors > 0 {
        counts.push(format!(
            "{errors} {}",
            if errors == 1 { "erro" } else { "erros" }
        ));
    }
    if warnings > 0 {
        counts.push(format!(
            "{warnings} {}",
            if warnings == 1 { "aviso" } else { "avisos" }
        ));
    }

    if counts.is_empty() {
        "Problemas".to_owned()
    } else {
        format!("Problemas: {}", counts.join(", "))
    }
}

fn find_project_entry_kind(
    entries: &[project::ProjectEntry],
    path: &Path,
) -> Option<project::EntryKind> {
    entries.iter().find_map(|entry| {
        if entry.path == path {
            Some(entry.kind)
        } else {
            find_project_entry_kind(&entry.children, path)
        }
    })
}

fn project_entry_contains_path(entry: &Path, kind: project::EntryKind, candidate: &Path) -> bool {
    if kind == project::EntryKind::Directory {
        candidate.starts_with(entry)
    } else {
        candidate == entry
    }
}

fn collect_project_files(entries: &[project::ProjectEntry]) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for entry in entries {
        if entry.kind == project::EntryKind::Directory {
            files.extend(collect_project_files(&entry.children));
        } else {
            files.push(entry.path.clone());
        }
    }
    files
}

fn append_visible_project_entries(
    source: &[project::ProjectEntry],
    expanded: &HashSet<PathBuf>,
    target: &mut Vec<(PathBuf, project::EntryKind)>,
) {
    for entry in source {
        target.push((entry.path.clone(), entry.kind));
        if entry.kind == project::EntryKind::Directory && expanded.contains(&entry.path) {
            append_visible_project_entries(&entry.children, expanded, target);
        }
    }
}

fn remap_project_path(
    path: &Path,
    from: &Path,
    to: &Path,
    kind: project::EntryKind,
) -> Option<PathBuf> {
    if !project_entry_contains_path(from, kind, path) {
        return None;
    }

    if kind == project::EntryKind::Directory {
        path.strip_prefix(from).ok().map(|suffix| to.join(suffix))
    } else {
        Some(to.to_path_buf())
    }
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

    fn menu_labels(entries: &[AppMenuEntry]) -> Vec<&str> {
        entries
            .iter()
            .filter_map(|entry| match entry {
                AppMenuEntry::Item { label, .. } => Some(label.as_str()),
                AppMenuEntry::Divider => None,
            })
            .collect()
    }

    fn mapped_preview_page(regions: Vec<source_map::SourceRegion>) -> PreviewPage {
        PreviewPage {
            handle: svg::Handle::from_memory(
                br#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100"/>"#,
            ),
            width: 100.0,
            height: 100.0,
            regions,
        }
    }

    fn mapped_region(
        target: source_map::SourceTarget,
        range: Range<usize>,
        bounds: source_map::SourceBounds,
        kind: source_map::SourceRegionKind,
    ) -> source_map::SourceRegion {
        source_map::SourceRegion {
            target,
            range,
            bounds,
            kind,
        }
    }

    #[test]
    fn application_window_starts_maximized() {
        let settings = app_window_settings();

        assert!(settings.maximized);
        assert_eq!(settings.size, iced::Size::new(1200.0, 800.0));
        assert!(matches!(settings.position, window::Position::Centered));
        assert!(!settings.exit_on_close_request);
    }

    #[test]
    fn settings_use_a_separate_compact_window() {
        let settings = settings_window_settings();

        assert_eq!(
            settings.size,
            iced::Size::new(SETTINGS_WINDOW_WIDTH, SETTINGS_WINDOW_HEIGHT)
        );
        assert_eq!(settings.min_size, Some(iced::Size::new(520.0, 480.0)));
        assert!(!settings.maximized);
        assert_eq!(settings.level, window::Level::AlwaysOnTop);
        assert!(!settings.exit_on_close_request);
    }

    #[test]
    fn settings_window_is_reused_and_cleared_when_closed() {
        let mut app = App::new();

        let _ = app.update(Message::OpenSettings);
        let first = app.settings_window.expect("settings window should be open");
        let _ = app.update(Message::OpenSettings);
        assert_eq!(app.settings_window, Some(first));

        let _ = app.update(Message::CloseRequested(first));
        assert!(app.settings_window.is_none());
    }

    #[test]
    fn settings_categories_and_export_shortcut_select_the_expected_page() {
        let mut app = App::new();

        assert_eq!(app.settings_page, SettingsPage::Editor);
        let _ = app.update(Message::SettingsPageSelected(SettingsPage::Preview));
        assert_eq!(app.settings_page, SettingsPage::Preview);

        let _ = app.update(Message::OpenExportSettings);
        assert_eq!(app.settings_page, SettingsPage::Export);
        assert!(app.settings_window.is_some());
        assert_eq!(SettingsPage::ALL.len(), 4);
    }

    #[test]
    fn preview_canvas_fills_the_viewport_and_grows_for_wide_pages() {
        assert_eq!(preview_canvas_width(700.0, 580.0), 700.0);
        assert_eq!(preview_canvas_width(700.0, 900.0), 924.0);
    }

    #[test]
    fn preview_at_100_percent_uses_the_detected_monitor_density() {
        const TOLERANCE: f32 = 0.000_001;
        const MONITOR_PPI: f32 = 92.36;

        assert!((preview_scale(100, MONITOR_PPI) - MONITOR_PPI / 72.0).abs() < TOLERANCE);
    }

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
        for (format, expected) in [
            (compiler::ExportFormat::Pdf, "documento.pdf"),
            (compiler::ExportFormat::Svg, "documento.svg"),
            (compiler::ExportFormat::Html, "documento.html"),
        ] {
            assert_eq!(
                with_export_extension(Path::new("documento"), format),
                PathBuf::from(expected)
            );
            assert_eq!(export_file_name("documento.typ", format), expected);
        }
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
        assert_eq!(app.panes.len(), 3);
        assert_eq!(
            app.panes
                .iter()
                .filter(|(_, pane)| matches!(pane, Pane::Project))
                .count(),
            1
        );
    }

    #[test]
    fn fixed_main_keeps_the_preview_target_while_an_import_is_edited() {
        let directory = tempfile::tempdir().expect("a temporary project can be created");
        let main_path = directory.path().join("main.typ");
        let part_path = directory.path().join("part.typ");
        let mut app = App::new();
        app.workspace_root = directory.path().to_path_buf();
        app.document = Documents::new(Document::opened(
            main_path.clone(),
            "#include \"part.typ\"".to_owned(),
        ));
        let main = app.document.active_id();
        let part = app
            .document
            .add(Document::opened(part_path.clone(), "Saved".to_owned()));
        app.project_main = Some(main_path);
        app.document.activate(main);
        app.preview = vec![mapped_preview_page(Vec::new())];
        let config = app.compiler_config();
        let revision = app.compilation_revision;

        assert!(app.activate_document(part));
        let _ = app.update(Message::Editor(Action::Insert("Unsaved ".to_owned())));

        assert_eq!(app.compiler_config(), config);
        assert_eq!(
            app.compilation_source().as_deref(),
            Some("#include \"part.typ\"")
        );
        assert!(app.compilation_revision > revision);
        assert_eq!(app.preview.len(), 1);
        assert_eq!(
            app.active_source_target(),
            Some(source_map::SourceTarget::ProjectFile(part_path.clone()))
        );
        assert_eq!(
            app.source_overlays(),
            vec![SourceOverlay {
                path: part_path,
                text: "Unsaved Saved".to_owned(),
            }]
        );
    }

    #[test]
    fn a_closed_fixed_main_falls_back_to_its_disk_source() {
        let directory = tempfile::tempdir().expect("a temporary project can be created");
        let main_path = directory.path().join("main.typ");
        let part_path = directory.path().join("part.typ");
        let mut app = App::new();
        app.workspace_root = directory.path().to_path_buf();
        app.document = Documents::new(Document::opened(main_path.clone(), "Main".to_owned()));
        let main = app.document.active_id();
        app.document
            .add(Document::opened(part_path, "Part".to_owned()));
        app.project_main = Some(main_path.clone());

        app.close_document(main);

        assert_eq!(app.project_main.as_deref(), Some(main_path.as_path()));
        assert_eq!(app.compilation_source(), None);
        assert_eq!(app.compilation_display_name(), "main.typ");
    }

    #[test]
    fn closing_a_dirty_document_starts_confirmation() {
        let mut app = App::new();
        let main_window = app.main_window;

        let _ = app.update(Message::CloseRequested(main_window));

        assert!(app.document.is_dirty());
        assert!(app.file_busy);
        assert!(app.pending_after_save.is_none());
        assert!(matches!(
            app.pending_alert_dialog,
            Some(PendingAlertDialog::Unsaved {
                action: DestructiveFileAction::Close(id),
                ..
            }) if id == main_window
        ));
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
        let command = keyboard::Modifiers::COMMAND;

        assert!(matches!(
            shortcut_message('n', command),
            Some(Message::NewDocument)
        ));
        assert!(matches!(
            shortcut_message('o', command),
            Some(Message::OpenDocument)
        ));
        assert!(matches!(
            shortcut_message('o', command | keyboard::Modifiers::SHIFT),
            Some(Message::OpenProject)
        ));
        assert!(matches!(
            shortcut_message('s', command),
            Some(Message::SaveDocument)
        ));
        assert!(matches!(
            shortcut_message('s', command | keyboard::Modifiers::SHIFT),
            Some(Message::SaveDocumentAs)
        ));
        assert!(matches!(
            shortcut_message('q', command),
            Some(Message::ExitApplication)
        ));
        assert!(matches!(
            shortcut_message('b', command),
            Some(Message::Bold)
        ));
        assert!(matches!(
            shortcut_message('i', command),
            Some(Message::Italic)
        ));
        assert!(matches!(
            shortcut_message('u', command),
            Some(Message::Underline)
        ));
        assert!(matches!(
            shortcut_message('f', command),
            Some(Message::OpenSearch)
        ));
        assert!(matches!(
            shortcut_message('h', command),
            Some(Message::OpenReplace)
        ));
        assert!(matches!(
            shortcut_message('j', command),
            Some(Message::RevealInPreview)
        ));
        assert!(matches!(
            shortcut_message('f', command | keyboard::Modifiers::SHIFT),
            Some(Message::OpenProjectSearch)
        ));
        assert!(matches!(
            shortcut_message('w', command),
            Some(Message::CloseActiveDocument)
        ));
        assert!(matches!(
            shortcut_message('t', command | keyboard::Modifiers::SHIFT),
            Some(Message::ReopenClosedDocument)
        ));
        assert!(matches!(
            shortcut_message('s', command | keyboard::Modifiers::ALT),
            Some(Message::SaveAllDocuments)
        ));
        assert!(shortcut_message('s', keyboard::Modifiers::NONE).is_none());
        assert!(shortcut_message('n', command | keyboard::Modifiers::SHIFT).is_none());
    }

    #[test]
    fn menu_navigation_skips_disabled_items_and_dividers() {
        let entries = vec![
            AppMenuEntry::item(
                "Desabilitado",
                None,
                false,
                false,
                MenuCommand::SaveDocument,
            ),
            AppMenuEntry::Divider,
            AppMenuEntry::item("Primeiro", None, false, true, MenuCommand::OpenSearch),
            AppMenuEntry::item("Segundo", None, false, true, MenuCommand::OpenReplace),
        ];

        assert_eq!(enabled_menu_items(&entries), vec![1, 2]);
        assert_eq!(first_enabled_menu_item(&entries), Some(1));
        assert!(menu_command_at(&entries, 0).is_none());
        assert!(matches!(
            menu_command_at(&entries, 1),
            Some(MenuCommand::OpenSearch)
        ));
    }

    #[test]
    fn export_overflow_contains_related_actions_and_can_be_dismissed() {
        let mut app = App::new();
        let entries = app.export_menu_entries();
        let labels = menu_labels(&entries);

        assert_eq!(
            labels,
            vec![
                "Exportar como SVG…",
                "Exportar como HTML…",
                "Configurações de exportação…",
            ]
        );

        let _ = app.update(Message::ToggleExportMenu);
        assert!(app.export_menu_visible);
        let _ = app.update(Message::DismissMenu);
        assert!(!app.export_menu_visible);
    }

    #[test]
    fn project_pane_can_be_hidden_and_restored() {
        let mut app = App::new();
        assert!(app.project_pane_visible());
        assert_eq!(app.panes.iter().count(), 3);

        app.toggle_project_pane();
        assert!(!app.project_pane_visible());
        assert_eq!(app.panes.iter().count(), 2);

        app.toggle_project_pane();
        assert!(app.project_pane_visible());
        assert_eq!(app.panes.iter().count(), 3);
    }

    #[test]
    fn opening_problems_restores_the_side_panel() {
        let mut app = App::new();
        app.toggle_project_pane();
        assert!(!app.project_pane_visible());

        let _ = app.update(Message::OpenProblems);

        assert!(app.project_pane_visible());
        assert_eq!(app.project_navigation, ProjectNavigation::Problems);
    }

    #[test]
    fn application_menus_cycle_in_both_directions() {
        assert_eq!(AppMenu::File.previous(), AppMenu::Help);
        assert_eq!(AppMenu::File.next(), AppMenu::Edit);
        assert_eq!(AppMenu::Help.next(), AppMenu::File);
        assert_eq!(AppMenu::Help.previous(), AppMenu::View);
    }

    #[test]
    fn pointer_drag_switches_between_menu_bar_submenus() {
        let mut app = App::new();

        let _ = app.update(Message::MenuBarPointerEntered(AppMenu::Edit));
        assert_eq!(app.open_menu, None);

        let _ = app.update(Message::MenuBarPointerPressed(AppMenu::File));
        assert_eq!(app.open_menu, Some(AppMenu::File));
        assert!(app.menu_bar_drag_active);

        let _ = app.update(Message::MenuBarPointerEntered(AppMenu::View));
        assert_eq!(app.open_menu, Some(AppMenu::View));

        let _ = app.update(Message::MenuBarPointerReleased);
        assert!(!app.menu_bar_drag_active);
        assert_eq!(app.open_menu, Some(AppMenu::View));

        let _ = app.update(Message::MenuBarPointerEntered(AppMenu::Help));
        assert_eq!(app.open_menu, Some(AppMenu::Help));

        let _ = app.update(Message::DismissMenu);
        let _ = app.update(Message::MenuBarPointerEntered(AppMenu::File));
        assert_eq!(app.open_menu, None);
    }

    #[test]
    fn source_navigation_selects_the_page_matching_the_cursor() {
        let pages = vec![
            mapped_preview_page(vec![mapped_region(
                source_map::SourceTarget::Main,
                40..45,
                source_map::SourceBounds {
                    x: 10.0,
                    y: 10.0,
                    width: 5.0,
                    height: 10.0,
                },
                source_map::SourceRegionKind::Text,
            )]),
            mapped_preview_page(vec![mapped_region(
                source_map::SourceTarget::Main,
                10..20,
                source_map::SourceBounds {
                    x: 20.0,
                    y: 30.0,
                    width: 8.0,
                    height: 10.0,
                },
                source_map::SourceRegionKind::Text,
            )]),
        ];

        let (page, region) = find_source_region(&pages, &source_map::SourceTarget::Main, 15)
            .expect("the cursor has a mapped preview region");

        assert_eq!(page, 1);
        assert_eq!(region.range, 10..20);
    }

    #[test]
    fn preview_hit_testing_prefers_text_over_an_overlapping_shape() {
        let bounds = source_map::SourceBounds {
            x: 10.0,
            y: 10.0,
            width: 20.0,
            height: 20.0,
        };
        let page = mapped_preview_page(vec![
            mapped_region(
                source_map::SourceTarget::Main,
                0..5,
                bounds,
                source_map::SourceRegionKind::Shape,
            ),
            mapped_region(
                source_map::SourceTarget::Main,
                8..9,
                bounds,
                source_map::SourceRegionKind::Text,
            ),
        ]);

        let region = find_preview_region(&page, 15.0, 15.0)
            .expect("the click intersects both mapped regions");

        assert_eq!(region.kind, source_map::SourceRegionKind::Text);
        assert_eq!(region.range, 8..9);
    }

    #[test]
    fn editor_to_preview_navigation_rejects_a_stale_map() {
        let mut app = App::new();
        app.preview = vec![mapped_preview_page(vec![mapped_region(
            source_map::SourceTarget::Main,
            0..4,
            source_map::SourceBounds {
                x: 10.0,
                y: 10.0,
                width: 20.0,
                height: 10.0,
            },
            source_map::SourceRegionKind::Text,
        )])];
        app.preview_revision = Some(app.compilation_revision + 1);
        app.preview_status = PreviewStatus::Ready {
            pages: 1,
            warnings: 0,
        };

        let _ = app.reveal_cursor_in_preview();

        assert!(app.preview_highlight.is_none());
        assert!(
            app.file_status
                .as_deref()
                .is_some_and(|status| status.contains("preview atualizado"))
        );
    }

    #[test]
    fn command_click_in_the_editor_reveals_the_mapped_preview_region() {
        let mut app = App::new();
        app.preview = vec![mapped_preview_page(vec![mapped_region(
            source_map::SourceTarget::Main,
            5..10,
            source_map::SourceBounds {
                x: 12.0,
                y: 24.0,
                width: 8.0,
                height: 10.0,
            },
            source_map::SourceRegionKind::Text,
        )])];
        app.preview_revision = Some(app.compilation_revision);
        app.preview_status = PreviewStatus::Ready {
            pages: 1,
            warnings: 0,
        };
        app.modifiers = keyboard::Modifiers::COMMAND;

        let _ = app.update(Message::Editor(Action::MoveTo(7)));

        assert!(app.preview_highlight.is_some_and(|highlight| {
            highlight.page == 0 && highlight.bounds.x == 12.0 && highlight.bounds.y == 24.0
        }));
    }

    #[test]
    fn fixed_main_locates_the_active_import_in_the_preview() {
        let directory = tempfile::tempdir().expect("a temporary project can be created");
        let main_path = directory.path().join("main.typ");
        let part_path = directory.path().join("part.typ");
        let mut app = App::new();
        app.workspace_root = directory.path().to_path_buf();
        app.document = Documents::new(Document::opened(main_path.clone(), "main".to_owned()));
        app.document.add(Document::opened(
            part_path.clone(),
            "imported text".to_owned(),
        ));
        app.project_main = Some(main_path);
        app.document.perform(Action::MoveTo(5));
        app.preview = vec![mapped_preview_page(vec![mapped_region(
            source_map::SourceTarget::ProjectFile(part_path),
            2..8,
            source_map::SourceBounds {
                x: 18.0,
                y: 26.0,
                width: 12.0,
                height: 10.0,
            },
            source_map::SourceRegionKind::Text,
        )])];
        app.preview_revision = Some(app.compilation_revision);
        app.preview_status = PreviewStatus::Ready {
            pages: 1,
            warnings: 0,
        };

        let _ = app.reveal_cursor_in_preview();

        assert!(app.preview_highlight.is_some_and(|highlight| {
            highlight.page == 0 && highlight.bounds.x == 18.0 && highlight.bounds.y == 26.0
        }));
    }

    #[test]
    fn preview_click_uses_zoom_and_activates_an_imported_source() {
        let directory = tempfile::tempdir().expect("a temporary project can be created");
        let main_path = directory.path().join("main.typ");
        let part_path = directory.path().join("part.typ");
        let mut app = App::new();
        app.workspace_root = directory.path().to_path_buf();
        app.document = Documents::new(Document::opened(main_path.clone(), "main".to_owned()));
        app.project_main = Some(main_path);
        let part = app.document.add(Document::opened(
            part_path.clone(),
            "imported text".to_owned(),
        ));
        let main = app
            .document
            .iter()
            .find_map(|(id, document)| (document.path() != Some(&part_path)).then_some(id))
            .expect("the main document is open");
        app.document.activate(main);
        app.settings.preview_zoom = 200;
        app.preview = vec![mapped_preview_page(vec![mapped_region(
            source_map::SourceTarget::ProjectFile(part_path),
            2..8,
            source_map::SourceBounds {
                x: 10.0,
                y: 20.0,
                width: 20.0,
                height: 10.0,
            },
            source_map::SourceRegionKind::Text,
        )])];
        app.preview_revision = Some(app.compilation_revision);
        app.preview_status = PreviewStatus::Ready {
            pages: 1,
            warnings: 0,
        };
        let scale = preview_scale(200, app.preview_logical_ppi);
        app.preview_pointer = Some(PreviewPointer {
            page: 0,
            position: Point::new(15.0 * scale, 25.0 * scale),
        });

        let _ = app.reveal_preview_source(0);

        assert_eq!(app.document.active_id(), part);
        assert_eq!(app.document.content().selection(), 2..8);
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
            snapshot: Ok(project::ProjectSnapshot {
                entries: Vec::new(),
                typst_files: vec![PathBuf::from("/projeto/antigo/main.typ")],
            }),
        }));

        assert!(app.project_tree.is_empty());
        assert!(app.project_scan_busy);
    }

    #[test]
    fn project_side_navigation_switches_between_all_destinations() {
        let mut app = App::new();

        assert_eq!(app.project_navigation, ProjectNavigation::Files);
        assert_eq!(app.project_navigation.title(), "Arquivos");

        let _ = app.update(Message::ProjectNavigationSelected(
            ProjectNavigation::Topics,
        ));

        assert_eq!(app.project_navigation, ProjectNavigation::Topics);
        assert_eq!(app.project_navigation.title(), "Sumário");

        let _ = app.update(Message::ProjectNavigationSelected(
            ProjectNavigation::Problems,
        ));

        assert_eq!(app.project_navigation, ProjectNavigation::Problems);
        assert_eq!(app.project_navigation.title(), "Problemas");
    }

    #[test]
    fn document_outline_tracks_the_cursor_and_toggles_branches() {
        let source = "= Introdução\nTexto\n== Detalhes\nMais texto";
        let details_start = source.find("== Detalhes").expect("the heading exists");
        let mut app = App::new();
        app.document = Documents::new(Document::draft(source));
        app.install_document_outline(vec![compiler::DocumentOutlineItem {
            title: "Introdução".to_owned(),
            target: source_map::SourceTarget::Main,
            range: 0.."= Introdução".len(),
            children: vec![compiler::DocumentOutlineItem {
                title: "Detalhes".to_owned(),
                target: source_map::SourceTarget::Main,
                range: details_start..details_start + "== Detalhes".len(),
                children: Vec::new(),
            }],
        }]);

        let _ = app
            .document
            .perform(Action::MoveTo(details_start + "== Detalhes".len()));
        assert_eq!(
            app.current_outline_key(),
            Some(OutlineKey::new(
                source_map::SourceTarget::Main,
                details_start
            ))
        );

        let _ = app.update(Message::DocumentOutlinePressed {
            target: source_map::SourceTarget::Main,
            range: 0.."= Introdução".len(),
            has_children: true,
        });
        let root_key = OutlineKey::new(source_map::SourceTarget::Main, 0);
        assert!(app.collapsed_outline_entries.contains(&root_key));
        assert_eq!(
            app.document.selection_text().as_deref(),
            Some("= Introdução")
        );
        assert!(
            app.document_outline_items(&app.document_outline, None)[0]
                .children
                .is_empty()
        );

        let _ = app.update(Message::DocumentOutlinePressed {
            target: source_map::SourceTarget::Main,
            range: 0.."= Introdução".len(),
            has_children: true,
        });
        assert!(!app.collapsed_outline_entries.contains(&root_key));
        assert_eq!(
            app.document_outline_items(&app.document_outline, None)[0]
                .children
                .len(),
            1
        );
    }

    #[test]
    fn imported_outline_heading_opens_its_source_document() {
        let directory = tempfile::tempdir().expect("a temporary project can be created");
        let main_path = directory.path().join("main.typ");
        let chapter_path = directory.path().join("chapter.typ");
        let mut app = App::new();
        app.workspace_root = directory.path().to_path_buf();
        app.project_main = Some(main_path.clone());
        app.document = Documents::new(Document::opened(
            main_path,
            "#include \"chapter.typ\"".to_owned(),
        ));
        let main = app.document.active_id();
        app.document.add(Document::opened(
            chapter_path.clone(),
            "= Capítulo\nTexto".to_owned(),
        ));
        app.document.activate(main);

        let _ = app.update(Message::DocumentOutlinePressed {
            target: source_map::SourceTarget::ProjectFile(chapter_path.clone()),
            range: 0.."= Capítulo".len(),
            has_children: false,
        });

        assert_eq!(app.document.path(), Some(chapter_path.as_path()));
        assert_eq!(app.document.selection_text().as_deref(), Some("= Capítulo"));
    }

    #[test]
    fn project_tree_displays_an_expandable_named_root() {
        let directory = tempfile::tempdir().expect("a temporary project can be created");
        let root = directory.path().to_path_buf();
        let mut app = App::build(
            Documents::new(Document::new()),
            panes_from_layout(session::PaneLayout::default()),
            root.clone(),
            None,
            None,
            settings::Settings::default(),
            "Projeto aberto".to_owned(),
        );

        let root_item = app.project_tree_root_item();
        assert_eq!(root_item.label, project_display_name(&root).to_uppercase());
        assert_eq!(root_item.icon, None);
        assert_eq!(root_item.reserve_icon_space, Some(false));
        assert!(root_item.expanded);
        assert!(root_item.has_children);
        assert_eq!(root_item.actions.len(), 3);
        assert_eq!(root_item.actions[0].icon, ui::WorkflowIcon::FileAdd);
        assert_eq!(root_item.actions[1].icon, ui::WorkflowIcon::FolderAdd);
        assert_eq!(root_item.actions[2].icon, ui::WorkflowIcon::Refresh);
        assert_eq!(root_item.children.len(), 1);

        app.project_scan_busy = true;
        let _ = app.update(Message::ProjectScanned(project::ScanOutcome {
            root: root.clone(),
            snapshot: Ok(project::ProjectSnapshot {
                entries: Vec::new(),
                typst_files: Vec::new(),
            }),
        }));
        assert!(app.expanded_project_directories.contains(&root));

        let _ = app.update(Message::ProjectEntryPressed(
            root.clone(),
            project::EntryKind::Directory,
        ));
        let root_item = app.project_tree_root_item();
        assert!(root_item.selected);
        assert!(!root_item.expanded);
        assert!(root_item.children.is_empty());
        assert!(
            app.file_status
                .as_deref()
                .is_some_and(|status| status.contains("Projeto") && status.contains("recolhido"))
        );
    }

    #[test]
    fn project_tree_keeps_hierarchy_and_toggles_directories() {
        let directory = tempfile::tempdir().expect("a temporary project can be created");
        let chapters = directory.path().join("chapters");
        let chapter = chapters.join("one.typ");
        let entries = vec![project::ProjectEntry {
            path: chapters.clone(),
            kind: project::EntryKind::Directory,
            children: vec![project::ProjectEntry {
                path: chapter.clone(),
                kind: project::EntryKind::TypstFile,
                children: Vec::new(),
            }],
        }];
        let mut app = App::new();
        app.workspace_root = directory.path().to_path_buf();
        app.project_scan_busy = true;

        let _ = app.update(Message::ProjectScanned(project::ScanOutcome {
            root: directory.path().to_path_buf(),
            snapshot: Ok(project::ProjectSnapshot {
                entries: entries.clone(),
                typst_files: vec![chapter],
            }),
        }));

        assert_eq!(app.project_tree, entries);
        assert!(!app.project_scan_busy);

        let _ = app.update(Message::ProjectEntryPressed(
            chapters.clone(),
            project::EntryKind::Directory,
        ));
        assert_eq!(app.selected_project_entry, Some(chapters.clone()));
        assert!(app.expanded_project_directories.contains(&chapters));
        assert!(app.selected_project_file.is_none());

        let _ = app.update(Message::ProjectEntryPressed(
            chapters.clone(),
            project::EntryKind::Directory,
        ));
        assert!(!app.expanded_project_directories.contains(&chapters));
    }

    #[test]
    fn project_context_menu_selects_without_opening_the_entry() {
        let directory = tempfile::tempdir().expect("a temporary project can be created");
        let path = directory.path().join("main.typ");
        let mut app = App::new();
        app.workspace_root = directory.path().to_path_buf();
        app.cursor_position = Point::new(120.0, 240.0);

        app.show_project_context_menu(path.clone(), project::EntryKind::TypstFile);

        assert_eq!(app.selected_project_entry, Some(path.clone()));
        assert_eq!(app.selected_project_file, Some(path.clone()));
        assert!(app.document.find_path(&path).is_none());
        let context = app
            .project_context_menu
            .expect("the context menu should be open");
        assert_eq!(context.path, path);
        assert_eq!(context.position, Point::new(120.0, 240.0));
    }

    #[test]
    fn project_context_menu_protects_the_root_and_exposes_directory_operations() {
        let app = App::new();
        let root = app.workspace_root.clone();
        let root_context = ProjectContextMenu {
            path: root.clone(),
            kind: project::EntryKind::Directory,
            position: Point::ORIGIN,
        };
        let root_entries = app.project_context_entries(&root_context);
        let root_labels = menu_labels(&root_entries);

        assert!(root_labels.contains(&"Novo arquivo Typst…"));
        assert!(root_labels.contains(&"Nova pasta…"));
        assert!(root_labels.contains(&"Atualizar árvore"));
        assert!(!root_labels.contains(&"Renomear…"));
        assert!(!root_labels.contains(&"Excluir…"));

        let folder_context = ProjectContextMenu {
            path: root.join("capitulos"),
            kind: project::EntryKind::Directory,
            position: Point::ORIGIN,
        };
        let folder_entries = app.project_context_entries(&folder_context);
        let folder_labels = menu_labels(&folder_entries);

        assert!(folder_labels.contains(&"Renomear…"));
        assert!(folder_labels.contains(&"Excluir…"));
    }

    #[test]
    fn creation_toolbar_targets_the_selected_directory_or_file_parent() {
        let directory = tempfile::tempdir().expect("a temporary project can be created");
        let chapters = directory.path().join("chapters");
        let chapter = chapters.join("one.typ");
        let mut app = App::new();
        app.workspace_root = directory.path().to_path_buf();
        app.project_tree = vec![project::ProjectEntry {
            path: chapters.clone(),
            kind: project::EntryKind::Directory,
            children: vec![project::ProjectEntry {
                path: chapter.clone(),
                kind: project::EntryKind::TypstFile,
                children: Vec::new(),
            }],
        }];

        app.selected_project_entry = Some(chapters.clone());
        assert_eq!(app.selected_project_target_directory(), chapters);

        app.selected_project_entry = Some(chapter);
        assert_eq!(app.selected_project_target_directory(), chapters);

        app.selected_project_entry = None;
        assert_eq!(app.selected_project_target_directory(), directory.path());
    }

    #[test]
    fn visibility_icon_follows_the_effective_compilation_document() {
        let directory = tempfile::tempdir().expect("a temporary project can be created");
        let active = directory.path().join("active.typ");
        let fixed = directory.path().join("fixed.typ");
        let mut app = App::new();
        app.workspace_root = directory.path().to_path_buf();
        app.document = Documents::new(Document::opened(active.clone(), String::new()));
        app.project_tree = vec![
            project::ProjectEntry {
                path: active.clone(),
                kind: project::EntryKind::TypstFile,
                children: Vec::new(),
            },
            project::ProjectEntry {
                path: fixed.clone(),
                kind: project::EntryKind::TypstFile,
                children: Vec::new(),
            },
        ];

        let items = app.project_tree_items(&app.project_tree);
        assert_eq!(items[0].status_icon, Some(ui::WorkflowIcon::Visibility));
        assert_eq!(items[1].status_icon, None);

        app.project_main = Some(fixed);
        let items = app.project_tree_items(&app.project_tree);
        assert_eq!(items[0].status_icon, None);
        assert_eq!(items[1].status_icon, Some(ui::WorkflowIcon::Visibility));
    }

    #[test]
    fn selecting_a_non_typst_file_disables_typst_file_operations() {
        let mut app = App::new();
        app.selected_project_file = Some(PathBuf::from("main.typ"));
        let readme = app.workspace_root.join("README.md");

        let _ = app.update(Message::ProjectEntryPressed(
            readme.clone(),
            project::EntryKind::File,
        ));

        assert_eq!(app.selected_project_entry, Some(readme));
        assert!(app.selected_project_file.is_none());
        assert!(
            app.file_status
                .as_deref()
                .is_some_and(|status| status.contains("não é um documento Typst"))
        );
    }

    #[test]
    fn restored_active_document_is_revealed_in_the_project_tree() {
        let directory = tempfile::tempdir().expect("a temporary project can be created");
        let chapters = directory.path().join("chapters");
        let path = chapters.join("main.typ");
        let app = App::build(
            Documents::new(Document::opened(path.clone(), "Main".to_owned())),
            panes_from_layout(session::PaneLayout::default()),
            directory.path().to_path_buf(),
            None,
            None,
            settings::Settings::default(),
            "Sessão restaurada".to_owned(),
        );

        assert_eq!(app.selected_project_entry, Some(path.clone()));
        assert_eq!(app.selected_project_file, Some(path));
        assert!(app.expanded_project_directories.contains(&chapters));
    }

    #[test]
    fn project_scan_detects_a_root_main_file_automatically() {
        let directory = tempfile::tempdir().expect("a temporary project can be created");
        let main = directory.path().join("main.typ");
        let chapter = directory.path().join("chapter.typ");
        let mut app = App::new();
        app.workspace_root = directory.path().to_path_buf();
        app.project_scan_busy = true;
        app.detect_project_main_on_scan = true;

        let _ = app.update(Message::ProjectScanned(project::ScanOutcome {
            root: directory.path().to_path_buf(),
            snapshot: Ok(project::ProjectSnapshot {
                entries: Vec::new(),
                typst_files: vec![chapter, main.clone()],
            }),
        }));

        assert_eq!(app.project_main, Some(main));
        assert!(app.pending_compile.is_some());
        assert!(
            app.file_status
                .as_deref()
                .is_some_and(|status| status.contains("detectado"))
        );
    }

    #[test]
    fn explicit_active_tab_mode_is_not_undone_by_later_scans() {
        let directory = tempfile::tempdir().expect("a temporary project can be created");
        let main = directory.path().join("main.typ");
        let mut app = App::new();
        app.workspace_root = directory.path().to_path_buf();
        app.project_main = Some(main.clone());

        app.clear_project_main();
        app.project_scan_busy = true;
        let _ = app.update(Message::ProjectScanned(project::ScanOutcome {
            root: directory.path().to_path_buf(),
            snapshot: Ok(project::ProjectSnapshot {
                entries: Vec::new(),
                typst_files: vec![main],
            }),
        }));

        assert_eq!(app.project_main, None);
        assert!(!app.detect_project_main_on_scan);
    }

    #[test]
    fn renaming_and_deleting_the_fixed_main_updates_project_state() {
        let directory = tempfile::tempdir().expect("a temporary project can be created");
        let from = directory.path().join("main.typ");
        let to = directory.path().join("book.typ");
        let mut app = App::new();
        app.workspace_root = directory.path().to_path_buf();
        app.document = Documents::new(Document::opened(from.clone(), "Main".to_owned()));
        app.project_main = Some(from.clone());
        app.selected_project_file = Some(from.clone());

        let _ = app.handle_project_operation(project::OperationOutcome::Renamed {
            from,
            to: to.clone(),
            kind: project::EntryKind::TypstFile,
        });

        assert_eq!(app.project_main.as_deref(), Some(to.as_path()));
        assert_eq!(app.document.path(), Some(to.as_path()));

        let _ = app.handle_project_operation(project::OperationOutcome::Deleted {
            path: to.clone(),
            kind: project::EntryKind::TypstFile,
        });

        assert_eq!(app.project_main, None);
        assert!(app.document.find_path(&to).is_none());
        assert!(
            app.file_status
                .as_deref()
                .is_some_and(|status| status.contains("acompanha a aba ativa"))
        );
    }

    #[test]
    fn renaming_a_directory_remaps_open_documents_main_and_expansion() {
        let directory = tempfile::tempdir().expect("a temporary project can be created");
        let from = directory.path().join("chapters");
        let to = directory.path().join("sections");
        let first = from.join("one.typ");
        let second = from.join("nested/two.typ");
        let mut app = App::new();
        app.workspace_root = directory.path().to_path_buf();
        app.document = Documents::new(Document::opened(first, String::new()));
        app.document.add(Document::opened(second, String::new()));
        app.project_main = Some(from.join("one.typ"));
        app.expanded_project_directories = HashSet::from([
            directory.path().to_path_buf(),
            from.clone(),
            from.join("nested"),
        ]);

        let _ = app.handle_project_operation(project::OperationOutcome::Renamed {
            from,
            to: to.clone(),
            kind: project::EntryKind::Directory,
        });

        let paths = app
            .document
            .iter()
            .filter_map(|(_, document)| document.path().map(Path::to_path_buf))
            .collect::<Vec<_>>();
        assert!(paths.contains(&to.join("one.typ")));
        assert!(paths.contains(&to.join("nested/two.typ")));
        assert_eq!(app.project_main, Some(to.join("one.typ")));
        assert!(app.expanded_project_directories.contains(&to));
        assert!(
            app.expanded_project_directories
                .contains(&to.join("nested"))
        );
    }

    #[test]
    fn deleting_a_directory_with_dirty_documents_is_blocked_before_confirmation() {
        let directory = tempfile::tempdir().expect("a temporary project can be created");
        let folder = directory.path().join("chapters");
        let path = folder.join("one.typ");
        let mut app = App::new();
        app.workspace_root = directory.path().to_path_buf();
        app.document = Documents::new(Document::opened(path, "texto".to_owned()));
        app.document.perform(Action::MoveTo(5));
        app.document.perform(Action::Insert(" alterado".to_owned()));

        let _ = app.start_delete_project_entry(folder, project::EntryKind::Directory);

        assert!(!app.file_busy);
        assert!(
            app.file_status
                .as_deref()
                .is_some_and(|status| status.contains("documentos alterados"))
        );
    }

    #[test]
    fn deleting_a_project_entry_waits_for_the_spectrum_confirmation() {
        let directory = tempfile::tempdir().expect("a temporary project can be created");
        let folder = directory.path().join("chapters");
        std::fs::create_dir(&folder).expect("the project folder can be created");
        let mut app = App::new();
        app.workspace_root = directory.path().to_path_buf();

        let _ = app.start_delete_project_entry(folder.clone(), project::EntryKind::Directory);

        assert!(app.file_busy);
        assert!(folder.exists());
        assert!(matches!(
            app.pending_alert_dialog,
            Some(PendingAlertDialog::DeleteProjectEntry {
                ref path,
                kind: project::EntryKind::Directory,
            }) if path == &folder
        ));

        let _ = app.update(Message::DismissAlertDialog);
        assert!(!app.file_busy);
        assert!(app.pending_alert_dialog.is_none());
        assert!(folder.exists());
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
        let mut app = App::fresh(
            Some(directory.path().join("session.json")),
            display::FALLBACK_LOGICAL_PPI,
        );
        *app.document.active_mut() =
            Document::opened(PathBuf::from("/project/main.typ"), "saved".to_owned());
        app.document.perform(Action::MoveTo("saved".len()));
        app.document.perform(Action::Insert(" local".to_owned()));
        app.new_document();
        app.document.perform(Action::Insert("draft".to_owned()));
        app.workspace_root = PathBuf::from("/project");
        app.project_main = Some(PathBuf::from("/project/main.typ"));
        let pane_layout = session::PaneLayout::from_tree(session::PaneNode::split(
            session::Axis::Horizontal,
            0.7,
            session::PaneNode::pane(session::Pane::Preview),
            session::PaneNode::split(
                session::Axis::Vertical,
                0.35,
                session::PaneNode::pane(session::Pane::Project),
                session::PaneNode::pane(session::Pane::Editor),
            ),
        ));
        app.panes = panes_from_layout(pane_layout.clone());
        app.settings.wrap_lines = true;
        app.settings.preview_zoom = 135;

        let stored = app.session_snapshot(false);
        let restored = App::restore(stored, None, display::FALLBACK_LOGICAL_PPI);
        let documents = restored
            .document
            .iter()
            .map(|(_, document)| document.snapshot().1)
            .collect::<Vec<_>>();

        assert_eq!(documents, vec!["saved local", "draft"]);
        assert_eq!(restored.document.snapshot().1, "draft");
        assert_eq!(restored.workspace_root, PathBuf::from("/project"));
        assert_eq!(
            restored.project_main,
            Some(PathBuf::from("/project/main.typ"))
        );
        assert!(restored.settings.wrap_lines);
        assert_eq!(restored.settings.preview_zoom, 135);
        assert_eq!(restored.pane_layout(), pane_layout);
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
        let mut app = App::fresh(
            Some(directory.path().join("session.json")),
            display::FALLBACK_LOGICAL_PPI,
        );

        let _ = app.update(Message::Editor(Action::Insert("edit".to_owned())));

        assert_eq!(app.session.revision, 1);
        assert!(app.session.deadline.is_some());
        assert!(!app.session.write_busy);
    }

    #[test]
    fn stale_session_write_is_repeated_before_window_close() {
        let directory = tempfile::tempdir().expect("a temporary directory can be created");
        let mut app = App::fresh(
            Some(directory.path().join("session.json")),
            display::FALLBACK_LOGICAL_PPI,
        );
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
        let mut app = App::fresh(
            Some(directory.path().join("session.json")),
            display::FALLBACK_LOGICAL_PPI,
        );
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
    fn problem_items_preserve_severity_full_message_source_and_navigation() {
        let directory = tempfile::tempdir().expect("a temporary project can be created");
        let chapter = directory.path().join("chapters").join("one.typ");
        let message =
            "uma mensagem de aviso longa que deve permanecer completa no painel de problemas";
        let mut app = App::new();
        app.workspace_root = directory.path().to_path_buf();
        app.diagnostics = vec![compiler::ReportedDiagnostic {
            target: compiler::DiagnosticTarget::ProjectFile(chapter.clone()),
            range: 12..34,
            severity: compiler::DiagnosticSeverity::Warning,
            message: message.to_owned(),
        }];

        let items = app.problem_items();

        assert_eq!(items.len(), 1);
        assert_eq!(items[0].severity, ui::ProblemSeverity::Warning);
        assert_eq!(items[0].message, message);
        assert_eq!(items[0].source, "chapters/one.typ");
        match &items[0].on_press {
            Some(Message::OpenDiagnostic(target, range)) => {
                assert_eq!(target, &compiler::DiagnosticTarget::ProjectFile(chapter));
                assert_eq!(range, &(12..34));
            }
            _ => panic!("o problema deve navegar para o diagnóstico"),
        }
    }

    #[test]
    fn diagnostic_counts_separate_errors_from_warnings() {
        let mut app = App::new();
        app.diagnostics = vec![
            compiler::ReportedDiagnostic {
                target: compiler::DiagnosticTarget::Main,
                range: 0..1,
                severity: compiler::DiagnosticSeverity::Error,
                message: "erro".to_owned(),
            },
            compiler::ReportedDiagnostic {
                target: compiler::DiagnosticTarget::Main,
                range: 2..3,
                severity: compiler::DiagnosticSeverity::Warning,
                message: "aviso".to_owned(),
            },
        ];

        assert_eq!(app.diagnostic_counts(), (1, 1));
    }

    #[test]
    fn problems_navigation_label_reports_each_active_severity() {
        assert_eq!(problems_navigation_label(0, 0), "Problemas");
        assert_eq!(problems_navigation_label(1, 0), "Problemas: 1 erro");
        assert_eq!(problems_navigation_label(0, 1), "Problemas: 1 aviso");
        assert_eq!(
            problems_navigation_label(2, 3),
            "Problemas: 2 erros, 3 avisos"
        );
    }

    #[test]
    fn main_diagnostics_stay_on_the_fixed_main_while_an_import_is_active() {
        let directory = tempfile::tempdir().expect("a temporary project can be created");
        let main_path = directory.path().join("main.typ");
        let part_path = directory.path().join("part.typ");
        let mut app = App::new();
        app.workspace_root = directory.path().to_path_buf();
        app.document = Documents::new(Document::opened(main_path.clone(), "broken".to_owned()));
        let main = app.document.active_id();
        let part = app
            .document
            .add(Document::opened(part_path, "import".to_owned()));
        app.project_main = Some(main_path);

        app.install_diagnostics(vec![compiler::ReportedDiagnostic {
            target: compiler::DiagnosticTarget::Main,
            range: 0..6,
            severity: compiler::DiagnosticSeverity::Error,
            message: "erro principal".to_owned(),
        }]);

        assert_eq!(
            app.document
                .get(main)
                .expect("the main document is open")
                .content()
                .diagnostics()
                .len(),
            1
        );
        assert!(
            app.document
                .get(part)
                .expect("the import is active")
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

    #[test]
    fn project_tree_keyboard_navigation_respects_expansion() {
        let directory = tempfile::tempdir().expect("a temporary project can be created");
        let folder = directory.path().join("chapters");
        let file = folder.join("one.typ");
        let mut app = App::new();
        app.workspace_root = directory.path().to_path_buf();
        app.expanded_project_directories = HashSet::from([directory.path().to_path_buf()]);
        app.project_tree = vec![project::ProjectEntry {
            path: folder.clone(),
            kind: project::EntryKind::Directory,
            children: vec![project::ProjectEntry {
                path: file.clone(),
                kind: project::EntryKind::TypstFile,
                children: Vec::new(),
            }],
        }];
        app.selected_project_entry = Some(folder.clone());

        let _ = app.navigate_project_tree(TreeNavigation::ChildOrExpand);
        assert!(app.expanded_project_directories.contains(&folder));
        let _ = app.navigate_project_tree(TreeNavigation::ChildOrExpand);
        assert_eq!(app.selected_project_entry, Some(file));
        let _ = app.navigate_project_tree(TreeNavigation::ParentOrCollapse);
        assert_eq!(app.selected_project_entry, Some(folder));
    }

    #[test]
    fn closed_tabs_can_be_reopened_and_discarded_text_is_not_restored() {
        let path = PathBuf::from("/project/main.typ");
        let mut app = App::new();
        app.document = Documents::new(Document::opened(path.clone(), "salvo".to_owned()));
        let id = app.document.active_id();
        app.document.perform(Action::MoveTo(5));
        app.document.perform(Action::Insert(" local".to_owned()));
        app.discarded_tabs.insert(id);

        app.close_document(id);
        app.reopen_closed_document();

        assert_eq!(app.document.path(), Some(path.as_path()));
        assert_eq!(app.document.snapshot().1, "salvo");
        assert!(!app.document.is_dirty());
    }

    #[test]
    fn autosave_is_scheduled_only_for_named_dirty_documents() {
        let mut app = App::new();
        app.settings.auto_save = true;
        app.document = Documents::new(Document::opened(
            PathBuf::from("/project/main.typ"),
            "texto".to_owned(),
        ));

        let _ = app.update(Message::Editor(Action::Insert(" novo".to_owned())));
        assert!(app.auto_save_deadline.is_some());

        app.document = Documents::new(Document::new());
        app.auto_save_deadline = None;
        let _ = app.update(Message::Editor(Action::Insert("rascunho".to_owned())));
        assert!(app.auto_save_deadline.is_none());
    }

    #[test]
    fn recent_projects_are_persisted_in_most_recent_first_order() {
        let first = tempfile::tempdir().expect("a first project can be created");
        let second = tempfile::tempdir().expect("a second project can be created");
        let mut app = App::new();

        let _ = app.handle_project_folder_selected(Some(first.path().to_path_buf()));
        let _ = app.handle_project_folder_selected(Some(second.path().to_path_buf()));
        let _ = app.handle_project_folder_selected(Some(first.path().to_path_buf()));
        let stored = app.session_snapshot(false);

        assert_eq!(stored.recent_projects[0], first.path());
        assert_eq!(stored.recent_projects[1], second.path());
        assert_eq!(stored.recent_projects.len(), 2);
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

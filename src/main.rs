use std::path::PathBuf;

use iced::{
    Element,
    Length::Fill,
    Task, Theme,
    widget::{button, column, pane_grid, row, svg, text},
};
use typst::{
    Library, LibraryExt, World,
    diag::FileResult,
    foundations::{Bytes, Datetime, Duration},
    syntax::{FileId, RootedPath, Source, VirtualPath, VirtualRoot},
    text::{Font, FontBook},
    utils::LazyHash,
};
use typst_iced_editor::{Action, Content, code_editor};
use typst_kit::{
    datetime::Time,
    diagnostics::DiagnosticWorld,
    downloader::SystemDownloader,
    files::{FileStore, FsRoot, SystemFiles},
    fonts::{self, FontStore},
    packages::SystemPackages,
};
use typst_layout::PagedDocument;

fn main() -> iced::Result {
    let title: &str = concat!("Typstation v", env!("CARGO_PKG_VERSION"));
    iced::application(App::boot, App::update, App::view)
        .title(title)
        .theme(Theme::Dark)
        .window_size([1200.0, 800.0])
        .centered()
        .run()
}

struct App {
    content: Content,
    panes: pane_grid::State<Pane>,
    world: TypstationWorld,
    preview: Option<svg::Handle>,
}

#[derive(Debug, Clone)]
enum Message {
    Editor(Action),
    PaneDragged(pane_grid::DragEvent),
    PaneResized(pane_grid::ResizeEvent),
    Compile,
    Bold,
    Italic,
    Underline,
    PrefixLines(String),
}

enum Pane {
    Editor,
    Preview,
}

impl App {
    fn boot() -> App {
        let root = std::env::current_dir().unwrap_or_else(|err| {
            eprintln!("erro ao obter diretório atual: {err}");
            PathBuf::from(".")
        });

        let (mut panes, editor_pane) = pane_grid::State::new(Pane::Editor);

        panes.split(pane_grid::Axis::Vertical, editor_pane, Pane::Preview);

        App {
            content: Content::with_text(DEMO),
            panes,
            world: TypstationWorld::new(root),
            preview: None,
        }
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Editor(action) => {
                let should_compile = action.is_edit();

                self.content.perform(action);

                if should_compile {
                    Task::done(Message::Compile)
                } else {
                    Task::none()
                }
            }
            Message::Bold => {
                if self.content.selection().is_empty() {
                    Task::none()
                } else {
                    self.content.perform(Action::Insert("*".to_owned()));
                    Task::done(Message::Compile)
                }
            }
            Message::Italic => {
                if self.content.selection().is_empty() {
                    Task::none()
                } else {
                    self.content.perform(Action::Insert("_".to_owned()));
                    Task::done(Message::Compile)
                }
            }
            Message::Underline => {
                let range = self.content.selection();

                if range.is_empty() {
                    Task::none()
                } else {
                    let selected = {
                        let buffer = self.content.buffer();
                        buffer.text()[range.clone()].to_owned()
                    };

                    let open = "#underline[";
                    let replacement = format!("{open}{selected}]");

                    // Posições da seleção depois da inserção.
                    let selection_start = range.start + open.len();
                    let selection_end = selection_start + selected.len();

                    self.content.perform(Action::Replace {
                        range,
                        text: replacement,
                    });

                    // Mantém somente o texto interno selecionado.
                    self.content.perform(Action::MoveTo(selection_start));
                    self.content.perform(Action::SelectTo(selection_end));

                    Task::done(Message::Compile)
                }
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
            Message::Compile => {
                let buffer = self.content.buffer();

                self.world.set_source(buffer.text());

                let result = typst::compile::<PagedDocument>(&self.world);

                let document = match result.output {
                    Ok(document) => document,
                    Err(errors) => {
                        println!("erro ao compilar: {} erro(s)", errors.len());
                        return Task::none();
                    }
                };

                let svg = typst_svg::svg(&document.pages()[0], &typst_svg::SvgOptions::default());

                self.preview = Some(svg::Handle::from_memory(svg.into_bytes()));

                Task::none()
            }
            Message::PrefixLines(prefix) => {
                let selection = self.content.selection();

                let edits = {
                    let buffer = self.content.buffer();

                    let first_line = buffer.byte_to_line(selection.start);

                    let last_line = if selection.is_empty() {
                        first_line
                    } else {
                        buffer.byte_to_line(selection.end.saturating_sub(1))
                    };

                    (first_line..=last_line)
                        .map(|line| {
                            let line_start = buffer.line_range(line).start;

                            // Intervalo vazio significa inserir, sem remover texto.
                            (line_start..line_start, prefix.clone())
                        })
                        .collect()
                };

                self.content.perform(Action::ApplyEdits(edits));
                Task::done(Message::Compile)
            }
        }
    }

    fn view(&self) -> Element<'_, Message> {
        let pane_grid = pane_grid(&self.panes, |_id, pane, _is_maximized| {
            let editor = code_editor(&self.content).on_action(Message::Editor);

            let content: Element<'_, Message> = match pane {
                Pane::Editor => editor.into(),

                Pane::Preview => match &self.preview {
                    Some(handle) => svg(handle.clone()).width(Fill).height(Fill).into(),

                    None => text("Sem preview").into(),
                },
            };

            pane_grid::Content::new(content)
        })
        .spacing(8)
        .min_size(200)
        .on_drag(Message::PaneDragged)
        .on_resize(10, Message::PaneResized);

        let header = row![
            button("▶").on_press(Message::Compile),
            button("B").on_press(Message::Bold),
            button("I").on_press(Message::Italic),
            button("U").on_press(Message::Underline),
            button("Lista").on_press(Message::PrefixLines("- ".into())),
            button("Numeração").on_press(Message::PrefixLines("+ ".into())),
        ];

        column![header, pane_grid].width(Fill).height(Fill).into()
    }
}

const USER_AGENT: &str = concat!("typstation/", env!("CARGO_PKG_VERSION"));

struct TypstationWorld {
    library: LazyHash<Library>,
    fonts: FontStore,
    files: FileStore<SystemFiles>,
    time: Time,
    main: FileId,
    source: Source,
    bytes: Bytes,
}

impl TypstationWorld {
    fn new(root: PathBuf) -> Self {
        let mut fonts = FontStore::new();
        fonts.extend(fonts::embedded());
        fonts.extend(fonts::system());

        let packages = SystemPackages::new(SystemDownloader::new(USER_AGENT));
        let files = FileStore::new(SystemFiles::new(FsRoot::new(root), packages));

        let vpath = VirtualPath::new("main.typ").expect("`main.typ` é um caminho válido");
        let main = RootedPath::new(VirtualRoot::Project, vpath).intern();

        Self {
            library: LazyHash::new(Library::default()),
            fonts,
            files,
            time: Time::system(),
            main,
            source: Source::new(main, String::new()),
            bytes: Bytes::from_string(String::new()),
        }
    }

    fn set_source(&mut self, text: &str) {
        self.source.replace(text);
        self.bytes = Bytes::from_string(text.to_owned());
        self.files.reset();
        self.time.reset();
    }
}

impl World for TypstationWorld {
    fn library(&self) -> &LazyHash<Library> {
        &self.library
    }

    fn book(&self) -> &LazyHash<FontBook> {
        self.fonts.book()
    }

    fn main(&self) -> FileId {
        self.main
    }

    fn source(&self, id: FileId) -> FileResult<Source> {
        if id == self.main {
            Ok(self.source.clone())
        } else {
            self.files.source(id)
        }
    }

    fn file(&self, id: FileId) -> FileResult<Bytes> {
        if id == self.main {
            Ok(self.bytes.clone())
        } else {
            self.files.file(id)
        }
    }

    fn font(&self, index: usize) -> Option<Font> {
        self.fonts.font(index)
    }

    fn today(&self, offset: Option<Duration>) -> Option<Datetime> {
        self.time.today(offset)
    }
}

impl DiagnosticWorld for TypstationWorld {
    fn name(&self, id: FileId) -> String {
        let path = id.vpath().get_without_slash();
        match id.root() {
            VirtualRoot::Project => path.to_string(),
            VirtualRoot::Package(spec) => format!("{spec}/{path}"),
        }
    }
}

const DEMO: &str = r#"#set page(paper: "a5")
#set heading(numbering: "1.")

#show link: set text(fill: blue, weight: 700)
#show link: underline

= The Typst Playground

Welcome to the Typst Playground! This is a sandbox where you can experiment with Typst. You can type anywhere in the editor panel on the left. The preview panel to the right will update live.

= Basics <basics>
== Loaerstonrest
Typst is a _markup_ language. You use it to express not just the content, but also the structure and formatting of your document. For example, surrounding a word with underscores _emphasizes_ it with italics and starting a line with an equals sign creates a section heading.

Typst has lightweight syntax like this for the most common formatting needs. Among other things, you can use it to:

- *Strongly emphasize* some text
- Refer to @basics
- Typeset math: $a, b in { 1/2, sqrt(4 a b) }$

That's just the surface though! Typst has powerful systems for scripting, styling, introspection, and more. In the realm of a Typst document, there is nothing you can't automate.

= Next steps

To learn more about Typst, we recommend you to check out our tutorial at https://typst.app/docs/tutorial.

Once you've explored Typst a bit, why not set yourself up a proper editing environment?

#import "@preview/tiaoma:0.3.0"
#let next-step(url, body) = grid(
  columns: 2,
  gutter: 1em,
  tiaoma.qrcode(url, width: 3em),
  {
    show strong: link.with(url)
    body
  }
)

#next-step("https://typst.app/signup")[
  To get access to multi-file projects, live collaboration, and more, *sign up* to our web app for free.
]

#next-step("https://typst.app/open-source/#download")[
  You can also *download* our free and open-source command line tool to continue your journey locally.
]
"#;

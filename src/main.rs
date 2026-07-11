use std::path::{Path, PathBuf};
use std::process::ExitCode;

use typst::{
    Library, LibraryExt, World,
    diag::FileResult,
    foundations::{Bytes, Datetime, Duration},
    syntax::{FileId, RootedPath, Source, VirtualPath, VirtualRoot},
    text::{Font, FontBook},
    utils::LazyHash,
};
use typst_kit::{
    datetime::Time,
    diagnostics::{
        self, DiagnosticFormat, DiagnosticWorld,
        termcolor::{ColorChoice, StandardStream},
    },
    downloader::SystemDownloader,
    files::{FileStore, FsRoot, SystemFiles},
    fonts::{self, FontStore},
    packages::SystemPackages,
};
use typst_layout::PagedDocument;
use typst_pdf::PdfOptions;

/// Identifica o programa ao baixar pacotes do Typst Universe.
const USER_AGENT: &str = concat!("typstation/", env!("CARGO_PKG_VERSION"));

/// O ambiente que o compilador do Typst consulta durante a compilação.
///
/// Está dividido em duas partes: o ambiente propriamente dito (fontes, pacotes,
/// raiz do projeto), que é caro de montar e imutável, e o texto principal, que
/// muda a cada edição. Ver [`TypstationWorld::new`] e
/// [`TypstationWorld::set_source`].
struct TypstationWorld {
    library: LazyHash<Library>,
    fonts: FontStore,
    files: FileStore<SystemFiles>,
    time: Time,
    main: FileId,
    source: Source,
    /// Os bytes de `source`, prontos para servir `World::file` sem recopiar o
    /// documento inteiro a cada chamada.
    bytes: Bytes,
}

impl TypstationWorld {
    /// Monta o ambiente de compilação.
    ///
    /// Isto é caro — o scan de fontes do sistema sozinho custa ~165 ms, contra
    /// ~20 ms de uma compilação. Construa um `TypstationWorld` uma única vez e
    /// reaproveite-o via [`set_source`](Self::set_source) a cada recompilação.
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

    /// Troca o texto principal e prepara o mundo para uma nova compilação.
    ///
    /// `Source::replace` faz diff do texto e remenda a árvore sintática no
    /// lugar, em vez de reparsear do zero.
    fn set_source(&mut self, text: &str) {
        self.source.replace(text);
        self.bytes = Bytes::from_string(text.to_owned());

        // Descarta os arquivos em cache para que edições em disco (imagens,
        // arquivos incluídos) sejam relidas na próxima compilação, e refaz a
        // leitura da data do sistema.
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

const DEMO: &str = r#"
#set page(paper: "a5")
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

/// Diretório onde a aplicação escreve tudo o que gera. É ignorado pelo git.
const OUT_DIR: &str = "out";

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(err) => {
            eprintln!("erro: {err}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<ExitCode, Box<dyn std::error::Error>> {
    let root = std::env::current_dir()?;

    let mut world = TypstationWorld::new(root);
    world.set_source(DEMO);

    let result = typst::compile::<PagedDocument>(&world);

    let mut stderr = StandardStream::stderr(ColorChoice::Auto);
    diagnostics::emit(
        &mut stderr,
        &world,
        result.warnings.iter(),
        DiagnosticFormat::Human,
    )?;

    let document = match result.output {
        Ok(document) => document,
        Err(errors) => {
            diagnostics::emit(&mut stderr, &world, errors.iter(), DiagnosticFormat::Human)?;
            return Ok(ExitCode::FAILURE);
        }
    };

    let pdf = typst_pdf::pdf(&document, &PdfOptions::default())
        .map_err(|errors| format!("falha ao exportar PDF: {}", errors.len()))?;

    let output = Path::new(OUT_DIR).join("tutorial.pdf");
    std::fs::create_dir_all(OUT_DIR)?;
    std::fs::write(&output, pdf)?;

    println!("PDF gerado em {}", output.display());
    Ok(ExitCode::SUCCESS)
}

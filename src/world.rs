use std::{ops::Range, path::PathBuf};

use typst::{
    Library, LibraryExt, World,
    diag::FileResult,
    foundations::{Bytes, Datetime, Duration},
    syntax::{DiagSpan, DiagSpanKind, FileId, RootedPath, Source, VirtualPath, VirtualRoot},
    text::{Font, FontBook},
    utils::LazyHash,
};
use typst_kit::{
    datetime::Time,
    downloader::SystemDownloader,
    files::{FileStore, FsRoot, SystemFiles},
    fonts::{self, FontStore},
    packages::SystemPackages,
};

const USER_AGENT: &str = concat!("typstation/", env!("CARGO_PKG_VERSION"));

/// Resources consulted by Typst while compiling a project.
///
/// The expensive font and package stores are created once and reused across
/// source revisions by the compilation worker.
pub struct TypstationWorld {
    library: LazyHash<Library>,
    fonts: FontStore,
    files: FileStore<SystemFiles>,
    time: Time,
    main: FileId,
    source: Source,
    bytes: Bytes,
}

impl TypstationWorld {
    pub fn new(root: PathBuf) -> Self {
        Self::with_main(root, "main.typ")
    }

    pub fn with_main(root: PathBuf, main_name: &str) -> Self {
        let mut fonts = FontStore::new();
        fonts.extend(fonts::embedded());
        fonts.extend(fonts::system());

        let packages = SystemPackages::new(SystemDownloader::new(USER_AGENT));
        let files = FileStore::new(SystemFiles::new(FsRoot::new(root), packages));

        let vpath = VirtualPath::new(main_name)
            .expect("the document file name must be a valid Typst virtual path");
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

    /// Replaces only the in-memory main source, preserving external-file caches.
    pub fn set_source(&mut self, text: &str) {
        self.source.replace(text);
        self.bytes = Bytes::from_string(text.to_owned());
        self.time.reset();
    }

    /// Invalidates imported files and assets before an explicit recompilation.
    pub fn reset_files(&mut self) {
        self.files.reset();
    }

    /// Resolves a Typst diagnostic span when it belongs to the main document.
    pub fn main_range(&self, span: DiagSpan) -> Option<Range<usize>> {
        match span.get() {
            DiagSpanKind::Detached => None,
            DiagSpanKind::Number { id, num, sub_range } if id == self.main => {
                self.source.range(num, sub_range)
            }
            DiagSpanKind::Range { id, range } if id == self.main => Some(range),
            DiagSpanKind::Number { .. } | DiagSpanKind::Range { .. } => None,
        }
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

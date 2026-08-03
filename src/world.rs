use std::{collections::HashMap, ops::Range, path::PathBuf};

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
    root: PathBuf,
    main: FileId,
    source: Source,
    bytes: Bytes,
    overlays: HashMap<FileId, MemoryFile>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceOverlay {
    pub path: PathBuf,
    pub text: String,
}

struct MemoryFile {
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
        let files = FileStore::new(SystemFiles::new(FsRoot::new(root.clone()), packages));

        let vpath = VirtualPath::new(main_name)
            .expect("the document file name must be a valid Typst virtual path");
        let main = RootedPath::new(VirtualRoot::Project, vpath).intern();

        Self {
            library: LazyHash::new(Library::default()),
            fonts,
            files,
            time: Time::system(),
            root,
            main,
            source: Source::new(main, String::new()),
            bytes: Bytes::from_string(String::new()),
            overlays: HashMap::new(),
        }
    }

    /// Replaces only the in-memory main source, preserving external-file caches.
    pub fn set_source(&mut self, text: &str) {
        self.source.replace(text);
        self.bytes = Bytes::from_string(text.to_owned());
        self.time.reset();
    }

    /// Replaces the in-memory versions of imported project files.
    ///
    /// Files outside the project root cannot be represented by a Typst
    /// project path and are ignored. Removing an overlay invalidates the disk
    /// cache so the next compilation reads the current saved version.
    pub fn set_overlays(&mut self, overlays: impl IntoIterator<Item = SourceOverlay>) {
        let next = overlays
            .into_iter()
            .filter_map(|overlay| {
                let vpath = VirtualPath::virtualize(&self.root, &overlay.path).ok()?;
                let id = RootedPath::new(VirtualRoot::Project, vpath).intern();

                (id != self.main).then(|| {
                    let text = overlay.text;
                    (
                        id,
                        MemoryFile {
                            source: Source::new(id, text.clone()),
                            bytes: Bytes::from_string(text),
                        },
                    )
                })
            })
            .collect::<HashMap<_, _>>();
        let changed = self.overlays.len() != next.len()
            || self.overlays.iter().any(|(id, current)| {
                next.get(id)
                    .is_none_or(|replacement| replacement.source.text() != current.source.text())
            });

        if changed {
            self.overlays = next;
            self.files.reset();
            self.time.reset();
        }
    }

    /// Invalidates imported files and assets before an explicit recompilation.
    pub fn reset_files(&mut self) {
        self.files.reset();
    }

    /// Resolves a Typst diagnostic span to its source file and byte range.
    pub fn span_range(&self, span: DiagSpan) -> Option<(FileId, Range<usize>)> {
        match span.get() {
            DiagSpanKind::Detached => None,
            DiagSpanKind::Number { id, num, sub_range } => self
                .source(id)
                .ok()?
                .range(num, sub_range)
                .map(|range| (id, range)),
            DiagSpanKind::Range { id, range } => Some((id, range)),
        }
    }

    pub fn is_main(&self, id: FileId) -> bool {
        id == self.main
    }

    /// Maps a project file ID back to the absolute path shown by the editor.
    pub fn project_path(&self, id: FileId) -> Option<PathBuf> {
        matches!(id.root(), VirtualRoot::Project)
            .then(|| id.vpath().realize(self.root.as_path()).ok())
            .flatten()
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
        } else if let Some(file) = self.overlays.get(&id) {
            Ok(file.source.clone())
        } else {
            self.files.source(id)
        }
    }

    fn file(&self, id: FileId) -> FileResult<Bytes> {
        if id == self.main {
            Ok(self.bytes.clone())
        } else if let Some(file) = self.overlays.get(&id) {
            Ok(file.bytes.clone())
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

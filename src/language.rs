use std::{collections::HashMap, ops::Range, path::PathBuf};

use iced::{
    Subscription,
    futures::{
        SinkExt, Stream, StreamExt,
        channel::mpsc::{self, UnboundedSender},
    },
    stream,
};
use typst::{
    World,
    syntax::{FileId, LinkedNode, Side, Source, Span, SyntaxKind},
};
use typst_iced_editor::{Completion as EditorCompletion, Hover as EditorHover};
use typst_ide::{Definition, IdeWorld, Tooltip};
use typst_layout::PagedDocument;
use typstation::world::{SourceOverlay, TypstationWorld};

use crate::document::DocumentId;

pub type Sender = UnboundedSender<Request>;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Config {
    root: PathBuf,
    main_name: String,
}

impl Config {
    pub fn new(root: PathBuf, main_name: String) -> Self {
        Self { root, main_name }
    }

    pub fn root(&self) -> &std::path::Path {
        &self.root
    }
}

#[derive(Debug, Clone)]
pub struct DocumentSnapshot {
    pub id: DocumentId,
    pub revision: u64,
    pub path: Option<PathBuf>,
    pub text: String,
}

#[derive(Debug, Clone)]
pub struct Snapshot {
    pub revision: u64,
    pub main_document: Option<DocumentId>,
    pub active_document: DocumentId,
    pub documents: Vec<DocumentSnapshot>,
    pub known_files: Vec<PathBuf>,
}

#[derive(Debug)]
pub enum Request {
    Sync(Snapshot),
    Complete {
        request_id: u64,
        document: DocumentId,
        revision: u64,
        offset: usize,
        explicit: bool,
    },
    Hover {
        request_id: u64,
        document: DocumentId,
        revision: u64,
        offset: usize,
    },
    Definition {
        document: DocumentId,
        revision: u64,
        offset: usize,
    },
    References {
        document: DocumentId,
        revision: u64,
        offset: usize,
    },
    PrepareRename {
        document: DocumentId,
        revision: u64,
        offset: usize,
    },
    Format {
        document: DocumentId,
        revision: u64,
        tab_width: usize,
    },
}

#[derive(Debug, Clone)]
pub enum Event {
    Ready {
        config: Config,
        sender: Sender,
    },
    Completions {
        config: Config,
        request_id: u64,
        document: DocumentId,
        revision: u64,
        items: Vec<EditorCompletion>,
        snippets: Vec<SnippetCompletion>,
    },
    Hover {
        config: Config,
        request_id: u64,
        document: DocumentId,
        revision: u64,
        hover: Option<EditorHover>,
    },
    Definition {
        config: Config,
        document: DocumentId,
        revision: u64,
        location: Option<Location>,
    },
    References {
        config: Config,
        document: DocumentId,
        revision: u64,
        symbol: Option<String>,
        locations: Vec<Location>,
    },
    RenamePrepared {
        config: Config,
        document: DocumentId,
        revision: u64,
        workspace_revision: u64,
        symbol: Option<String>,
        kind: Option<RenameKind>,
        locations: Vec<Location>,
    },
    Formatted {
        config: Config,
        document: DocumentId,
        revision: u64,
        result: Result<String, String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnippetCompletion {
    pub replace: Range<usize>,
    pub insert: String,
    pub placeholders: Vec<Range<usize>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Location {
    pub document: Option<DocumentId>,
    pub path: Option<PathBuf>,
    pub range: Range<usize>,
    pub line: usize,
    pub column: usize,
    pub excerpt: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenameKind {
    Label,
    Identifier,
}

pub fn subscription(config: Config) -> Subscription<Event> {
    Subscription::run_with(config, worker)
}

fn worker(config: &Config) -> impl Stream<Item = Event> + use<> {
    let config = config.clone();

    stream::channel(16, async move |mut output| {
        let (sender, mut requests) = mpsc::unbounded();
        if output
            .send(Event::Ready {
                config: config.clone(),
                sender,
            })
            .await
            .is_err()
        {
            return;
        }

        let mut state = LanguageState::new(&config);
        while let Some(first) = requests.next().await {
            let mut pending = vec![first];
            while let Ok(request) = requests.try_recv() {
                pending.push(request);
            }
            for request in coalesce_requests(pending) {
                let event = match request {
                    Request::Sync(snapshot) => {
                        state.sync(snapshot);
                        None
                    }
                    Request::Complete {
                        request_id,
                        document,
                        revision,
                        offset,
                        explicit,
                    } => state.complete(&config, request_id, document, revision, offset, explicit),
                    Request::Hover {
                        request_id,
                        document,
                        revision,
                        offset,
                    } => state.hover(&config, request_id, document, revision, offset),
                    Request::Definition {
                        document,
                        revision,
                        offset,
                    } => state.definition(&config, document, revision, offset),
                    Request::References {
                        document,
                        revision,
                        offset,
                    } => state.references(&config, document, revision, offset, false),
                    Request::PrepareRename {
                        document,
                        revision,
                        offset,
                    } => state.references(&config, document, revision, offset, true),
                    Request::Format {
                        document,
                        revision,
                        tab_width,
                    } => state.format(&config, document, revision, tab_width),
                };

                if let Some(event) = event
                    && output.send(event).await.is_err()
                {
                    return;
                }
            }
        }
    })
}

fn coalesce_requests(mut requests: Vec<Request>) -> Vec<Request> {
    if let Some(last_sync) = requests
        .iter()
        .rposition(|request| matches!(request, Request::Sync(_)))
    {
        requests.drain(..last_sync);
    }
    let last_completion = requests
        .iter()
        .rposition(|request| matches!(request, Request::Complete { .. }));
    let last_hover = requests
        .iter()
        .rposition(|request| matches!(request, Request::Hover { .. }));

    requests
        .into_iter()
        .enumerate()
        .filter_map(|(index, request)| match request {
            Request::Complete { .. } if Some(index) != last_completion => None,
            Request::Hover { .. } if Some(index) != last_hover => None,
            request => Some(request),
        })
        .collect()
}

struct SyncedDocument {
    revision: u64,
    path: Option<PathBuf>,
    file_id: FileId,
}

struct LanguageState {
    world: TypstationWorld,
    revision: u64,
    active_document: Option<DocumentId>,
    documents: HashMap<DocumentId, SyncedDocument>,
    file_documents: HashMap<FileId, DocumentId>,
    source_ids: Vec<FileId>,
}

impl LanguageState {
    fn new(config: &Config) -> Self {
        Self {
            world: TypstationWorld::with_main(config.root.clone(), &config.main_name),
            revision: 0,
            active_document: None,
            documents: HashMap::new(),
            file_documents: HashMap::new(),
            source_ids: Vec::new(),
        }
    }

    fn sync(&mut self, snapshot: Snapshot) {
        self.revision = snapshot.revision;
        self.active_document = Some(snapshot.active_document);
        self.documents.clear();
        self.file_documents.clear();

        let mut overlays = Vec::new();
        let mut main_source = None;
        let mut known_files = snapshot.known_files;
        let main_id = self.world.main_id();

        for document in snapshot.documents {
            let is_main = Some(document.id) == snapshot.main_document;
            let path = document.path.clone().unwrap_or_else(|| {
                self.world
                    .project_path(main_id)
                    .and_then(|path| path.parent().map(|parent| parent.to_path_buf()))
                    .unwrap_or_else(|| PathBuf::from("."))
                    .join(".typstation")
                    .join(format!("untitled-{}.typ", document.id.get()))
            });
            let file_id = if is_main {
                main_source = Some(document.text.clone());
                main_id
            } else if let Some(file_id) = self.world.project_file_id(&path) {
                overlays.push(SourceOverlay {
                    path: path.clone(),
                    text: document.text,
                });
                file_id
            } else {
                continue;
            };

            known_files.push(path);
            self.documents.insert(
                document.id,
                SyncedDocument {
                    revision: document.revision,
                    path: document.path,
                    file_id,
                },
            );
            self.file_documents.insert(file_id, document.id);
        }

        self.world.set_main_source(main_source.as_deref());
        self.world.set_overlays(overlays);
        self.world.set_known_files(known_files);
        self.source_ids = self.world.files();
    }

    fn current_source(&self, document: DocumentId, revision: u64) -> Option<Source> {
        let synced = self.documents.get(&document)?;
        (synced.revision == revision)
            .then(|| self.world.source(synced.file_id).ok())
            .flatten()
    }

    fn complete(
        &self,
        config: &Config,
        request_id: u64,
        document: DocumentId,
        revision: u64,
        offset: usize,
        explicit: bool,
    ) -> Option<Event> {
        let source = self.current_source(document, revision)?;
        let output = needs_label_document(&source, offset)
            .then(|| typst::compile::<PagedDocument>(&self.world).output.ok())
            .flatten();
        let (from, completions) = typst_ide::autocomplete(
            &self.world,
            output.as_ref(),
            &source,
            offset.min(source.text().len()),
            explicit,
        )?;
        let replace = from..offset.min(source.text().len());
        let mut items = Vec::with_capacity(completions.len());
        let mut snippets = Vec::new();

        for completion in completions {
            let detail = completion
                .detail
                .map(|detail| detail.to_string())
                .unwrap_or_else(|| completion_kind_label(&completion.kind).to_owned());
            let raw_insert = completion
                .apply
                .as_deref()
                .unwrap_or(completion.label.as_str());
            let parsed = parse_snippet(raw_insert);
            let item = EditorCompletion::new(replace.clone(), completion.label.to_string())
                .insert(parsed.text.clone())
                .detail(detail);
            if !parsed.placeholders.is_empty() {
                snippets.push(SnippetCompletion {
                    replace: replace.clone(),
                    insert: parsed.text,
                    placeholders: parsed.placeholders,
                });
            }
            items.push(item);
        }

        Some(Event::Completions {
            config: config.clone(),
            request_id,
            document,
            revision,
            items,
            snippets,
        })
    }

    fn hover(
        &self,
        config: &Config,
        request_id: u64,
        document: DocumentId,
        revision: u64,
        offset: usize,
    ) -> Option<Event> {
        let source = self.current_source(document, revision)?;
        let output = needs_label_document(&source, offset)
            .then(|| typst::compile::<PagedDocument>(&self.world).output.ok())
            .flatten();
        let offset = offset.min(source.text().len());
        let hover = typst_ide::tooltip(&self.world, output.as_ref(), &source, offset, Side::Before)
            .or_else(|| {
                typst_ide::tooltip(&self.world, output.as_ref(), &source, offset, Side::After)
            })
            .map(|tooltip| EditorHover {
                range: leaf_range(&source, offset).unwrap_or(offset..offset),
                content: match tooltip {
                    Tooltip::Text(text) | Tooltip::Code(text) => text.to_string(),
                },
            });

        Some(Event::Hover {
            config: config.clone(),
            request_id,
            document,
            revision,
            hover,
        })
    }

    fn definition(
        &self,
        config: &Config,
        document: DocumentId,
        revision: u64,
        offset: usize,
    ) -> Option<Event> {
        let source = self.current_source(document, revision)?;
        let output = needs_label_document(&source, offset)
            .then(|| typst::compile::<PagedDocument>(&self.world).output.ok())
            .flatten();
        let offset = offset.min(source.text().len());
        let definition =
            typst_ide::definition(&self.world, output.as_ref(), &source, offset, Side::Before)
                .or_else(|| {
                    typst_ide::definition(
                        &self.world,
                        output.as_ref(),
                        &source,
                        offset,
                        Side::After,
                    )
                });
        let location = definition.and_then(|definition| self.definition_location(definition));

        Some(Event::Definition {
            config: config.clone(),
            document,
            revision,
            location,
        })
    }

    fn references(
        &self,
        config: &Config,
        document: DocumentId,
        revision: u64,
        offset: usize,
        rename: bool,
    ) -> Option<Event> {
        let source = self.current_source(document, revision)?;
        let symbol = self.symbol_at(&source, offset)?;
        let name = symbol.name().to_owned();
        let locations = self.symbol_locations(&symbol);

        Some(if rename {
            Event::RenamePrepared {
                config: config.clone(),
                document,
                revision,
                workspace_revision: self.revision,
                symbol: Some(name),
                kind: Some(symbol.rename_kind()),
                locations,
            }
        } else {
            Event::References {
                config: config.clone(),
                document,
                revision,
                symbol: Some(name),
                locations,
            }
        })
    }

    fn format(
        &self,
        config: &Config,
        document: DocumentId,
        revision: u64,
        tab_width: usize,
    ) -> Option<Event> {
        let source = self.current_source(document, revision)?;
        let formatter = typstyle_core::Typstyle::new(
            typstyle_core::Config::new()
                .with_width(80)
                .with_tab_spaces(tab_width.max(1)),
        );
        let result = formatter
            .format_text(source.text())
            .render()
            .map_err(|error| error.to_string());

        Some(Event::Formatted {
            config: config.clone(),
            document,
            revision,
            result,
        })
    }

    fn definition_location(&self, definition: Definition) -> Option<Location> {
        match definition {
            Definition::Span(span) => {
                let (id, range) = self.world.span_range(span.into())?;
                self.location(id, range)
            }
            Definition::File(id) => self.location(id, 0..0),
            Definition::Std(_) => None,
        }
    }

    fn symbol_at(&self, source: &Source, offset: usize) -> Option<Symbol> {
        let offset = offset.min(source.text().len());
        let root = LinkedNode::new(source.root());
        let leaf = root
            .leaf_at(offset, Side::Before)
            .or_else(|| root.leaf_at(offset, Side::After))?;

        if leaf.kind() == SyntaxKind::Label {
            return Some(Symbol::Label(
                leaf.leaf_text()
                    .trim_start_matches('<')
                    .trim_end_matches('>')
                    .to_owned(),
            ));
        }
        if leaf.kind() == SyntaxKind::RefMarker {
            return Some(Symbol::Label(
                leaf.leaf_text().trim_start_matches('@').to_owned(),
            ));
        }
        if !matches!(leaf.kind(), SyntaxKind::Ident | SyntaxKind::MathIdent) {
            return None;
        }

        let name = leaf.leaf_text().to_string();
        let definition = typst_ide::definition(
            &self.world,
            Option::<&PagedDocument>::None,
            source,
            offset,
            Side::Before,
        )
        .or_else(|| {
            typst_ide::definition(
                &self.world,
                Option::<&PagedDocument>::None,
                source,
                offset,
                Side::After,
            )
        });
        let span = match definition {
            Some(Definition::Span(span)) => span,
            _ if definition_context(&leaf) => leaf.span(),
            _ => return None,
        };
        Some(Symbol::Identifier { name, span })
    }

    fn symbol_locations(&self, symbol: &Symbol) -> Vec<Location> {
        let mut locations = Vec::new();
        for id in self.source_ids.iter().copied() {
            let Ok(source) = self.world.source(id) else {
                continue;
            };
            collect_symbol_ranges(&self.world, &source, symbol, &mut |range| {
                if let Some(location) = self.location(id, range) {
                    locations.push(location);
                }
            });
        }
        locations.sort_by(|left, right| {
            left.path
                .cmp(&right.path)
                .then(left.document.cmp(&right.document))
                .then(left.range.start.cmp(&right.range.start))
        });
        locations.dedup_by(|left, right| {
            left.document == right.document && left.path == right.path && left.range == right.range
        });
        locations
    }

    fn location(&self, id: FileId, range: Range<usize>) -> Option<Location> {
        let source = self.world.source(id).ok()?;
        let range = range.start.min(source.text().len())..range.end.min(source.text().len());
        let (line, column) = source.lines().byte_to_line_column(range.start)?;
        let line_range = source.lines().line_to_range(line)?;
        let excerpt = source.text()[line_range]
            .trim_end_matches(['\r', '\n'])
            .to_owned();
        let document = self.file_documents.get(&id).copied();
        let path = match document {
            Some(document) => self
                .documents
                .get(&document)
                .and_then(|document| document.path.clone()),
            None => self.world.project_path(id),
        };

        Some(Location {
            document,
            path,
            range,
            line: line + 1,
            column: column + 1,
            excerpt,
        })
    }
}

#[derive(Debug)]
enum Symbol {
    Label(String),
    Identifier { name: String, span: Span },
}

impl Symbol {
    fn name(&self) -> &str {
        match self {
            Self::Label(name) | Self::Identifier { name, .. } => name,
        }
    }

    fn rename_kind(&self) -> RenameKind {
        match self {
            Self::Label(_) => RenameKind::Label,
            Self::Identifier { .. } => RenameKind::Identifier,
        }
    }
}

fn collect_symbol_ranges(
    world: &TypstationWorld,
    source: &Source,
    symbol: &Symbol,
    output: &mut impl FnMut(Range<usize>),
) {
    fn visit(
        world: &TypstationWorld,
        source: &Source,
        node: &LinkedNode<'_>,
        symbol: &Symbol,
        output: &mut impl FnMut(Range<usize>),
    ) {
        if node.children().next().is_none() {
            match symbol {
                Symbol::Label(name) if node.kind() == SyntaxKind::Label => {
                    let text = node.leaf_text();
                    if text.trim_start_matches('<').trim_end_matches('>') == name {
                        output(node.range().start + 1..node.range().end.saturating_sub(1));
                    }
                }
                Symbol::Label(name) if node.kind() == SyntaxKind::RefMarker => {
                    let text = node.leaf_text();
                    if text.trim_start_matches('@') == name {
                        output(node.range().start + 1..node.range().end);
                    }
                }
                Symbol::Identifier { name, span }
                    if matches!(node.kind(), SyntaxKind::Ident | SyntaxKind::MathIdent)
                        && node.leaf_text() == name =>
                {
                    let offset = node.range().end;
                    let definition = typst_ide::definition(
                        world,
                        Option::<&PagedDocument>::None,
                        source,
                        offset,
                        Side::Before,
                    )
                    .or_else(|| {
                        typst_ide::definition(
                            world,
                            Option::<&PagedDocument>::None,
                            source,
                            node.range().start,
                            Side::After,
                        )
                    });
                    if matches!(definition, Some(Definition::Span(found)) if found == *span)
                        || node.span() == *span
                    {
                        output(node.range());
                    }
                }
                _ => {}
            }
            return;
        }

        for child in node.children() {
            visit(world, source, &child, symbol, output);
        }
    }

    visit(
        world,
        source,
        &LinkedNode::new(source.root()),
        symbol,
        output,
    );
}

fn needs_label_document(source: &Source, offset: usize) -> bool {
    let offset = offset.min(source.text().len());
    source.text()[..offset]
        .chars()
        .next_back()
        .is_some_and(|character| matches!(character, '@' | '<'))
        || leaf_range(source, offset).is_some_and(|range| {
            source.text()[range]
                .chars()
                .next()
                .is_some_and(|character| matches!(character, '@' | '<'))
        })
}

fn definition_context(node: &LinkedNode<'_>) -> bool {
    let mut current = node.clone();
    while let Some(parent) = current.parent() {
        if matches!(
            parent.kind(),
            SyntaxKind::LetBinding
                | SyntaxKind::Params
                | SyntaxKind::ForLoop
                | SyntaxKind::ImportItems
                | SyntaxKind::RenamedImportItem
        ) {
            return true;
        }
        current = parent.clone();
    }
    false
}

fn leaf_range(source: &Source, offset: usize) -> Option<Range<usize>> {
    let root = LinkedNode::new(source.root());
    root.leaf_at(offset, Side::Before)
        .or_else(|| root.leaf_at(offset, Side::After))
        .map(|leaf| leaf.range())
}

fn completion_kind_label(kind: &typst_ide::CompletionKind) -> &'static str {
    use typst_ide::CompletionKind;
    match kind {
        CompletionKind::Syntax => "Sintaxe",
        CompletionKind::Func => "Função",
        CompletionKind::Type => "Tipo",
        CompletionKind::Param => "Parâmetro",
        CompletionKind::Constant => "Constante",
        CompletionKind::Path => "Caminho",
        CompletionKind::Package => "Pacote",
        CompletionKind::Label => "Label",
        CompletionKind::Font => "Fonte",
        CompletionKind::Symbol(_) => "Símbolo",
    }
}

struct ParsedSnippet {
    text: String,
    placeholders: Vec<Range<usize>>,
}

fn parse_snippet(snippet: &str) -> ParsedSnippet {
    let mut text = String::with_capacity(snippet.len());
    let mut placeholders = Vec::new();
    let mut remaining = snippet;

    while let Some(start) = remaining.find("${") {
        text.push_str(&remaining[..start]);
        let after_open = &remaining[start + 2..];
        let Some(end) = after_open.find('}') else {
            text.push_str(&remaining[start..]);
            remaining = "";
            break;
        };
        let marker = &after_open[..end];
        let value = marker
            .split_once(':')
            .filter(|(index, _)| index.chars().all(|character| character.is_ascii_digit()))
            .map_or(marker, |(_, value)| value);
        let placeholder_start = text.len();
        text.push_str(value);
        placeholders.push(placeholder_start..text.len());
        remaining = &after_open[end + 1..];
    }
    text.push_str(remaining);

    ParsedSnippet { text, placeholders }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{Document, Documents};

    fn state_with_source(source: &str) -> (LanguageState, Config, DocumentId, u64) {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let path = root.join("language-test.typ");
        let documents = Documents::new(Document::opened(path.clone(), source.to_owned()));
        let id = documents.active_id();
        let revision = documents.active().revision();
        let config = Config::new(root, "language-test.typ".to_owned());
        let mut state = LanguageState::new(&config);
        state.sync(Snapshot {
            revision: 7,
            main_document: Some(id),
            active_document: id,
            documents: vec![DocumentSnapshot {
                id,
                revision,
                path: Some(path.clone()),
                text: source.to_owned(),
            }],
            known_files: vec![path],
        });
        (state, config, id, revision)
    }

    #[test]
    fn request_coalescing_keeps_latest_transient_queries_and_explicit_commands() {
        let documents = Documents::new(Document::new());
        let id = documents.active_id();
        let requests = vec![
            Request::Complete {
                request_id: 1,
                document: id,
                revision: 0,
                offset: 0,
                explicit: false,
            },
            Request::Hover {
                request_id: 2,
                document: id,
                revision: 0,
                offset: 0,
            },
            Request::Definition {
                document: id,
                revision: 0,
                offset: 0,
            },
            Request::Complete {
                request_id: 3,
                document: id,
                revision: 0,
                offset: 0,
                explicit: true,
            },
            Request::Hover {
                request_id: 4,
                document: id,
                revision: 0,
                offset: 0,
            },
        ];
        let coalesced = coalesce_requests(requests);

        assert_eq!(coalesced.len(), 3);
        assert!(matches!(coalesced[0], Request::Definition { .. }));
        assert!(matches!(
            coalesced[1],
            Request::Complete { request_id: 3, .. }
        ));
        assert!(matches!(coalesced[2], Request::Hover { request_id: 4, .. }));
    }

    #[test]
    fn snippet_markers_become_plain_text_and_ranges() {
        let parsed = parse_snippet("let ${name}(${2:params}) = ${}");

        assert_eq!(parsed.text, "let name(params) = ");
        assert_eq!(parsed.placeholders, vec![4..8, 9..15, 19..19]);
    }

    #[test]
    fn malformed_snippet_is_preserved() {
        let parsed = parse_snippet("text ${open");

        assert_eq!(parsed.text, "text ${open");
        assert!(parsed.placeholders.is_empty());
    }

    #[test]
    fn typst_ide_completions_are_converted_for_the_editor() {
        let (state, config, id, revision) = state_with_source("#tex");
        let Some(Event::Completions { items, .. }) =
            state.complete(&config, 1, id, revision, 4, false)
        else {
            panic!("completion event expected");
        };

        assert!(items.iter().any(|item| item.label == "text"));
    }

    #[test]
    fn automatic_completion_refines_hash_function_names() {
        let (state, config, id, revision) = state_with_source("#fig");
        let Some(Event::Completions {
            items, snippets, ..
        }) = state.complete(&config, 1, id, revision, 4, false)
        else {
            panic!("completion event expected");
        };

        assert!(items.iter().any(|item| item.label == "figure"));
        assert!(
            snippets
                .iter()
                .any(|snippet| snippet.insert.starts_with("figure("))
        );
    }

    #[test]
    fn label_references_use_ranges_without_markers() {
        let source = "= Título <secao>\nVeja @secao.";
        let (state, config, id, revision) = state_with_source(source);
        let cursor = source.find("@secao").unwrap() + 3;
        let Some(Event::References {
            symbol, locations, ..
        }) = state.references(&config, id, revision, cursor, false)
        else {
            panic!("references event expected");
        };

        assert_eq!(symbol.as_deref(), Some("secao"));
        assert_eq!(locations.len(), 2);
        assert!(
            locations
                .iter()
                .all(|location| { &source[location.range.clone()] == "secao" })
        );
    }

    #[test]
    fn identifier_references_respect_typst_name_resolution() {
        let source = "#let total = 1\n#total + { let total = 2; total }";
        let (state, config, id, revision) = state_with_source(source);
        let cursor = source.find("#total").unwrap() + 3;
        let Some(Event::References { locations, .. }) =
            state.references(&config, id, revision, cursor, false)
        else {
            panic!("references event expected");
        };

        assert_eq!(locations.len(), 2);
        assert_eq!(locations[0].range, 5..10);
        assert_eq!(locations[1].range, 16..21);
    }

    #[test]
    fn identifier_definition_itself_can_start_a_rename() {
        let source = "#let total = 1\n#total";
        let (state, config, id, revision) = state_with_source(source);
        let cursor = source.find("total").unwrap() + 2;
        let Some(Event::RenamePrepared {
            kind, locations, ..
        }) = state.references(&config, id, revision, cursor, true)
        else {
            panic!("rename event expected");
        };

        assert_eq!(kind, Some(RenameKind::Identifier));
        assert_eq!(locations.len(), 2);
    }

    #[test]
    fn formatter_uses_the_configured_typstyle_engine() {
        let source = "#let soma(a,b)=(a+b)";
        let (state, config, id, revision) = state_with_source(source);
        let Some(Event::Formatted { result, .. }) = state.format(&config, id, revision, 2) else {
            panic!("format event expected");
        };
        let formatted = result.expect("valid Typst should format");

        assert_ne!(formatted, source);
        assert!(formatted.contains("soma"));
    }
}

use std::{ops::Range, path::PathBuf};

use iced::{
    Subscription,
    futures::{
        SinkExt, Stream, StreamExt,
        channel::mpsc::{self, UnboundedSender},
    },
    stream,
};
use typst::{
    diag::SourceDiagnostic,
    foundations::{NativeElement, StyleChain},
    introspection::Introspector,
    layout::Abs,
    model::{HeadingElem, OutlineNode as TypstOutlineNode},
};
use typst_html::HtmlDocument;
use typst_iced_editor::Diagnostic;
use typst_layout::PagedDocument;
use typstation::world::{SourceOverlay, TypstationWorld};

use crate::source_map::{self, SourceRegion, SourceTarget};

const TYPOGRAPHIC_POINTS_PER_INCH: f32 = 72.0;

pub type Sender = UnboundedSender<Request>;

#[derive(Debug)]
pub struct Request {
    pub id: u64,
    pub revision: u64,
    pub source: Option<String>,
    pub overlays: Vec<SourceOverlay>,
    pub reset_files: bool,
    pub purpose: Purpose,
    pub export_options: ExportOptions,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Purpose {
    Preview,
    Export(ExportFormat),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    Pdf,
    Svg,
    Html,
    Png,
}

impl ExportFormat {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Pdf => "PDF",
            Self::Svg => "SVG",
            Self::Html => "HTML",
            Self::Png => "PNG",
        }
    }

    pub const fn extension(self) -> &'static str {
        match self {
            Self::Pdf => "pdf",
            Self::Svg => "svg",
            Self::Html => "html",
            Self::Png => "png",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExportOptions {
    pub pdf_tagged: bool,
    pub pdf_pretty: bool,
    pub svg_render_bleed: bool,
    pub svg_pretty: bool,
    pub svg_page_gap: u16,
    pub html_pretty: bool,
    pub png_ppi: u16,
    pub png_render_bleed: bool,
    pub png_page_gap: u16,
}

impl Default for ExportOptions {
    fn default() -> Self {
        Self {
            pdf_tagged: true,
            pdf_pretty: false,
            svg_render_bleed: false,
            svg_pretty: false,
            svg_page_gap: 12,
            html_pretty: true,
            png_ppi: 144,
            png_render_bleed: false,
            png_page_gap: 12,
        }
    }
}

#[derive(Debug, Clone)]
pub enum Event {
    Ready { config: Config, sender: Sender },
    Finished { config: Config, output: Output },
}

#[derive(Debug, Clone)]
pub struct Output {
    pub id: u64,
    pub revision: u64,
    pub purpose: Purpose,
    pub pages: Vec<RenderedPage>,
    pub artifact: Option<Vec<u8>>,
    pub diagnostics: Vec<ReportedDiagnostic>,
    pub outline: Vec<DocumentOutlineItem>,
    pub page_count: usize,
    pub warning_count: usize,
    pub error_count: usize,
    pub summary: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentOutlineItem {
    pub title: String,
    pub target: SourceTarget,
    pub range: Range<usize>,
    pub children: Vec<DocumentOutlineItem>,
}

#[derive(Debug)]
struct NavigableHeading {
    title: String,
    target: SourceTarget,
    range: Range<usize>,
}

#[derive(Debug, Clone)]
pub struct RenderedPage {
    pub svg: Vec<u8>,
    pub width: f32,
    pub height: f32,
    pub regions: Vec<SourceRegion>,
}

pub type DiagnosticTarget = SourceTarget;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticSeverity {
    Error,
    Warning,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportedDiagnostic {
    pub target: DiagnosticTarget,
    pub range: Range<usize>,
    pub severity: DiagnosticSeverity,
    pub message: String,
}

impl ReportedDiagnostic {
    pub fn editor_diagnostic(&self) -> Diagnostic {
        match self.severity {
            DiagnosticSeverity::Error => {
                Diagnostic::error(self.range.clone(), self.message.clone())
            }
            DiagnosticSeverity::Warning => {
                Diagnostic::warning(self.range.clone(), self.message.clone())
            }
        }
    }
}

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

pub fn subscription(config: Config) -> Subscription<Event> {
    Subscription::run_with(config, worker)
}

fn worker(config: &Config) -> impl Stream<Item = Event> + use<> {
    let config = config.clone();

    stream::channel(8, async move |mut output| {
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

        let mut world = TypstationWorld::with_main(config.root.clone(), &config.main_name);

        while let Some(first) = requests.next().await {
            let mut pending = vec![first];
            while let Ok(request) = requests.try_recv() {
                pending.push(request);
            }

            for request in coalesce_requests(pending) {
                let compiled = compile(&mut world, request);

                if output
                    .send(Event::Finished {
                        config: config.clone(),
                        output: compiled,
                    })
                    .await
                    .is_err()
                {
                    return;
                }
            }
        }
    })
}

fn coalesce_requests(mut requests: Vec<Request>) -> Vec<Request> {
    let latest_preview = requests
        .iter()
        .rposition(|request| request.purpose == Purpose::Preview);
    let preview_requires_reset = requests
        .iter()
        .any(|request| request.purpose == Purpose::Preview && request.reset_files);

    requests
        .drain(..)
        .enumerate()
        .filter_map(|(index, mut request)| {
            if request.purpose == Purpose::Preview && Some(index) == latest_preview {
                request.reset_files |= preview_requires_reset;
                Some(request)
            } else {
                (request.purpose != Purpose::Preview).then_some(request)
            }
        })
        .collect()
}

fn compile(world: &mut TypstationWorld, request: Request) -> Output {
    let Request {
        id,
        revision,
        source,
        overlays,
        reset_files,
        purpose,
        export_options,
    } = request;

    if reset_files {
        world.reset_files();
    }

    world.set_main_source(source.as_deref());
    world.set_overlays(overlays);

    match purpose {
        Purpose::Export(ExportFormat::Html) => {
            compile_html(world, id, revision, purpose, export_options)
        }
        Purpose::Preview
        | Purpose::Export(ExportFormat::Pdf)
        | Purpose::Export(ExportFormat::Svg)
        | Purpose::Export(ExportFormat::Png) => {
            compile_paged(world, id, revision, purpose, export_options)
        }
    }
}

fn compile_paged(
    world: &TypstationWorld,
    id: u64,
    revision: u64,
    purpose: Purpose,
    export_options: ExportOptions,
) -> Output {
    let result = typst::compile::<PagedDocument>(world);
    let (mut diagnostics, mut summary) = reported_warnings(world, &result.warnings);
    let warning_count = result.warnings.len();

    match result.output {
        Ok(document) if !document.pages().is_empty() => {
            let page_count = document.pages().len();
            let outline = if purpose == Purpose::Preview {
                document_outline(world, &document)
            } else {
                Vec::new()
            };
            let mut output = Output {
                id,
                revision,
                purpose,
                pages: Vec::new(),
                artifact: None,
                diagnostics,
                outline,
                page_count,
                warning_count,
                error_count: 0,
                summary,
            };

            match purpose {
                Purpose::Preview => {
                    let options = typst_svg::SvgOptions::default();
                    output.pages = document
                        .pages()
                        .iter()
                        .map(|page| RenderedPage {
                            svg: typst_svg::svg(page, &options).into_bytes(),
                            width: page.frame.width().to_pt() as f32,
                            height: page.frame.height().to_pt() as f32,
                            regions: source_map::page_regions(world, page),
                        })
                        .collect();
                }
                Purpose::Export(ExportFormat::Pdf) => {
                    let options = typst_pdf::PdfOptions {
                        tagged: export_options.pdf_tagged,
                        pretty: export_options.pdf_pretty,
                        ..typst_pdf::PdfOptions::default()
                    };
                    match typst_pdf::pdf(&document, &options) {
                        Ok(pdf) => output.artifact = Some(pdf),
                        Err(errors) => {
                            output.error_count = errors.len();
                            output.summary = errors
                                .first()
                                .map(|error| error.message.to_string())
                                .or(output.summary);

                            for error in &errors {
                                if let Some(diagnostic) = editor_diagnostic(world, error) {
                                    output.diagnostics.push(diagnostic);
                                }
                            }
                        }
                    }
                }
                Purpose::Export(ExportFormat::Svg) => {
                    let options = typst_svg::SvgOptions {
                        render_bleed: export_options.svg_render_bleed,
                        pretty: export_options.svg_pretty,
                    };
                    output.artifact = Some(
                        typst_svg::svg_merged(
                            &document,
                            &options,
                            Abs::pt(f64::from(export_options.svg_page_gap)),
                        )
                        .into_bytes(),
                    );
                }
                Purpose::Export(ExportFormat::Png) => {
                    let options = typst_render::RenderOptions {
                        pixel_per_pt: (f64::from(export_options.png_ppi)
                            / f64::from(TYPOGRAPHIC_POINTS_PER_INCH))
                        .into(),
                        render_bleed: export_options.png_render_bleed,
                    };
                    let pixmap = typst_render::render_merged(
                        &document,
                        &options,
                        Abs::pt(f64::from(export_options.png_page_gap)),
                        None,
                    );
                    match pixmap.encode_png() {
                        Ok(png) => output.artifact = Some(png),
                        Err(error) => {
                            output.error_count = 1;
                            output.summary = Some(format!("falha ao codificar PNG: {error}"));
                        }
                    }
                }
                Purpose::Export(ExportFormat::Html) => unreachable!(),
            }

            output
        }
        Ok(_) => Output {
            id,
            revision,
            purpose,
            pages: Vec::new(),
            artifact: None,
            diagnostics,
            outline: Vec::new(),
            page_count: 0,
            warning_count,
            error_count: 1,
            summary: Some("O documento compilado não possui páginas".to_owned()),
        },
        Err(errors) => {
            summary = errors
                .first()
                .map(|error| error.message.to_string())
                .or(summary);

            for error in &errors {
                if let Some(diagnostic) = editor_diagnostic(world, error) {
                    diagnostics.push(diagnostic);
                }
            }

            Output {
                id,
                revision,
                purpose,
                pages: Vec::new(),
                artifact: None,
                diagnostics,
                outline: Vec::new(),
                page_count: 0,
                warning_count,
                error_count: errors.len(),
                summary,
            }
        }
    }
}

fn compile_html(
    world: &TypstationWorld,
    id: u64,
    revision: u64,
    purpose: Purpose,
    export_options: ExportOptions,
) -> Output {
    let result = typst::compile::<HtmlDocument>(world);
    let (mut diagnostics, mut summary) = reported_warnings(world, &result.warnings);
    let warning_count = result.warnings.len();
    let mut output = Output {
        id,
        revision,
        purpose,
        pages: Vec::new(),
        artifact: None,
        diagnostics: Vec::new(),
        outline: Vec::new(),
        page_count: 0,
        warning_count,
        error_count: 0,
        summary: None,
    };

    match result.output {
        Ok(document) => {
            let options = typst_html::HtmlOptions {
                pretty: export_options.html_pretty,
            };
            match typst_html::html(&document, &options) {
                Ok(html) => output.artifact = Some(html.into_bytes()),
                Err(errors) => {
                    output.error_count = errors.len();
                    summary = errors
                        .first()
                        .map(|error| error.message.to_string())
                        .or(summary);
                    append_reported_errors(world, &errors, &mut diagnostics);
                }
            }
        }
        Err(errors) => {
            output.error_count = errors.len();
            summary = errors
                .first()
                .map(|error| error.message.to_string())
                .or(summary);
            append_reported_errors(world, &errors, &mut diagnostics);
        }
    }

    output.diagnostics = diagnostics;
    output.summary = summary;
    output
}

fn reported_warnings(
    world: &TypstationWorld,
    warnings: &[SourceDiagnostic],
) -> (Vec<ReportedDiagnostic>, Option<String>) {
    let mut diagnostics = Vec::new();
    let mut summary = None;
    for warning in warnings {
        summary.get_or_insert_with(|| warning.message.to_string());
        if let Some(diagnostic) = editor_diagnostic(world, warning) {
            diagnostics.push(diagnostic);
        }
    }
    (diagnostics, summary)
}

fn append_reported_errors(
    world: &TypstationWorld,
    errors: &[SourceDiagnostic],
    diagnostics: &mut Vec<ReportedDiagnostic>,
) {
    for error in errors {
        if let Some(diagnostic) = editor_diagnostic(world, error) {
            diagnostics.push(diagnostic);
        }
    }
}

fn document_outline(world: &TypstationWorld, document: &PagedDocument) -> Vec<DocumentOutlineItem> {
    let elements = document.introspector().query(&HeadingElem::ELEM.select());
    let headings = elements.iter().filter_map(|element| {
        let heading = element.to_packed::<HeadingElem>()?;
        let level = heading.resolve_level(StyleChain::default());
        let include = heading.outlined.get(StyleChain::default());
        let (target, range) = source_map::span_source_range(world, heading.span())?;
        let body = heading.body.plain_text();
        let title = match &heading.numbers {
            Some(numbers) => format!("{numbers} {body}"),
            None => body.to_string(),
        };
        let mut title = title.split_whitespace().collect::<Vec<_>>().join(" ");
        if let Some(label) = heading.label() {
            let label = label.resolve();
            if title.is_empty() {
                title = format!("<{label}>");
            } else {
                title.push_str(&format!(" <{label}>"));
            }
        }

        Some((
            NavigableHeading {
                title: if title.is_empty() {
                    "Tópico sem título".to_owned()
                } else {
                    title
                },
                target,
                range,
            },
            level,
            include,
        ))
    });

    TypstOutlineNode::build_tree(headings)
        .into_iter()
        .map(convert_outline_node)
        .collect()
}

fn convert_outline_node(node: TypstOutlineNode<NavigableHeading>) -> DocumentOutlineItem {
    DocumentOutlineItem {
        title: node.entry.title,
        target: node.entry.target,
        range: node.entry.range,
        children: node
            .children
            .into_iter()
            .map(convert_outline_node)
            .collect(),
    }
}

fn editor_diagnostic(
    world: &TypstationWorld,
    diagnostic: &SourceDiagnostic,
) -> Option<ReportedDiagnostic> {
    let (id, range) = world.span_range(diagnostic.span)?;
    let target = if world.is_main(id) {
        DiagnosticTarget::Main
    } else {
        DiagnosticTarget::ProjectFile(world.project_path(id)?)
    };
    let message = diagnostic.message.to_string();

    Some(ReportedDiagnostic {
        target,
        range,
        severity: match diagnostic.severity {
            typst::diag::Severity::Error => DiagnosticSeverity::Error,
            typst::diag::Severity::Warning => DiagnosticSeverity::Warning,
        },
        message,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn request(id: u64, purpose: Purpose) -> Request {
        Request {
            id,
            revision: id,
            source: Some(format!("Revisão {id}")),
            overlays: Vec::new(),
            reset_files: false,
            purpose,
            export_options: ExportOptions::default(),
        }
    }

    #[test]
    fn coalescing_keeps_only_the_latest_preview_and_every_export() {
        let requests = vec![
            Request {
                reset_files: true,
                ..request(1, Purpose::Preview)
            },
            request(2, Purpose::Export(ExportFormat::Pdf)),
            request(3, Purpose::Preview),
            request(4, Purpose::Export(ExportFormat::Svg)),
            request(5, Purpose::Preview),
        ];

        let coalesced = coalesce_requests(requests);
        let ids = coalesced
            .iter()
            .map(|request| request.id)
            .collect::<Vec<_>>();

        assert_eq!(ids, vec![2, 4, 5]);
        assert_eq!(coalesced[0].purpose, Purpose::Export(ExportFormat::Pdf));
        assert_eq!(coalesced[1].purpose, Purpose::Export(ExportFormat::Svg));
        assert_eq!(coalesced[2].purpose, Purpose::Preview);
        assert!(coalesced[2].reset_files);
    }

    #[test]
    fn coalescing_does_not_require_a_preview_request() {
        let requests = vec![
            request(1, Purpose::Export(ExportFormat::Pdf)),
            request(2, Purpose::Export(ExportFormat::Html)),
        ];

        let ids = coalesce_requests(requests)
            .into_iter()
            .map(|request| request.id)
            .collect::<Vec<_>>();

        assert_eq!(ids, vec![1, 2]);
    }

    #[test]
    fn compiler_generates_multiple_pages_and_editor_diagnostics() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let mut world = TypstationWorld::new(root);

        let preview = compile(
            &mut world,
            Request {
                id: 1,
                revision: 4,
                source: Some("First page\n#pagebreak()\nSecond page".to_owned()),
                overlays: Vec::new(),
                reset_files: true,
                purpose: Purpose::Preview,
                export_options: ExportOptions::default(),
            },
        );

        assert_eq!(preview.id, 1);
        assert_eq!(preview.revision, 4);
        assert_eq!(preview.page_count, 2);
        assert_eq!(preview.error_count, 0);
        assert_eq!(preview.pages.len(), 2);
        assert!(preview.artifact.is_none());
        assert!(preview.outline.is_empty());
        assert!(preview.pages.iter().all(|page| !page.regions.is_empty()));
        assert!(
            preview.pages[0]
                .regions
                .iter()
                .any(|region| region.target == DiagnosticTarget::Main && region.range.start < 10)
        );
        let second_page_start = "First page\n#pagebreak()\n".len();
        assert!(preview.pages[1].regions.iter().any(|region| {
            region.target == DiagnosticTarget::Main && region.range.start >= second_page_start
        }));

        let pdf = compile(
            &mut world,
            Request {
                id: 2,
                revision: 4,
                source: Some("Exported page".to_owned()),
                overlays: Vec::new(),
                reset_files: false,
                purpose: Purpose::Export(ExportFormat::Pdf),
                export_options: ExportOptions::default(),
            },
        );

        assert_eq!(pdf.error_count, 0);
        assert!(pdf.pages.is_empty());
        assert!(
            pdf.artifact
                .as_deref()
                .is_some_and(|bytes| bytes.starts_with(b"%PDF-"))
        );

        let failure = compile(
            &mut world,
            Request {
                id: 3,
                revision: 5,
                source: Some("#let value =".to_owned()),
                overlays: Vec::new(),
                reset_files: false,
                purpose: Purpose::Preview,
                export_options: ExportOptions::default(),
            },
        );

        assert!(failure.error_count > 0);
        assert!(failure.pages.is_empty());
        assert!(!failure.diagnostics.is_empty());
    }

    #[test]
    fn compiler_exports_svg_with_the_selected_layout_options() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let mut world = TypstationWorld::new(root);
        let options = ExportOptions {
            svg_pretty: true,
            svg_page_gap: 24,
            ..ExportOptions::default()
        };

        let output = compile(
            &mut world,
            Request {
                id: 1,
                revision: 1,
                source: Some("Primeira página\n#pagebreak()\nSegunda página".to_owned()),
                overlays: Vec::new(),
                reset_files: true,
                purpose: Purpose::Export(ExportFormat::Svg),
                export_options: options,
            },
        );

        let svg = String::from_utf8(output.artifact.expect("SVG should be generated"))
            .expect("SVG should be UTF-8");
        assert_eq!(output.error_count, 0);
        assert_eq!(output.page_count, 2);
        assert!(svg.starts_with("<svg"));
        assert!(svg.contains('\n'));
    }

    #[test]
    fn compiler_exports_experimental_html() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let mut world = TypstationWorld::new(root);

        let output = compile(
            &mut world,
            Request {
                id: 1,
                revision: 1,
                source: Some("= Título exportado\n\nConteúdo do documento.".to_owned()),
                overlays: Vec::new(),
                reset_files: true,
                purpose: Purpose::Export(ExportFormat::Html),
                export_options: ExportOptions {
                    html_pretty: true,
                    ..ExportOptions::default()
                },
            },
        );

        let html = String::from_utf8(output.artifact.expect("HTML should be generated"))
            .expect("HTML should be UTF-8");
        assert_eq!(output.error_count, 0);
        assert!(html.contains("<!DOCTYPE html>"));
        assert!(html.contains("Título exportado"));
        assert!(html.contains("Conteúdo do documento."));
        assert!(html.contains('\n'));
    }

    #[test]
    fn compiler_exports_merged_png_at_the_selected_resolution() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let mut world = TypstationWorld::new(root);
        let output = compile(
            &mut world,
            Request {
                id: 1,
                revision: 1,
                source: Some("Primeira página\n#pagebreak()\nSegunda página".to_owned()),
                overlays: Vec::new(),
                reset_files: true,
                purpose: Purpose::Export(ExportFormat::Png),
                export_options: ExportOptions {
                    png_ppi: 72,
                    png_page_gap: 24,
                    ..ExportOptions::default()
                },
            },
        );

        let png = output.artifact.expect("PNG should be generated");
        assert_eq!(output.error_count, 0);
        assert_eq!(output.page_count, 2);
        assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));
    }

    #[test]
    fn compiler_builds_a_semantic_document_outline() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let mut world = TypstationWorld::new(root);
        let source =
            "= Introdução\n== Fundamentos\n#heading(outlined: false)[Oculto]\n= Próximos passos";

        let output = compile(
            &mut world,
            Request {
                id: 1,
                revision: 1,
                source: Some(source.to_owned()),
                overlays: Vec::new(),
                reset_files: true,
                purpose: Purpose::Preview,
                export_options: ExportOptions::default(),
            },
        );

        assert_eq!(output.error_count, 0);
        assert_eq!(output.outline.len(), 2);
        assert_eq!(output.outline[0].title, "Introdução");
        assert_eq!(output.outline[0].children.len(), 1);
        assert_eq!(output.outline[0].children[0].title, "Fundamentos");
        assert_eq!(output.outline[1].title, "Próximos passos");
        assert_eq!(output.outline[0].target, SourceTarget::Main);
        assert_eq!(&source[output.outline[0].range.clone()], "= Introdução");
    }

    #[test]
    fn semantic_outline_displays_heading_labels() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let mut world = TypstationWorld::new(root);
        let output = compile(
            &mut world,
            Request {
                id: 1,
                revision: 1,
                source: Some("= Introdução <intro>".to_owned()),
                overlays: Vec::new(),
                reset_files: true,
                purpose: Purpose::Preview,
                export_options: ExportOptions::default(),
            },
        );

        assert_eq!(output.outline[0].title, "Introdução <intro>");
    }

    #[test]
    fn imported_headings_keep_their_project_source() {
        let directory = tempfile::tempdir().expect("a temporary project can be created");
        let imported = directory.path().join("chapter.typ");
        fs::write(&imported, "= Capítulo importado").expect("the imported source can be written");
        let mut world = TypstationWorld::with_main(directory.path().to_path_buf(), "main.typ");

        let output = compile(
            &mut world,
            Request {
                id: 1,
                revision: 1,
                source: Some("#include \"chapter.typ\"".to_owned()),
                overlays: Vec::new(),
                reset_files: true,
                purpose: Purpose::Preview,
                export_options: ExportOptions::default(),
            },
        );

        assert_eq!(output.error_count, 0);
        assert_eq!(output.outline.len(), 1);
        assert_eq!(output.outline[0].title, "Capítulo importado");
        assert_eq!(
            output.outline[0].target,
            SourceTarget::ProjectFile(imported)
        );
    }

    #[test]
    fn unsaved_import_overrides_disk_and_reports_its_own_path() {
        let directory = tempfile::tempdir().expect("a temporary project can be created");
        let imported = directory.path().join("part.typ");
        fs::write(&imported, "#let title = [Saved]").expect("the imported source can be written");
        let mut world = TypstationWorld::with_main(directory.path().to_path_buf(), "main.typ");
        let main = "#import \"part.typ\": title\n#title".to_owned();

        let unsaved_failure = compile(
            &mut world,
            Request {
                id: 1,
                revision: 1,
                source: Some(main.clone()),
                overlays: vec![SourceOverlay {
                    path: imported.clone(),
                    text: "#let title =".to_owned(),
                }],
                reset_files: true,
                purpose: Purpose::Preview,
                export_options: ExportOptions::default(),
            },
        );

        assert!(unsaved_failure.error_count > 0);
        assert!(unsaved_failure.diagnostics.iter().any(|diagnostic| {
            diagnostic.target == DiagnosticTarget::ProjectFile(imported.clone())
        }));

        let disk_success = compile(
            &mut world,
            Request {
                id: 2,
                revision: 1,
                source: Some(main),
                overlays: Vec::new(),
                reset_files: false,
                purpose: Purpose::Preview,
                export_options: ExportOptions::default(),
            },
        );

        assert_eq!(disk_success.error_count, 0);
        assert_eq!(disk_success.pages.len(), 1);
        assert!(
            disk_success.pages[0]
                .regions
                .iter()
                .any(|region| { region.target == DiagnosticTarget::ProjectFile(imported.clone()) })
        );
    }

    #[test]
    fn closed_main_document_is_loaded_from_disk() {
        let directory = tempfile::tempdir().expect("a temporary project can be created");
        fs::write(directory.path().join("main.typ"), "Loaded from disk")
            .expect("the main source can be written");
        let mut world = TypstationWorld::with_main(directory.path().to_path_buf(), "main.typ");

        let output = compile(
            &mut world,
            Request {
                id: 1,
                revision: 1,
                source: None,
                overlays: Vec::new(),
                reset_files: true,
                purpose: Purpose::Preview,
                export_options: ExportOptions::default(),
            },
        );

        assert_eq!(output.error_count, 0);
        assert_eq!(output.page_count, 1);
        assert!(
            output.pages[0]
                .regions
                .iter()
                .any(|region| region.target == DiagnosticTarget::Main)
        );
    }
}

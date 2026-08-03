use std::{ops::Range, path::PathBuf};

use iced::{
    Subscription,
    futures::{
        SinkExt, Stream, StreamExt,
        channel::mpsc::{self, UnboundedSender},
    },
    stream,
};
use typst::diag::SourceDiagnostic;
use typst_iced_editor::Diagnostic;
use typst_layout::PagedDocument;
use typstation::world::{SourceOverlay, TypstationWorld};

pub type Sender = UnboundedSender<Request>;

#[derive(Debug)]
pub struct Request {
    pub id: u64,
    pub revision: u64,
    pub source: String,
    pub overlays: Vec<SourceOverlay>,
    pub reset_files: bool,
    pub purpose: Purpose,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Purpose {
    Preview,
    ExportPdf,
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
    pub pdf: Option<Vec<u8>>,
    pub diagnostics: Vec<ReportedDiagnostic>,
    pub page_count: usize,
    pub warning_count: usize,
    pub error_count: usize,
    pub summary: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RenderedPage {
    pub svg: Vec<u8>,
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiagnosticTarget {
    Main,
    ProjectFile(PathBuf),
}

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

        while let Some(request) = requests.next().await {
            let compiled = compile(&mut world, request);

            if output
                .send(Event::Finished {
                    config: config.clone(),
                    output: compiled,
                })
                .await
                .is_err()
            {
                break;
            }
        }
    })
}

fn compile(world: &mut TypstationWorld, request: Request) -> Output {
    if request.reset_files {
        world.reset_files();
    }

    world.set_source(&request.source);
    world.set_overlays(request.overlays);

    let result = typst::compile::<PagedDocument>(world);
    let mut diagnostics = Vec::new();
    let mut summary = None;

    for warning in &result.warnings {
        summary.get_or_insert_with(|| warning.message.to_string());

        if let Some(diagnostic) = editor_diagnostic(world, warning) {
            diagnostics.push(diagnostic);
        }
    }

    let warning_count = result.warnings.len();

    match result.output {
        Ok(document) if !document.pages().is_empty() => {
            let page_count = document.pages().len();
            let mut output = Output {
                id: request.id,
                revision: request.revision,
                purpose: request.purpose,
                pages: Vec::new(),
                pdf: None,
                diagnostics,
                page_count,
                warning_count,
                error_count: 0,
                summary,
            };

            match request.purpose {
                Purpose::Preview => {
                    let options = typst_svg::SvgOptions::default();
                    output.pages = document
                        .pages()
                        .iter()
                        .map(|page| RenderedPage {
                            svg: typst_svg::svg(page, &options).into_bytes(),
                            width: page.frame.width().to_pt() as f32,
                            height: page.frame.height().to_pt() as f32,
                        })
                        .collect();
                }
                Purpose::ExportPdf => {
                    match typst_pdf::pdf(&document, &typst_pdf::PdfOptions::default()) {
                        Ok(pdf) => output.pdf = Some(pdf),
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
            }

            output
        }
        Ok(_) => Output {
            id: request.id,
            revision: request.revision,
            purpose: request.purpose,
            pages: Vec::new(),
            pdf: None,
            diagnostics,
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
                id: request.id,
                revision: request.revision,
                purpose: request.purpose,
                pages: Vec::new(),
                pdf: None,
                diagnostics,
                page_count: 0,
                warning_count,
                error_count: errors.len(),
                summary,
            }
        }
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

    #[test]
    fn compiler_generates_multiple_pages_and_editor_diagnostics() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let mut world = TypstationWorld::new(root);

        let preview = compile(
            &mut world,
            Request {
                id: 1,
                revision: 4,
                source: "First page\n#pagebreak()\nSecond page".to_owned(),
                overlays: Vec::new(),
                reset_files: true,
                purpose: Purpose::Preview,
            },
        );

        assert_eq!(preview.id, 1);
        assert_eq!(preview.revision, 4);
        assert_eq!(preview.page_count, 2);
        assert_eq!(preview.error_count, 0);
        assert_eq!(preview.pages.len(), 2);
        assert!(preview.pdf.is_none());

        let pdf = compile(
            &mut world,
            Request {
                id: 2,
                revision: 4,
                source: "Exported page".to_owned(),
                overlays: Vec::new(),
                reset_files: false,
                purpose: Purpose::ExportPdf,
            },
        );

        assert_eq!(pdf.error_count, 0);
        assert!(pdf.pages.is_empty());
        assert!(
            pdf.pdf
                .as_deref()
                .is_some_and(|bytes| bytes.starts_with(b"%PDF-"))
        );

        let failure = compile(
            &mut world,
            Request {
                id: 3,
                revision: 5,
                source: "#let value =".to_owned(),
                overlays: Vec::new(),
                reset_files: false,
                purpose: Purpose::Preview,
            },
        );

        assert!(failure.error_count > 0);
        assert!(failure.pages.is_empty());
        assert!(!failure.diagnostics.is_empty());
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
                source: main.clone(),
                overlays: vec![SourceOverlay {
                    path: imported.clone(),
                    text: "#let title =".to_owned(),
                }],
                reset_files: true,
                purpose: Purpose::Preview,
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
                source: main,
                overlays: Vec::new(),
                reset_files: false,
                purpose: Purpose::Preview,
            },
        );

        assert_eq!(disk_success.error_count, 0);
        assert_eq!(disk_success.pages.len(), 1);
    }
}

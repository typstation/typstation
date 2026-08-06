use std::{
    collections::HashMap,
    fs, io,
    ops::Range,
    path::{Path, PathBuf},
};

use crate::search::{self, Options};

const MAX_SEARCH_FILE_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Match {
    pub path: PathBuf,
    pub range: Range<usize>,
    pub line: usize,
    pub column: usize,
    pub excerpt: String,
}

#[derive(Debug, Clone)]
pub struct SearchOutcome {
    pub revision: u64,
    pub root: PathBuf,
    pub result: Result<SearchReport, String>,
}

#[derive(Debug, Clone)]
pub struct SearchReport {
    pub matches: Vec<Match>,
    pub skipped_files: usize,
}

#[derive(Debug, Clone)]
pub struct ReplaceOutcome {
    pub revision: u64,
    pub replaced: usize,
    pub changed_files: usize,
    pub errors: Vec<String>,
}

pub async fn search(
    revision: u64,
    root: PathBuf,
    files: Vec<PathBuf>,
    overlays: Vec<(PathBuf, String)>,
    query: String,
    options: Options,
) -> SearchOutcome {
    let search_root = root.clone();
    let result = tokio::task::spawn_blocking(move || {
        search_sync(&search_root, files, overlays, &query, options)
    })
    .await
    .map_err(|error| format!("tarefa de busca interrompida: {error}"))
    .and_then(|result| result);

    SearchOutcome {
        revision,
        root,
        result,
    }
}

pub async fn replace_closed_files(
    revision: u64,
    files: Vec<PathBuf>,
    query: String,
    replacement: String,
    options: Options,
) -> ReplaceOutcome {
    match tokio::task::spawn_blocking(move || {
        replace_closed_files_sync(files, &query, &replacement, options)
    })
    .await
    {
        Ok((replaced, changed_files, errors)) => ReplaceOutcome {
            revision,
            replaced,
            changed_files,
            errors,
        },
        Err(error) => ReplaceOutcome {
            revision,
            replaced: 0,
            changed_files: 0,
            errors: vec![format!("tarefa de substituição interrompida: {error}")],
        },
    }
}

fn search_sync(
    root: &Path,
    files: Vec<PathBuf>,
    overlays: Vec<(PathBuf, String)>,
    query: &str,
    options: Options,
) -> Result<SearchReport, String> {
    if query.is_empty() {
        return Ok(SearchReport {
            matches: Vec::new(),
            skipped_files: 0,
        });
    }

    let overlays = overlays.into_iter().collect::<HashMap<_, _>>();
    let mut matches = Vec::new();
    let mut skipped_files = 0;

    for path in files {
        if !path.starts_with(root) {
            continue;
        }

        let text = if let Some(text) = overlays.get(&path) {
            text.clone()
        } else {
            match read_searchable_file(&path) {
                Ok(Some(text)) => text,
                Ok(None) | Err(_) => {
                    skipped_files += 1;
                    continue;
                }
            }
        };

        for range in search::find_matches(&text, query, options) {
            let (line, column) = line_column(&text, range.start);
            matches.push(Match {
                path: path.clone(),
                excerpt: line_excerpt(&text, range.clone()),
                range,
                line,
                column,
            });
        }
    }

    matches.sort_by(|left, right| {
        left.path
            .cmp(&right.path)
            .then_with(|| left.range.start.cmp(&right.range.start))
    });
    Ok(SearchReport {
        matches,
        skipped_files,
    })
}

fn replace_closed_files_sync(
    files: Vec<PathBuf>,
    query: &str,
    replacement: &str,
    options: Options,
) -> (usize, usize, Vec<String>) {
    let mut replaced = 0;
    let mut changed_files = 0;
    let mut errors = Vec::new();

    for path in files {
        let result = (|| -> Result<usize, String> {
            let Some(mut text) = read_searchable_file(&path)
                .map_err(|error| format!("{}: {error}", path.display()))?
            else {
                return Ok(0);
            };
            let matches = search::find_matches(&text, query, options);
            let count = matches.len();

            for range in matches.into_iter().rev() {
                text.replace_range(range, replacement);
            }
            if count > 0 {
                crate::atomic_write_file(&path, text.as_bytes())
                    .map_err(|error| format!("{}: {error}", path.display()))?;
            }
            Ok(count)
        })();

        match result {
            Ok(0) => {}
            Ok(count) => {
                replaced += count;
                changed_files += 1;
            }
            Err(error) => errors.push(error),
        }
    }

    (replaced, changed_files, errors)
}

fn read_searchable_file(path: &Path) -> io::Result<Option<String>> {
    if fs::metadata(path)?.len() > MAX_SEARCH_FILE_BYTES {
        return Ok(None);
    }

    match fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(error) if error.kind() == io::ErrorKind::InvalidData => Ok(None),
        Err(error) => Err(error),
    }
}

fn line_column(text: &str, offset: usize) -> (usize, usize) {
    let prefix = &text[..offset.min(text.len())];
    let line = prefix.bytes().filter(|byte| *byte == b'\n').count() + 1;
    let column = prefix
        .rsplit_once('\n')
        .map_or(prefix, |(_, line)| line)
        .chars()
        .count()
        + 1;
    (line, column)
}

fn line_excerpt(text: &str, range: Range<usize>) -> String {
    let start = text[..range.start].rfind('\n').map_or(0, |index| index + 1);
    let end = text[range.end..]
        .find('\n')
        .map_or(text.len(), |index| range.end + index);
    let line = text[start..end].trim();
    let mut excerpt = line.chars().take(160).collect::<String>();
    if line.chars().count() > 160 {
        excerpt.push_str("...");
    }
    excerpt
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_search_uses_overlays_and_reports_positions() {
        let directory = tempfile::tempdir().expect("a temporary project can be created");
        let path = directory.path().join("main.typ");
        fs::write(&path, "disco").expect("the source can be written");

        let report = search_sync(
            directory.path(),
            vec![path.clone()],
            vec![(path.clone(), "linha\nTexto local".to_owned())],
            "texto",
            Options::default(),
        )
        .expect("the project search should finish");

        assert_eq!(report.matches.len(), 1);
        assert_eq!(report.matches[0].path, path);
        assert_eq!((report.matches[0].line, report.matches[0].column), (2, 1));
        assert_eq!(report.matches[0].excerpt, "Texto local");
    }

    #[test]
    fn closed_file_replacement_revalidates_matches_before_writing() {
        let directory = tempfile::tempdir().expect("a temporary project can be created");
        let path = directory.path().join("main.typ");
        fs::write(&path, "um alvo e outro alvo").expect("the source can be written");

        let (replaced, changed, errors) =
            replace_closed_files_sync(vec![path.clone()], "alvo", "item", Options::default());

        assert_eq!((replaced, changed), (2, 1));
        assert!(errors.is_empty());
        assert_eq!(fs::read_to_string(path).unwrap(), "um item e outro item");
    }
}

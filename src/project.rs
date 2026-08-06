use std::{
    fs,
    path::{Path, PathBuf},
};

use rfd::AsyncFileDialog;

#[derive(Debug, Clone)]
pub struct ScanOutcome {
    pub root: PathBuf,
    pub snapshot: Result<ProjectSnapshot, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectSnapshot {
    pub entries: Vec<ProjectEntry>,
    pub typst_files: Vec<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectEntry {
    pub path: PathBuf,
    pub kind: EntryKind,
    pub children: Vec<ProjectEntry>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    Directory,
    TypstFile,
    File,
}

impl ProjectSnapshot {
    pub fn contains_path(&self, path: &Path) -> bool {
        contains_entry(&self.entries, path, None)
    }

    pub fn contains_directory(&self, path: &Path) -> bool {
        contains_entry(&self.entries, path, Some(EntryKind::Directory))
    }
}

#[derive(Debug, Clone)]
pub enum OperationOutcome {
    Cancelled,
    Created {
        path: PathBuf,
        kind: EntryKind,
    },
    Renamed {
        from: PathBuf,
        to: PathBuf,
        kind: EntryKind,
    },
    Deleted {
        path: PathBuf,
        kind: EntryKind,
    },
    Failed(String),
}

pub async fn create_file(root: PathBuf, directory: PathBuf) -> OperationOutcome {
    let directory = project_directory(&root, directory);
    let Some(file) = AsyncFileDialog::new()
        .add_filter("Documento Typst", &["typ"])
        .set_directory(&directory)
        .set_file_name("novo.typ")
        .set_title("Criar arquivo Typst no projeto")
        .save_file()
        .await
    else {
        return OperationOutcome::Cancelled;
    };
    let path = with_typst_extension(file.path());

    if !path.starts_with(&root) {
        return OperationOutcome::Failed(
            "o novo arquivo deve permanecer dentro da pasta do projeto".to_owned(),
        );
    }

    let destination = path.clone();
    let result = tokio::task::spawn_blocking(move || {
        let file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&destination)?;
        file.sync_all()
    })
    .await;

    match result {
        Ok(Ok(())) => OperationOutcome::Created {
            path,
            kind: EntryKind::TypstFile,
        },
        Ok(Err(error)) => OperationOutcome::Failed(format!("{}: {error}", path.display())),
        Err(error) => OperationOutcome::Failed(format!(
            "{}: tarefa de criação interrompida: {error}",
            path.display()
        )),
    }
}

pub async fn create_directory(root: PathBuf, directory: PathBuf) -> OperationOutcome {
    let directory = project_directory(&root, directory);
    let Some(folder) = AsyncFileDialog::new()
        .set_directory(&directory)
        .set_file_name("nova-pasta")
        .set_title("Criar pasta no projeto")
        .save_file()
        .await
    else {
        return OperationOutcome::Cancelled;
    };
    let path = folder.path().to_path_buf();

    if !path.starts_with(&root) {
        return OperationOutcome::Failed(
            "a nova pasta deve permanecer dentro da pasta do projeto".to_owned(),
        );
    }

    match tokio::fs::create_dir(&path).await {
        Ok(()) => OperationOutcome::Created {
            path,
            kind: EntryKind::Directory,
        },
        Err(error) => OperationOutcome::Failed(format!("{}: {error}", path.display())),
    }
}

pub async fn rename_entry(root: PathBuf, from: PathBuf, kind: EntryKind) -> OperationOutcome {
    if from == root || !from.starts_with(&root) {
        return OperationOutcome::Failed("a raiz do projeto não pode ser renomeada".to_owned());
    }

    let directory = from.parent().unwrap_or(&root);
    let entry_name = from
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| entry_fallback_name(kind).to_owned());
    let mut dialog = AsyncFileDialog::new()
        .set_directory(directory)
        .set_file_name(entry_name)
        .set_title(rename_dialog_title(kind));
    if kind == EntryKind::TypstFile {
        dialog = dialog.add_filter("Documento Typst", &["typ"]);
    }
    let Some(destination) = dialog.save_file().await else {
        return OperationOutcome::Cancelled;
    };
    let to = if kind == EntryKind::TypstFile {
        with_typst_extension(destination.path())
    } else {
        destination.path().to_path_buf()
    };

    if to == from {
        return OperationOutcome::Cancelled;
    }
    if !to.starts_with(&root) {
        return OperationOutcome::Failed(
            "o arquivo renomeado deve permanecer dentro da pasta do projeto".to_owned(),
        );
    }
    if tokio::fs::try_exists(&to).await.unwrap_or(false) {
        return OperationOutcome::Failed(format!("o destino já existe: {}", to.display()));
    }
    if kind == EntryKind::Directory && to.starts_with(&from) {
        return OperationOutcome::Failed(
            "uma pasta não pode ser movida para dentro dela mesma".to_owned(),
        );
    }

    match tokio::fs::rename(&from, &to).await {
        Ok(()) => OperationOutcome::Renamed { from, to, kind },
        Err(error) => {
            OperationOutcome::Failed(format!("{} -> {}: {error}", from.display(), to.display()))
        }
    }
}

pub async fn delete_entry(root: PathBuf, path: PathBuf, kind: EntryKind) -> OperationOutcome {
    if path == root || !path.starts_with(&root) {
        return OperationOutcome::Failed("a raiz do projeto não pode ser excluída".to_owned());
    }

    let result = if kind == EntryKind::Directory {
        tokio::fs::remove_dir_all(&path).await
    } else {
        tokio::fs::remove_file(&path).await
    };

    match result {
        Ok(()) => OperationOutcome::Deleted { path, kind },
        Err(error) => OperationOutcome::Failed(format!("{}: {error}", path.display())),
    }
}

fn project_directory(root: &Path, directory: PathBuf) -> PathBuf {
    if directory.starts_with(root) && directory.is_dir() {
        directory
    } else {
        root.to_path_buf()
    }
}

const fn entry_fallback_name(kind: EntryKind) -> &'static str {
    match kind {
        EntryKind::Directory => "pasta",
        EntryKind::TypstFile => "documento.typ",
        EntryKind::File => "arquivo",
    }
}

const fn rename_dialog_title(kind: EntryKind) -> &'static str {
    match kind {
        EntryKind::Directory => "Renomear pasta do projeto",
        EntryKind::TypstFile => "Renomear arquivo Typst",
        EntryKind::File => "Renomear arquivo do projeto",
    }
}

pub async fn scan(root: PathBuf) -> ScanOutcome {
    let scan_root = root.clone();
    let snapshot = tokio::task::spawn_blocking(move || scan_snapshot(&scan_root))
        .await
        .map_err(|error| format!("tarefa de varredura interrompida: {error}"))
        .and_then(|snapshot| snapshot);

    ScanOutcome { root, snapshot }
}

pub(crate) fn scan_snapshot(root: &Path) -> Result<ProjectSnapshot, String> {
    let mut typst_files = Vec::new();
    let entries = scan_directory(root, &mut typst_files)?;
    typst_files.sort();

    Ok(ProjectSnapshot {
        entries,
        typst_files,
    })
}

fn scan_directory(
    directory: &Path,
    typst_files: &mut Vec<PathBuf>,
) -> Result<Vec<ProjectEntry>, String> {
    let directory_entries =
        fs::read_dir(directory).map_err(|error| format!("{}: {error}", directory.display()))?;
    let mut entries = Vec::new();

    for entry in directory_entries {
        let entry = entry.map_err(|error| format!("{}: {error}", directory.display()))?;
        let path = entry.path();
        let file_type = entry
            .file_type()
            .map_err(|error| format!("{}: {error}", path.display()))?;

        if file_type.is_dir() {
            if should_ignore_directory(&path) {
                continue;
            }

            entries.push(ProjectEntry {
                children: scan_directory(&path, typst_files)?,
                path,
                kind: EntryKind::Directory,
            });
        } else if file_type.is_file() || file_type.is_symlink() {
            let kind = if path.extension().is_some_and(|extension| extension == "typ") {
                typst_files.push(path.clone());
                EntryKind::TypstFile
            } else {
                EntryKind::File
            };

            entries.push(ProjectEntry {
                path,
                kind,
                children: Vec::new(),
            });
        }
    }

    entries.sort_by(|left, right| {
        entry_rank(left.kind)
            .cmp(&entry_rank(right.kind))
            .then_with(|| {
                left.path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_lowercase()
                    .cmp(
                        &right
                            .path
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_lowercase(),
                    )
            })
            .then_with(|| left.path.cmp(&right.path))
    });

    Ok(entries)
}

fn should_ignore_directory(path: &Path) -> bool {
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    name == ".git" || name == "target" || name == "out" || name.starts_with('.')
}

const fn entry_rank(kind: EntryKind) -> u8 {
    match kind {
        EntryKind::Directory => 0,
        EntryKind::TypstFile | EntryKind::File => 1,
    }
}

fn contains_entry(entries: &[ProjectEntry], path: &Path, kind: Option<EntryKind>) -> bool {
    entries.iter().any(|entry| {
        (entry.path == path && kind.is_none_or(|kind| entry.kind == kind))
            || contains_entry(&entry.children, path, kind)
    })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scan_builds_a_sorted_tree_and_indexes_typst_files() {
        let directory = tempfile::tempdir().expect("a temporary project can be created");
        fs::create_dir_all(directory.path().join("chapters"))
            .expect("the source directory can be created");
        fs::create_dir_all(directory.path().join("target"))
            .expect("the build directory can be created");
        fs::create_dir_all(directory.path().join("out"))
            .expect("the output directory can be created");
        fs::create_dir_all(directory.path().join(".hidden"))
            .expect("the hidden directory can be created");
        fs::write(directory.path().join("main.typ"), "main").expect("main can be written");
        fs::write(directory.path().join("README.md"), "readme").expect("readme can be written");
        fs::write(directory.path().join("chapters/one.typ"), "one")
            .expect("chapter can be written");
        fs::write(directory.path().join("target/ignored.typ"), "ignored")
            .expect("build output can be written");
        fs::write(directory.path().join("out/ignored.typ"), "ignored")
            .expect("output can be written");
        fs::write(directory.path().join(".hidden/ignored.typ"), "ignored")
            .expect("hidden source can be written");

        let snapshot = scan_snapshot(directory.path()).expect("the project can be scanned");

        assert_eq!(
            snapshot.typst_files,
            vec![
                directory.path().join("chapters/one.typ"),
                directory.path().join("main.typ"),
            ]
        );
        assert_eq!(snapshot.entries.len(), 3);
        assert_eq!(snapshot.entries[0].path, directory.path().join("chapters"));
        assert_eq!(snapshot.entries[0].kind, EntryKind::Directory);
        assert_eq!(snapshot.entries[0].children.len(), 1);
        assert_eq!(snapshot.entries[1].path, directory.path().join("main.typ"));
        assert_eq!(snapshot.entries[1].kind, EntryKind::TypstFile);
        assert_eq!(snapshot.entries[2].path, directory.path().join("README.md"));
        assert_eq!(snapshot.entries[2].kind, EntryKind::File);
    }

    #[test]
    fn scan_keeps_empty_directories_and_does_not_follow_symlinks() {
        let directory = tempfile::tempdir().expect("a temporary project can be created");
        fs::create_dir(directory.path().join("empty")).expect("the empty directory can be created");

        #[cfg(unix)]
        std::os::unix::fs::symlink(directory.path(), directory.path().join("project-link"))
            .expect("the symlink can be created");

        let snapshot = scan_snapshot(directory.path()).expect("the project can be scanned");

        assert_eq!(snapshot.entries[0].path, directory.path().join("empty"));
        assert!(snapshot.entries[0].children.is_empty());
        #[cfg(unix)]
        assert_eq!(snapshot.entries[1].kind, EntryKind::File);
    }

    #[test]
    fn creation_directory_must_exist_inside_the_project() {
        let project = tempfile::tempdir().expect("a temporary project can be created");
        let outside = tempfile::tempdir().expect("an outside directory can be created");
        let nested = project.path().join("chapters");
        fs::create_dir(&nested).expect("the nested directory can be created");

        assert_eq!(project_directory(project.path(), nested.clone()), nested);
        assert_eq!(
            project_directory(project.path(), outside.path().to_path_buf()),
            project.path()
        );
        assert_eq!(
            project_directory(project.path(), project.path().join("missing")),
            project.path()
        );
    }
}

use std::{
    fs,
    path::{Path, PathBuf},
};

use rfd::{AsyncFileDialog, AsyncMessageDialog, MessageButtons, MessageDialogResult, MessageLevel};

#[derive(Debug, Clone)]
pub struct ScanOutcome {
    pub root: PathBuf,
    pub files: Result<Vec<PathBuf>, String>,
}

#[derive(Debug, Clone)]
pub enum OperationOutcome {
    Cancelled,
    Created(PathBuf),
    Renamed { from: PathBuf, to: PathBuf },
    Deleted(PathBuf),
    Failed(String),
}

pub async fn create_file(root: PathBuf) -> OperationOutcome {
    let Some(file) = AsyncFileDialog::new()
        .add_filter("Documento Typst", &["typ"])
        .set_directory(&root)
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
        Ok(Ok(())) => OperationOutcome::Created(path),
        Ok(Err(error)) => OperationOutcome::Failed(format!("{}: {error}", path.display())),
        Err(error) => OperationOutcome::Failed(format!(
            "{}: tarefa de criação interrompida: {error}",
            path.display()
        )),
    }
}

pub async fn rename_file(root: PathBuf, from: PathBuf) -> OperationOutcome {
    let directory = from.parent().unwrap_or(&root);
    let file_name = from
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "documento.typ".to_owned());
    let Some(file) = AsyncFileDialog::new()
        .add_filter("Documento Typst", &["typ"])
        .set_directory(directory)
        .set_file_name(file_name)
        .set_title("Renomear arquivo Typst")
        .save_file()
        .await
    else {
        return OperationOutcome::Cancelled;
    };
    let to = with_typst_extension(file.path());

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

    match tokio::fs::rename(&from, &to).await {
        Ok(()) => OperationOutcome::Renamed { from, to },
        Err(error) => {
            OperationOutcome::Failed(format!("{} -> {}: {error}", from.display(), to.display()))
        }
    }
}

pub async fn delete_file(path: PathBuf) -> OperationOutcome {
    let confirmed = AsyncMessageDialog::new()
        .set_level(MessageLevel::Warning)
        .set_title("Excluir arquivo do projeto")
        .set_description(format!(
            "Excluir permanentemente {}? Esta ação não pode ser desfeita.",
            path.display()
        ))
        .set_buttons(MessageButtons::YesNo)
        .show()
        .await;

    if confirmed != MessageDialogResult::Yes {
        return OperationOutcome::Cancelled;
    }

    match tokio::fs::remove_file(&path).await {
        Ok(()) => OperationOutcome::Deleted(path),
        Err(error) => OperationOutcome::Failed(format!("{}: {error}", path.display())),
    }
}

pub async fn scan(root: PathBuf) -> ScanOutcome {
    let scan_root = root.clone();
    let files = tokio::task::spawn_blocking(move || scan_files(&scan_root))
        .await
        .map_err(|error| format!("tarefa de varredura interrompida: {error}"))
        .and_then(|files| files);

    ScanOutcome { root, files }
}

pub(crate) fn scan_files(root: &Path) -> Result<Vec<PathBuf>, String> {
    let mut files = walkdir::WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| {
            if entry.depth() == 0 || !entry.file_type().is_dir() {
                return true;
            }

            let name = entry.file_name().to_string_lossy();
            name != ".git" && name != "target" && name != "out" && !name.starts_with('.')
        })
        .filter_map(|entry| match entry {
            Ok(entry)
                if entry.file_type().is_file()
                    && entry
                        .path()
                        .extension()
                        .is_some_and(|extension| extension == "typ") =>
            {
                Some(Ok(entry.into_path()))
            }
            Ok(_) => None,
            Err(error) => Some(Err(error.to_string())),
        })
        .collect::<Result<Vec<_>, String>>()?;

    files.sort();
    Ok(files)
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
    fn scan_lists_typst_files_and_ignores_generated_and_hidden_directories() {
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
        fs::write(directory.path().join("chapters/one.typ"), "one")
            .expect("chapter can be written");
        fs::write(directory.path().join("target/ignored.typ"), "ignored")
            .expect("build output can be written");
        fs::write(directory.path().join("out/ignored.typ"), "ignored")
            .expect("output can be written");
        fs::write(directory.path().join(".hidden/ignored.typ"), "ignored")
            .expect("hidden source can be written");

        let files = scan_files(directory.path()).expect("the project can be scanned");

        assert_eq!(
            files,
            vec![
                directory.path().join("chapters/one.typ"),
                directory.path().join("main.typ"),
            ]
        );
    }
}

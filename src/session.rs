use std::{
    env, fs, io,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::settings::Settings;

const SESSION_VERSION: u32 = 1;
const MAX_SESSION_BYTES: u64 = 128 * 1024 * 1024;
const MAX_DOCUMENTS: usize = 512;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Session {
    version: u32,
    pub workspace_root: PathBuf,
    pub active_document: usize,
    pub documents: Vec<Document>,
    pub pane_layout: PaneLayout,
    #[serde(default)]
    pub settings: Settings,
}

impl Session {
    pub fn new(
        workspace_root: PathBuf,
        active_document: usize,
        documents: Vec<Document>,
        pane_layout: PaneLayout,
        settings: Settings,
    ) -> Self {
        Self {
            version: SESSION_VERSION,
            workspace_root,
            active_document,
            documents,
            pane_layout,
            settings,
        }
    }

    fn validate(mut self) -> Result<Self, String> {
        if self.version != SESSION_VERSION {
            return Err(format!(
                "versão de sessão incompatível: {} (esperada: {SESSION_VERSION})",
                self.version
            ));
        }
        if self.documents.is_empty() {
            return Err("a sessão não contém documentos".to_owned());
        }
        if self.documents.len() > MAX_DOCUMENTS {
            return Err(format!(
                "a sessão excede o limite de {MAX_DOCUMENTS} documentos"
            ));
        }

        self.active_document = self.active_document.min(self.documents.len() - 1);
        self.pane_layout.ratio = if self.pane_layout.ratio.is_finite() {
            self.pane_layout.ratio.clamp(0.1, 0.9)
        } else {
            PaneLayout::default().ratio
        };
        self.settings = self.settings.validate();

        Ok(self)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Document {
    pub path: Option<PathBuf>,
    pub text: String,
    pub saved_text: Option<String>,
}

impl Document {
    pub fn blank() -> Self {
        Self {
            path: None,
            text: String::new(),
            saved_text: Some(String::new()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PaneLayout {
    pub axis: Axis,
    pub ratio: f32,
    pub first: Pane,
}

impl Default for PaneLayout {
    fn default() -> Self {
        Self {
            axis: Axis::Vertical,
            ratio: 0.5,
            first: Pane::Editor,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Axis {
    Horizontal,
    Vertical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Pane {
    Editor,
    Preview,
}

pub fn default_path() -> Option<PathBuf> {
    if let Some(path) = non_empty_path("TYPSTATION_SESSION_FILE") {
        return Some(path);
    }

    platform_state_root().map(|root| root.join("typstation").join("session.json"))
}

pub fn load(path: &Path) -> Result<Option<Session>, String> {
    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("{}: {error}", path.display())),
    };

    if metadata.len() > MAX_SESSION_BYTES {
        return Err(format!(
            "{} excede o limite de {} MiB",
            path.display(),
            MAX_SESSION_BYTES / 1024 / 1024
        ));
    }

    let bytes = fs::read(path).map_err(|error| format!("{}: {error}", path.display()))?;
    serde_json::from_slice::<Session>(&bytes)
        .map_err(|error| format!("{}: JSON inválido: {error}", path.display()))?
        .validate()
        .map(Some)
}

pub async fn save(path: PathBuf, session: Session) -> Result<(), String> {
    tokio::task::spawn_blocking(move || save_sync(&path, &session))
        .await
        .map_err(|error| format!("tarefa de sessão interrompida: {error}"))?
}

fn save_sync(path: &Path, session: &Session) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(session)
        .map_err(|error| format!("erro ao serializar sessão: {error}"))?;
    let directory = path
        .parent()
        .filter(|directory| !directory.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));

    fs::create_dir_all(directory).map_err(|error| format!("{}: {error}", directory.display()))?;

    crate::atomic_write_private_file(path, &bytes)
        .map_err(|error| format!("{}: {error}", path.display()))
}

fn non_empty_path(name: &str) -> Option<PathBuf> {
    env::var_os(name)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

#[cfg(target_os = "windows")]
fn platform_state_root() -> Option<PathBuf> {
    non_empty_path("LOCALAPPDATA").or_else(|| non_empty_path("APPDATA"))
}

#[cfg(target_os = "macos")]
fn platform_state_root() -> Option<PathBuf> {
    non_empty_path("HOME").map(|home| home.join("Library").join("Application Support"))
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn platform_state_root() -> Option<PathBuf> {
    non_empty_path("XDG_STATE_HOME")
        .filter(|path| path.is_absolute())
        .or_else(|| non_empty_path("HOME").map(|home| home.join(".local").join("state")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_session() -> Session {
        Session::new(
            PathBuf::from("/project"),
            1,
            vec![
                Document {
                    path: Some(PathBuf::from("/project/main.typ")),
                    text: "saved".to_owned(),
                    saved_text: Some("saved".to_owned()),
                },
                Document {
                    path: None,
                    text: "draft".to_owned(),
                    saved_text: None,
                },
            ],
            PaneLayout {
                axis: Axis::Horizontal,
                ratio: 0.7,
                first: Pane::Preview,
            },
            Settings {
                wrap_lines: true,
                preview_zoom: 125,
                ..Settings::default()
            },
        )
    }

    #[test]
    fn session_round_trip_preserves_documents_and_layout() {
        let directory = tempfile::tempdir().expect("a temporary directory can be created");
        let path = directory.path().join("session.json");
        let expected = sample_session();

        save_sync(&path, &expected).expect("the session can be saved");
        let loaded = load(&path)
            .expect("the session can be loaded")
            .expect("the session exists");

        assert_eq!(loaded, expected);
        assert!(loaded.settings.wrap_lines);
        assert_eq!(loaded.settings.preview_zoom, 125);
    }

    #[test]
    fn invalid_pane_ratio_is_normalized_during_loading() {
        let directory = tempfile::tempdir().expect("a temporary directory can be created");
        let path = directory.path().join("session.json");
        let mut stored = sample_session();
        stored.pane_layout.ratio = 4.0;
        let bytes = serde_json::to_vec(&stored).expect("the session can be serialized");
        fs::write(&path, bytes).expect("the invalid session can be written");

        let loaded = load(&path)
            .expect("the session can be loaded")
            .expect("the session exists");

        assert_eq!(loaded.pane_layout.ratio, 0.9);
    }

    #[test]
    fn sessions_written_before_settings_use_defaults() {
        let directory = tempfile::tempdir().expect("a temporary directory can be created");
        let path = directory.path().join("session.json");
        let mut stored = serde_json::to_value(sample_session())
            .expect("the old session can be represented as JSON");
        stored
            .as_object_mut()
            .expect("the session is a JSON object")
            .remove("settings");
        fs::write(
            &path,
            serde_json::to_vec(&stored).expect("the old session can be serialized"),
        )
        .expect("the old session can be written");

        let loaded = load(&path)
            .expect("the old session can be loaded")
            .expect("the old session exists");

        assert_eq!(loaded.settings, Settings::default());
    }

    #[cfg(unix)]
    #[test]
    fn session_file_is_private() {
        use std::os::unix::fs::PermissionsExt;

        let directory = tempfile::tempdir().expect("a temporary directory can be created");
        let path = directory.path().join("session.json");

        save_sync(&path, &sample_session()).expect("the session can be saved");

        assert_eq!(
            fs::metadata(path)
                .expect("the session metadata can be read")
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
}

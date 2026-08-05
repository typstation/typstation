use std::path::{Component, Path, PathBuf};

use iced::{
    Subscription,
    futures::{SinkExt, Stream, StreamExt, channel::mpsc},
    stream,
};
use notify::{EventKind, RecursiveMode, Watcher, event::ModifyKind};

#[derive(Debug, Clone)]
pub enum Event {
    Ready { root: PathBuf },
    Changed { root: PathBuf, paths: Vec<PathBuf> },
    Failed { root: PathBuf, error: String },
}

pub fn subscription(root: PathBuf) -> Subscription<Event> {
    Subscription::run_with(root, |root| worker(root.as_path()))
}

fn worker(root: &Path) -> impl Stream<Item = Event> + use<> {
    let root = root.to_path_buf();

    stream::channel(32, async move |mut output| {
        let (sender, mut events) = mpsc::unbounded::<notify::Result<notify::Event>>();
        let mut watcher = match notify::recommended_watcher(move |event| {
            let _ = sender.unbounded_send(event);
        }) {
            Ok(watcher) => watcher,
            Err(error) => {
                let _ = output
                    .send(Event::Failed {
                        root: root.clone(),
                        error: error.to_string(),
                    })
                    .await;
                std::future::pending::<notify::RecommendedWatcher>().await
            }
        };

        if let Err(error) = watcher.watch(&root, RecursiveMode::Recursive) {
            let _ = output
                .send(Event::Failed {
                    root,
                    error: error.to_string(),
                })
                .await;
            std::future::pending::<()>().await;
            return;
        }

        if output
            .send(Event::Ready { root: root.clone() })
            .await
            .is_err()
        {
            return;
        }

        while let Some(event) = events.next().await {
            let message = match event {
                Ok(event) => {
                    if !changes_project(&event.kind) {
                        continue;
                    }

                    let paths = event
                        .paths
                        .into_iter()
                        .filter(|path| is_relevant(&root, path))
                        .collect::<Vec<_>>();
                    if paths.is_empty() {
                        continue;
                    }

                    Event::Changed {
                        root: root.clone(),
                        paths,
                    }
                }
                Err(error) => Event::Failed {
                    root: root.clone(),
                    error: error.to_string(),
                },
            };

            if output.send(message).await.is_err() {
                break;
            }
        }
    })
}

fn changes_project(kind: &EventKind) -> bool {
    matches!(
        kind,
        EventKind::Any
            | EventKind::Create(_)
            | EventKind::Modify(
                ModifyKind::Any | ModifyKind::Data(_) | ModifyKind::Name(_) | ModifyKind::Other
            )
            | EventKind::Remove(_)
    )
}

fn is_relevant(root: &Path, path: &Path) -> bool {
    let relative = path.strip_prefix(root).unwrap_or(path);

    !relative.components().any(|component| {
        let Component::Normal(name) = component else {
            return false;
        };
        let name = name.to_string_lossy();
        name == ".git"
            || name == ".local"
            || name == "target"
            || name == "out"
            || name.starts_with('.')
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use notify::event::{AccessKind, CreateKind, DataChange, MetadataKind, RemoveKind};

    #[test]
    fn only_project_mutations_trigger_refreshes() {
        assert!(changes_project(&EventKind::Any));
        assert!(changes_project(&EventKind::Create(CreateKind::File)));
        assert!(changes_project(&EventKind::Modify(ModifyKind::Data(
            DataChange::Content
        ))));
        assert!(changes_project(&EventKind::Remove(RemoveKind::File)));

        assert!(!changes_project(&EventKind::Access(AccessKind::Read)));
        assert!(!changes_project(&EventKind::Modify(ModifyKind::Metadata(
            MetadataKind::AccessTime
        ))));
        assert!(!changes_project(&EventKind::Other));
    }

    #[test]
    fn build_and_hidden_directories_do_not_trigger_refreshes() {
        let root = Path::new("/project");

        assert!(is_relevant(root, Path::new("/project/chapters/one.typ")));
        assert!(!is_relevant(root, Path::new("/project/target/debug/app")));
        assert!(!is_relevant(root, Path::new("/project/.git/index")));
    }
}

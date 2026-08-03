use std::path::{Component, Path, PathBuf};

use iced::{
    Subscription,
    futures::{SinkExt, Stream, StreamExt, channel::mpsc},
    stream,
};
use notify::{RecursiveMode, Watcher};

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

    #[test]
    fn build_and_hidden_directories_do_not_trigger_refreshes() {
        let root = Path::new("/project");

        assert!(is_relevant(root, Path::new("/project/chapters/one.typ")));
        assert!(!is_relevant(root, Path::new("/project/target/debug/app")));
        assert!(!is_relevant(root, Path::new("/project/.git/index")));
    }
}

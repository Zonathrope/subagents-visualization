//! Watches `~/.claude` and keeps the [`GraphStore`] in sync, emitting a [`GraphEvent`] per change.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use notify::{RecursiveMode, Watcher};

use crate::parser::{self, TranscriptTail};
use crate::state::GraphStore;
use crate::types::GraphEvent;

/// Also the cadence for retrying directories that don't exist yet.
const POLL_INTERVAL: Duration = Duration::from_millis(1500);
/// Transcripts touched within this window are re-checked every tick, as a backstop for
/// file-change notifications that Windows delivers late or not at all for appends.
const HOT_WINDOW: Duration = Duration::from_secs(10 * 60);

pub struct ClaudeDirs {
    pub sessions: PathBuf,
    pub jobs: PathBuf,
    pub projects: PathBuf,
}

impl ClaudeDirs {
    pub fn new(root: &Path) -> Self {
        Self {
            sessions: root.join("sessions"),
            jobs: root.join("jobs"),
            projects: root.join("projects"),
        }
    }

    fn classify(&self, path: &Path) -> Option<Target> {
        if let Ok(rel) = path.strip_prefix(&self.sessions) {
            return (rel.components().count() == 1 && has_ext(path, "json"))
                .then(|| Target::Session(path.to_owned()));
        }
        if let Ok(rel) = path.strip_prefix(&self.jobs) {
            let mut parts = rel.components();
            let id = parts.next()?.as_os_str();
            return match parts.next().map(|c| c.as_os_str().to_str()) {
                None | Some(Some("state.json" | "timeline.jsonl")) => {
                    Some(Target::Job(self.jobs.join(id)))
                }
                _ => None,
            };
        }
        if let Ok(rel) = path.strip_prefix(&self.projects) {
            // projects/<project>/<session>.jsonl only; subagents/ subfolders are not tracked.
            return (rel.components().count() == 2 && has_ext(path, "jsonl"))
                .then(|| Target::Transcript(path.to_owned()));
        }
        None
    }

    /// Everything currently on disk under `dir`, as watch targets.
    fn scan(&self, dir: &Path) -> Vec<Target> {
        let children = |d: &Path| -> Vec<PathBuf> {
            std::fs::read_dir(d)
                .map(|it| it.filter_map(|e| e.ok().map(|e| e.path())).collect())
                .unwrap_or_default()
        };
        let paths = if dir == self.projects {
            children(dir).iter().flat_map(|p| children(p)).collect()
        } else {
            children(dir)
        };
        paths.iter().filter_map(|p| self.classify(p)).collect()
    }
}

fn has_ext(path: &Path, ext: &str) -> bool {
    path.extension().is_some_and(|e| e == ext)
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum Target {
    Session(PathBuf),
    /// The job's directory, `jobs/<id>`.
    Job(PathBuf),
    Transcript(PathBuf),
}

pub fn spawn(
    dirs: ClaudeDirs,
    store: Arc<Mutex<GraphStore>>,
    emit: impl Fn(GraphEvent) + Send + 'static,
) {
    thread::spawn(move || Worker { dirs, store, emit, tail: TranscriptTail::default() }.run());
}

struct Worker<E> {
    dirs: ClaudeDirs,
    store: Arc<Mutex<GraphStore>>,
    emit: E,
    tail: TranscriptTail,
}

impl<E: Fn(GraphEvent)> Worker<E> {
    fn run(mut self) {
        let (tx, rx) = mpsc::channel::<PathBuf>();
        let mut watcher = match notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
            if let Ok(event) = res {
                for path in event.paths {
                    let _ = tx.send(path);
                }
            }
        }) {
            Ok(w) => w,
            Err(e) => {
                log::error!("cannot create file watcher: {e}");
                return;
            }
        };

        let mut unwatched = vec![
            self.dirs.sessions.clone(),
            self.dirs.jobs.clone(),
            self.dirs.projects.clone(),
        ];
        let mut hot: HashMap<PathBuf, Instant> = HashMap::new();

        loop {
            let mut pending: HashSet<Target> = HashSet::new();

            unwatched.retain(|dir| match watcher.watch(dir, RecursiveMode::Recursive) {
                Ok(()) => {
                    log::info!("watching {}", dir.display());
                    pending.extend(self.dirs.scan(dir));
                    false
                }
                Err(_) => true,
            });

            // Block until something happens or the poll interval passes, then drain the burst.
            match rx.recv_timeout(POLL_INTERVAL) {
                Ok(path) => pending.extend(self.dirs.classify(&path)),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return,
            }
            pending.extend(rx.try_iter().filter_map(|p| self.dirs.classify(&p)));

            let now = Instant::now();
            for target in &pending {
                if let Target::Transcript(p) = target {
                    hot.insert(p.clone(), now);
                }
            }
            hot.retain(|_, seen| now.duration_since(*seen) < HOT_WINDOW);
            pending.extend(hot.keys().cloned().map(Target::Transcript));

            for target in pending {
                if self.handle(target.clone()) {
                    if let Target::Transcript(p) = target {
                        hot.insert(p, now);
                    }
                }
            }
        }
    }

    /// Applies one target to the store. Returns true if anything changed.
    fn handle(&mut self, target: Target) -> bool {
        let mut store = self.store.lock().unwrap();
        let mut events = Vec::new();

        match target {
            Target::Session(path) => match parser::parse_session(&path) {
                Some(info) => {
                    store.update_session(info.clone());
                    events.push(GraphEvent::SessionUpdate { data: info });
                }
                None if !path.exists() => {
                    let pid = path.file_stem().unwrap_or_default().to_string_lossy();
                    if let Some(id) = store.remove_session_by_pid(&pid) {
                        events.push(GraphEvent::SessionRemoved { id });
                    }
                }
                None => {} // half-written file; the next change event will retry
            },
            Target::Job(dir) => match parser::parse_job(&dir.join("state.json")) {
                Some(job) => {
                    store.update_job(job.clone());
                    events.push(GraphEvent::JobUpdate { data: job });
                }
                None if !dir.join("state.json").exists() => {
                    let id = dir.file_name().unwrap_or_default().to_string_lossy().into_owned();
                    if store.remove_job(&id) {
                        events.push(GraphEvent::JobRemoved { id });
                    }
                }
                None => {}
            },
            Target::Transcript(path) => {
                let (spawned, completed) = parser::parse_agent_events(&self.tail.read_new_lines(&path));
                for agent in spawned {
                    events.push(GraphEvent::AgentUpdate { data: store.update_agent(agent) });
                }
                for c in completed {
                    if let Some(agent) = store.complete_agent(&c.id, c.failed, c.at) {
                        events.push(GraphEvent::AgentUpdate { data: agent });
                    }
                }
            }
        }

        drop(store);
        let changed = !events.is_empty();
        for event in events {
            (self.emit)(event);
        }
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_paths() {
        let d = ClaudeDirs::new(Path::new("/c"));
        let t = |p: &str| d.classify(Path::new(p));

        assert_eq!(t("/c/sessions/1.json"), Some(Target::Session("/c/sessions/1.json".into())));
        assert_eq!(t("/c/sessions/1.abc.key"), None);
        assert_eq!(t("/c/jobs/ab/state.json"), Some(Target::Job("/c/jobs/ab".into())));
        assert_eq!(t("/c/jobs/ab/timeline.jsonl"), Some(Target::Job("/c/jobs/ab".into())));
        assert_eq!(t("/c/jobs/ab"), Some(Target::Job("/c/jobs/ab".into())));
        assert_eq!(t("/c/jobs/ab/other.txt"), None);
        assert_eq!(
            t("/c/projects/p/s.jsonl"),
            Some(Target::Transcript("/c/projects/p/s.jsonl".into()))
        );
        assert_eq!(t("/c/projects/p/s/subagents/a.jsonl"), None);
        assert_eq!(t("/c/other/x.json"), None);
    }
}

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
use crate::stats::StatsTracker;
use crate::types::GraphEvent;

/// Also the cadence for retrying directories that don't exist yet.
const POLL_INTERVAL: Duration = Duration::from_millis(1500);
/// Transcripts touched within this window are re-checked every tick, as a backstop for
/// file-change notifications that Windows delivers late or not at all for appends.
const HOT_WINDOW: Duration = Duration::from_secs(10 * 60);
/// The startup scan skips transcripts untouched for longer than this; older sessions
/// would only clutter the graph (and the History view) with agents nobody is watching.
const HISTORY_WINDOW: Duration = Duration::from_secs(24 * 60 * 60);

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
            let parts: Vec<_> = rel.components().map(|c| c.as_os_str().to_string_lossy()).collect();
            return match parts.as_slice() {
                // projects/<project>/<sessionId>.jsonl
                [_, file] if file.ends_with(".jsonl") => Some(Target::Transcript(path.to_owned())),
                // projects/<project>/<sessionId>/subagents/agent-<id>.{jsonl,meta.json}
                [_, _, dir, file] if dir == "subagents" && file.starts_with("agent-") => {
                    if let Some(stem) = file.strip_suffix(".meta.json") {
                        Some(Target::SubTranscript(path.with_file_name(format!("{stem}.jsonl"))))
                    } else if file.ends_with(".jsonl") {
                        Some(Target::SubTranscript(path.to_owned()))
                    } else {
                        None
                    }
                }
                _ => None,
            };
        }
        None
    }

    /// Everything currently on disk under `dir`, as watch targets.
    fn scan(&self, dir: &Path) -> HashSet<Target> {
        let children = |d: &Path| -> Vec<PathBuf> {
            std::fs::read_dir(d)
                .map(|it| it.filter_map(|e| e.ok().map(|e| e.path())).collect())
                .unwrap_or_default()
        };
        let paths: Vec<PathBuf> = if dir == self.projects {
            children(dir)
                .iter()
                .flat_map(|project| children(project))
                .flat_map(|entry| {
                    let mut found = children(&entry.join("subagents"));
                    found.push(entry);
                    found
                })
                .collect()
        } else {
            children(dir)
        };
        paths
            .iter()
            .filter_map(|p| self.classify(p))
            .filter(|t| t.transcript_path().is_none_or(recently_modified))
            .collect()
    }

    /// Finds `projects/<project>/<session_id>.jsonl` for a session whose transcript the
    /// scan skipped (e.g. a live session that has been idle longer than the history window).
    fn find_transcript(&self, session_id: &str) -> Option<PathBuf> {
        std::fs::read_dir(&self.projects)
            .ok()?
            .filter_map(|e| e.ok())
            .map(|e| e.path().join(format!("{session_id}.jsonl")))
            .find(|p| p.is_file())
    }
}

fn recently_modified(path: &Path) -> bool {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.elapsed().ok())
        .is_some_and(|age| age < HISTORY_WINDOW)
}

fn has_ext(path: &Path, ext: &str) -> bool {
    path.extension().is_some_and(|e| e == ext)
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum Target {
    Session(PathBuf),
    /// The job's directory, `jobs/<id>`.
    Job(PathBuf),
    /// A session's main transcript.
    Transcript(PathBuf),
    /// A subagent's transcript; its `.meta.json` sibling links it to the spawning tool call.
    SubTranscript(PathBuf),
}

impl Target {
    fn transcript_path(&self) -> Option<&Path> {
        match self {
            Target::Transcript(p) | Target::SubTranscript(p) => Some(p),
            _ => None,
        }
    }
}

pub fn spawn(
    dirs: ClaudeDirs,
    store: Arc<Mutex<GraphStore>>,
    emit: impl Fn(GraphEvent) + Send + 'static,
) {
    thread::spawn(move || {
        Worker {
            dirs,
            store,
            emit,
            tail: TranscriptTail::default(),
            sub_links: HashMap::new(),
            trackers: HashMap::new(),
        }
        .run()
    });
}

struct Worker<E> {
    dirs: ClaudeDirs,
    store: Arc<Mutex<GraphStore>>,
    emit: E,
    tail: TranscriptTail,
    /// Subagent transcript → spawning `Agent` tool_use id, cached once the meta file is read.
    sub_links: HashMap<PathBuf, String>,
    trackers: HashMap<PathBuf, StatsTracker>,
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
        let mut hot: HashMap<Target, Instant> = HashMap::new();

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
            for target in pending.iter().filter(|t| t.transcript_path().is_some()) {
                hot.insert(target.clone(), now);
            }
            hot.retain(|_, seen| now.duration_since(*seen) < HOT_WINDOW);
            pending.extend(hot.keys().cloned());

            for target in pending {
                if self.handle(&target) && target.transcript_path().is_some() {
                    hot.insert(target, now);
                }
            }
        }
    }

    /// Applies one target to the store. Returns true if anything changed.
    fn handle(&mut self, target: &Target) -> bool {
        let mut events = Vec::new();
        let mut new_lines = false;

        match target {
            Target::Session(path) => {
                let info = parser::parse_session(path);
                let unlinked = info.as_ref().filter(|i| !self.store.lock().unwrap().has_session_transcript(&i.id));
                let found = unlinked.and_then(|i| self.dirs.find_transcript(&i.id));
                let mut store = self.store.lock().unwrap();
                match info {
                    Some(info) => {
                        if let Some(transcript) = found {
                            store.link_session_transcript(&info.id, &transcript.to_string_lossy());
                        }
                        let data = store.update_session(info);
                        events.push(GraphEvent::SessionUpdate { data });
                    }
                    None if !path.exists() => {
                        let pid = path.file_stem().unwrap_or_default().to_string_lossy();
                        if let Some(id) = store.remove_session_by_pid(&pid) {
                            events.push(GraphEvent::SessionRemoved { id });
                        }
                    }
                    None => {} // half-written file; the next change event will retry
                }
            }
            Target::Job(dir) => {
                let mut store = self.store.lock().unwrap();
                match parser::parse_job(&dir.join("state.json")) {
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
                }
            }
            Target::Transcript(path) => {
                let session_id = path.file_stem().unwrap_or_default().to_string_lossy();
                let path_str = path.to_string_lossy();
                let lines = self.tail.read_new_lines(path);
                let mut store = self.store.lock().unwrap();
                if let Some(data) = store.link_session_transcript(&session_id, &path_str) {
                    events.push(GraphEvent::SessionUpdate { data });
                }
                new_lines = !lines.is_empty();
                if new_lines {
                    let (cwd, last) = parser::transcript_activity(&lines);
                    let mut summary = store.update_summary(&session_id, &path_str, cwd, last);
                    let tracker = self.trackers.entry(path.clone()).or_default();
                    tracker.feed(&lines);
                    summary = store.set_session_stats(&session_id, tracker.snapshot()).or(summary);
                    if let Some(data) = summary {
                        events.push(GraphEvent::SessionSummary { data });
                    }
                }
                apply_agent_events(&mut store, &lines, None, &mut events);
            }
            Target::SubTranscript(path) => {
                let parent = self.sub_link(path);
                let path_str = path.to_string_lossy();
                let lines = self.tail.read_new_lines(path);
                let mut store = self.store.lock().unwrap();
                if let Some(id) = &parent {
                    if let Some(data) = store.link_agent_transcript(id, &path_str) {
                        events.push(GraphEvent::AgentUpdate { data });
                    }
                }
                new_lines = !lines.is_empty();
                if new_lines {
                    let tracker = self.trackers.entry(path.clone()).or_default();
                    tracker.feed(&lines);
                    if let Some(id) = &parent {
                        if let Some(data) = store.set_agent_stats(id, tracker.snapshot()) {
                            events.push(GraphEvent::AgentUpdate { data });
                        }
                    }
                }
                apply_agent_events(&mut store, &lines, parent, &mut events);
            }
        }

        if new_lines {
            if let Some(path) = target.transcript_path() {
                events.push(GraphEvent::TranscriptChanged { path: path.to_string_lossy().into_owned() });
            }
        }
        let changed = !events.is_empty();
        for event in events {
            (self.emit)(event);
        }
        changed
    }

    fn sub_link(&mut self, transcript: &Path) -> Option<String> {
        if let Some(id) = self.sub_links.get(transcript) {
            return Some(id.clone());
        }
        let stem = transcript.file_stem()?.to_string_lossy();
        let id = parser::parse_subagent_meta(&transcript.with_file_name(format!("{stem}.meta.json")))?;
        self.sub_links.insert(transcript.to_owned(), id.clone());
        Some(id)
    }
}

/// Folds agent spawns/completions from transcript lines into the store.
/// `parent` is the spawning agent when the lines come from a subagent's own transcript.
fn apply_agent_events(
    store: &mut GraphStore,
    lines: &[serde_json::Value],
    parent: Option<String>,
    events: &mut Vec<GraphEvent>,
) {
    let (spawned, completed) = parser::parse_agent_events(lines);
    for mut agent in spawned {
        agent.parent_id = parent.clone();
        events.push(GraphEvent::AgentUpdate { data: store.update_agent(agent) });
    }
    for c in completed {
        if let Some(agent) = store.complete_agent(&c.id, c.failed, c.at) {
            events.push(GraphEvent::AgentUpdate { data: agent });
        }
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
        let sub = Some(Target::SubTranscript("/c/projects/p/s/subagents/agent-a1.jsonl".into()));
        assert_eq!(t("/c/projects/p/s/subagents/agent-a1.jsonl"), sub);
        assert_eq!(t("/c/projects/p/s/subagents/agent-a1.meta.json"), sub);
        assert_eq!(t("/c/projects/p/s/other/agent-a1.jsonl"), None);
        assert_eq!(t("/c/projects/p/s/tool-results/x.txt"), None);
        assert_eq!(t("/c/other/x.json"), None);
    }
}

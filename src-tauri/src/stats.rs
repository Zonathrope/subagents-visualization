//! Per-transcript statistics: token usage, last activity and edited files.
//!
//! Fed incrementally with the records the watcher tails, so it must cope with one API
//! response being split across several lines (and several reads).

use std::collections::BTreeSet;

use serde::Serialize;
use serde_json::Value;

/// Token counts, summed over responses (each response counted once).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenTotals {
    pub input: u64,
    pub output: u64,
    pub cache_write_5m: u64,
    pub cache_write_1h: u64,
    pub cache_read: u64,
}

impl TokenTotals {
    fn from_usage(usage: &Value) -> Self {
        let n = |v: Option<&Value>| v.and_then(Value::as_u64).unwrap_or(0);
        let creation = n(usage.get("cache_creation_input_tokens"));
        let split_1h = n(usage.pointer("/cache_creation/ephemeral_1h_input_tokens"));
        let split_5m = n(usage.pointer("/cache_creation/ephemeral_5m_input_tokens"));
        // Older records only have the combined figure; treat it as 5-minute writes.
        let (cw5, cw1) = if split_5m + split_1h > 0 { (split_5m, split_1h) } else { (creation, 0) };
        Self {
            input: n(usage.get("input_tokens")),
            output: n(usage.get("output_tokens")),
            cache_write_5m: cw5,
            cache_write_1h: cw1,
            cache_read: n(usage.get("cache_read_input_tokens")),
        }
    }

    fn add(&mut self, o: &Self) {
        self.input += o.input;
        self.output += o.output;
        self.cache_write_5m += o.cache_write_5m;
        self.cache_write_1h += o.cache_write_1h;
        self.cache_read += o.cache_read;
    }

    /// Lines of one response repeat the usage; streamed snapshots carry partial output.
    fn max(&mut self, o: &Self) {
        self.input = self.input.max(o.input);
        self.output = self.output.max(o.output);
        self.cache_write_5m = self.cache_write_5m.max(o.cache_write_5m);
        self.cache_write_1h = self.cache_write_1h.max(o.cache_write_1h);
        self.cache_read = self.cache_read.max(o.cache_read);
    }

    fn context(&self) -> u64 {
        self.input + self.cache_write_5m + self.cache_write_1h + self.cache_read
    }
}

/// What the UI shows about an agent's (or session's) transcript.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptStats {
    pub tokens: TokenTotals,
    /// Context size of the latest response (input + cache reads and writes).
    pub context_tokens: u64,
    /// Model of the latest response.
    pub model: Option<String>,
    /// Newest record timestamp (ms).
    pub last_activity: i64,
    /// Files changed by Edit/Write/MultiEdit/NotebookEdit, relative to the repo root.
    pub edited_files: Vec<String>,
}

#[derive(Default)]
pub struct StatsTracker {
    finished: TokenTotals,
    current_id: Option<String>,
    current: TokenTotals,
    context: u64,
    model: Option<String>,
    last_activity: i64,
    edited: BTreeSet<String>,
}

const EDIT_TOOLS: &[&str] = &["Edit", "Write", "MultiEdit", "NotebookEdit"];

impl StatsTracker {
    pub fn feed(&mut self, records: &[Value]) {
        for rec in records {
            if let Some(ts) = rec
                .get("timestamp")
                .and_then(Value::as_str)
                .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            {
                self.last_activity = self.last_activity.max(ts.timestamp_millis());
            }
            if rec.get("type").and_then(Value::as_str) != Some("assistant") {
                continue;
            }
            let Some(msg) = rec.get("message") else { continue };

            if let Some(usage) = msg.get("usage") {
                let t = TokenTotals::from_usage(usage);
                let id = msg.get("id").and_then(Value::as_str).map(str::to_owned);
                if id.is_some() && id == self.current_id {
                    self.current.max(&t);
                } else {
                    self.finished.add(&self.current);
                    self.current = t;
                    self.current_id = id;
                }
                self.context = self.current.context();
                if let Some(model) = msg.get("model").and_then(Value::as_str) {
                    self.model = Some(model.to_owned());
                }
            }

            let cwd = rec.get("cwd").and_then(Value::as_str);
            for block in msg.get("content").and_then(Value::as_array).into_iter().flatten() {
                let is_edit = block.get("type").and_then(Value::as_str) == Some("tool_use")
                    && block.get("name").and_then(Value::as_str).is_some_and(|n| EDIT_TOOLS.contains(&n));
                if !is_edit {
                    continue;
                }
                let input = block.get("input").unwrap_or(&Value::Null);
                let path = input
                    .get("file_path")
                    .or_else(|| input.get("notebook_path"))
                    .and_then(Value::as_str);
                if let Some(path) = path {
                    self.edited.insert(repo_relative(path, cwd));
                }
            }
        }
    }

    pub fn snapshot(&self) -> TranscriptStats {
        let mut tokens = self.finished;
        tokens.add(&self.current);
        TranscriptStats {
            tokens,
            context_tokens: self.context,
            model: self.model.clone(),
            last_activity: self.last_activity,
            edited_files: self.edited.iter().cloned().collect(),
        }
    }
}

/// A file path made comparable across worktrees: `<repo>/.claude/worktrees/<name>/src/a.rs`
/// and `<repo>/src/a.rs` both become `src/a.rs`. Case-folded on Windows.
///
/// Tried in order: the worktree layout (works even after the worktree is deleted), the
/// enclosing git checkout (a worktree has a `.git` file, the main repo a `.git` dir), then
/// the record's working directory. The cwd alone is unreliable: a session that `cd`s into a
/// subfolder would otherwise report the same file under two different relative paths.
pub fn repo_relative(path: &str, cwd: Option<&str>) -> String {
    let fold = |s: &str| if cfg!(windows) { s.to_lowercase() } else { s.to_owned() };
    let p = path.replace('\\', "/");
    const WORKTREES: &str = "/.claude/worktrees/";
    if let Some(i) = p.find(WORKTREES) {
        let rest = &p[i + WORKTREES.len()..];
        if let Some(j) = rest.find('/') {
            return fold(&rest[j + 1..]);
        }
    }
    if let Some(root) = git_root(std::path::Path::new(path)) {
        let root = fold(root.to_string_lossy().replace('\\', "/").trim_end_matches('/')) + "/";
        if let Some(rel) = fold(&p).strip_prefix(&root) {
            return rel.to_owned();
        }
    }
    if let Some(cwd) = cwd {
        let root = fold(cwd.replace('\\', "/").trim_end_matches('/')) + "/";
        let folded = fold(&p);
        if let Some(rel) = folded.strip_prefix(&root) {
            return rel.to_owned();
        }
    }
    fold(&p)
}

/// Nearest ancestor directory containing `.git`, cached per directory (many edited files
/// share a few directories, and this runs on every new transcript line).
fn git_root(file: &std::path::Path) -> Option<std::path::PathBuf> {
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::sync::{Mutex, OnceLock};
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, Option<PathBuf>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);

    let start = file.parent()?.to_path_buf();
    if let Some(hit) = cache.lock().unwrap().get(&start) {
        return hit.clone();
    }
    let found = start.ancestors().find(|d| d.join(".git").exists()).map(|d| d.to_path_buf());
    cache.lock().unwrap().insert(start, found.clone());
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn uses_git_root_not_cwd() {
        let repo = std::env::temp_dir().join(format!("subagents-gitroot-{}", std::process::id()));
        std::fs::create_dir_all(repo.join(".git")).unwrap();
        std::fs::create_dir_all(repo.join("src/deep")).unwrap();
        let file = repo.join("src/deep/a.rs");
        let file_str = file.to_string_lossy().into_owned();
        // A cwd inside the repo (the session `cd`ed) must not change the result.
        let cwd = repo.join("src").to_string_lossy().into_owned();
        assert_eq!(repo_relative(&file_str, Some(&cwd)), "src/deep/a.rs");
        assert_eq!(repo_relative(&file_str, None), "src/deep/a.rs");
        std::fs::remove_dir_all(&repo).ok();
    }

    fn assistant(id: &str, out: u64, stop: Option<&str>) -> Value {
        json!({ "type": "assistant", "timestamp": "2026-10-21T10:00:00.000Z",
                "message": { "id": id, "model": "claude-opus-5-5", "stop_reason": stop,
                  "usage": { "input_tokens": 10, "output_tokens": out, "cache_read_input_tokens": 1000,
                             "cache_creation_input_tokens": 200,
                             "cache_creation": { "ephemeral_5m_input_tokens": 150, "ephemeral_1h_input_tokens": 50 } },
                  "content": [] } })
    }

    #[test]
    fn counts_each_response_once_even_across_reads() {
        let mut t = StatsTracker::default();
        // Subagent streaming: partial snapshots, then the final line.
        t.feed(&[assistant("m1", 10, None), assistant("m1", 10, None)]);
        t.feed(&[assistant("m1", 375, Some("tool_use")), assistant("m2", 5, Some("end_turn"))]);
        let s = t.snapshot();
        assert_eq!(s.tokens.output, 380);
        assert_eq!(s.tokens.input, 20);
        assert_eq!(s.tokens.cache_read, 2000);
        assert_eq!((s.tokens.cache_write_5m, s.tokens.cache_write_1h), (300, 100));
        assert_eq!(s.context_tokens, 10 + 1000 + 200);
        assert_eq!(s.model.as_deref(), Some("claude-opus-5-5"));
        assert_eq!(s.last_activity, 1792576800000);
    }

    #[test]
    fn collects_edited_files_relative_to_repo() {
        let mut t = StatsTracker::default();
        t.feed(&[json!({ "type": "assistant", "cwd": "F:\\repo",
            "message": { "content": [
                { "type": "tool_use", "name": "Edit", "input": { "file_path": "F:\\repo\\src\\A.rs" } },
                { "type": "tool_use", "name": "Write",
                  "input": { "file_path": "F:\\repo\\.claude\\worktrees\\agent-1\\src\\a.rs" } },
                { "type": "tool_use", "name": "Read", "input": { "file_path": "F:\\repo\\b.rs" } }
            ] } })]);
        let files = t.snapshot().edited_files;
        if cfg!(windows) {
            assert_eq!(files, vec!["src/a.rs"]);
        } else {
            assert_eq!(files.len(), 2);
        }
    }
}

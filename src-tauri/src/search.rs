//! Full-text search across the transcripts the app knows about (the history window).

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::Value;

const MAX_READ_BYTES: u64 = 32 * 1024 * 1024;
const MAX_LINE_BYTES: usize = 1024 * 1024;
const MAX_HITS: usize = 200;

/// Where a transcript belongs in the graph.
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SearchSource {
    Session { id: String },
    Agent { id: String },
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    pub source: SearchSource,
    /// Record timestamp (RFC 3339), for ordering and display.
    pub at: String,
    /// What matched: `prompt`, `reply`, `tool:<Name>` or `result`.
    pub what: String,
    pub snippet: String,
}

/// Case-insensitive substring search, newest hits first.
pub fn search(sources: &[(PathBuf, SearchSource)], query: &str) -> Vec<SearchHit> {
    let needle = query.trim().to_lowercase();
    if needle.chars().count() < 2 {
        return Vec::new();
    }
    let mut hits = Vec::new();
    for (path, source) in sources {
        let Ok(text) = read_tail(path) else { continue };
        for line in text.lines() {
            if line.len() > MAX_LINE_BYTES || !line.to_lowercase().contains(&needle) {
                continue; // cheap pre-filter before parsing
            }
            let Ok(rec) = serde_json::from_str::<Value>(line) else { continue };
            let at = rec.get("timestamp").and_then(Value::as_str).unwrap_or("").to_owned();
            for (what, text) in searchable_texts(&rec) {
                if let Some(snippet) = snippet_around(&text, &needle) {
                    hits.push(SearchHit { source: source.clone(), at: at.clone(), what, snippet });
                }
            }
        }
    }
    hits.sort_by(|a, b| b.at.cmp(&a.at));
    hits.truncate(MAX_HITS);
    hits
}

fn read_tail(path: &Path) -> std::io::Result<String> {
    let mut file = File::open(path)?;
    let size = file.metadata()?.len();
    let start = size.saturating_sub(MAX_READ_BYTES);
    file.seek(SeekFrom::Start(start))?;
    let mut buf = Vec::new();
    file.read_to_end(&mut buf)?;
    Ok(String::from_utf8_lossy(&buf).into_owned())
}

/// The human-meaningful texts in a record, labeled by what they are.
fn searchable_texts(rec: &Value) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let kind = rec.get("type").and_then(Value::as_str);
    let content = rec.pointer("/message/content");
    match (kind, content) {
        (Some("user"), Some(Value::String(s))) => out.push(("prompt".into(), s.clone())),
        (Some(k @ ("user" | "assistant")), Some(Value::Array(blocks))) => {
            for b in blocks {
                match b.get("type").and_then(Value::as_str) {
                    Some("text") => {
                        let label = if k == "user" { "prompt" } else { "reply" };
                        out.push((label.into(), b.get("text").and_then(Value::as_str).unwrap_or("").into()));
                    }
                    Some("tool_use") => {
                        let name = b.get("name").and_then(Value::as_str).unwrap_or("tool");
                        out.push((format!("tool:{name}"), flatten(b.get("input").unwrap_or(&Value::Null))));
                    }
                    Some("tool_result") => out.push(("result".into(), flatten(b.get("content").unwrap_or(&Value::Null)))),
                    _ => {}
                }
            }
        }
        _ => {}
    }
    out
}

/// All string leaves of a JSON value, joined; keys are skipped so they don't match.
fn flatten(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Array(a) => a.iter().map(flatten).filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" · "),
        Value::Object(o) => o.values().map(flatten).filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" · "),
        _ => String::new(),
    }
}

/// ~60 chars before and ~100 after the first match, on one line.
fn snippet_around(text: &str, needle: &str) -> Option<String> {
    let lower = text.to_lowercase();
    let pos = lower.find(needle)?;
    // Lowercasing can change byte lengths for some scripts; fall back to the lowered text.
    let src = if lower.len() == text.len() { text } else { lower.as_str() };
    let chars: Vec<(usize, char)> = src.char_indices().collect();
    let at = chars.iter().position(|&(i, _)| i >= pos).unwrap_or(0);
    let from = at.saturating_sub(60);
    let to = (at + needle.chars().count() + 100).min(chars.len());
    let mut s: String = chars[from..to].iter().map(|&(_, c)| if c.is_whitespace() { ' ' } else { c }).collect();
    s = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if from > 0 {
        s.insert(0, '…');
    }
    if to < chars.len() {
        s.push('…');
    }
    Some(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn finds_tool_inputs_and_text_but_not_metadata() {
        let dir = std::env::temp_dir().join(format!("subagents-search-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("t.jsonl");
        let lines = [
            json!({ "type": "user", "cwd": "F:/executor-repo", "timestamp": "2026-10-21T10:00:00Z",
                    "message": { "content": "please fix the bug" } }),
            json!({ "type": "assistant", "timestamp": "2026-10-21T10:01:00Z", "message": { "content": [
                { "type": "tool_use", "name": "Edit", "input": { "file_path": "src/Executor.ts", "old_string": "a" } } ] } }),
        ];
        std::fs::write(&path, lines.iter().map(|l| l.to_string() + "\n").collect::<String>()).unwrap();
        let sources = vec![(path, SearchSource::Agent { id: "tu1".into() })];

        let hits = search(&sources, "executor");
        assert_eq!(hits.len(), 1, "cwd must not match: {hits:?}");
        assert_eq!(hits[0].what, "tool:Edit");
        assert!(hits[0].snippet.contains("src/Executor.ts"));
        assert!(search(&sources, "x").is_empty(), "one-char queries are ignored");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn snippet_is_trimmed_around_match() {
        let text = format!("{}needle{}", "a".repeat(200), "b".repeat(200));
        let s = snippet_around(&text, "needle").unwrap();
        assert!(s.starts_with('…') && s.ends_with('…'));
        assert!(s.contains("needle"));
        assert!(s.chars().count() < 180);
    }
}

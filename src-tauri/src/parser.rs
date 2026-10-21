//! Parsing of the files Claude Code writes under `~/.claude`.
//! Everything is lenient: missing or mistyped fields fall back to defaults, never errors.

use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

use crate::types::{AgentNode, AgentStatus, InFlight, JobNode, SessionInfo, TimelineEntry};

/// How far back from the end of a transcript to start reading the first time we see it.
const INITIAL_TAIL_BYTES: u64 = 64 * 1024;

/// First non-empty string among `keys` (mirrors JS `a || b`).
fn str_field(v: &Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .filter_map(|k| v.get(*k).and_then(Value::as_str))
        .find(|s| !s.is_empty())
        .map(str::to_owned)
}

fn int_field(v: &Value, key: &str) -> i64 {
    match v.get(key) {
        Some(n) => n.as_i64().or_else(|| n.as_f64().map(|f| f as i64)).unwrap_or(0),
        None => 0,
    }
}

fn read_json(path: &Path) -> Option<Value> {
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn parse_timestamp(v: &Value) -> Option<i64> {
    let s = v.get("timestamp")?.as_str()?;
    chrono::DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|t| t.timestamp_millis())
}

pub fn parse_session(path: &Path) -> Option<SessionInfo> {
    parse_session_value(&read_json(path)?)
}

fn parse_session_value(data: &Value) -> Option<SessionInfo> {
    let pid = match data.get("pid")? {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    };
    Some(SessionInfo {
        id: str_field(data, &["sessionId"])?,
        pid,
        cwd: str_field(data, &["cwd"]).unwrap_or_default(),
        status: str_field(data, &["status"]).unwrap_or_default(),
        started_at: int_field(data, "startedAt"),
        version: str_field(data, &["version"]).unwrap_or_default(),
        kind: str_field(data, &["kind"]).unwrap_or_default(),
        transcript_path: None,
    })
}

/// `subagents/agent-<id>.meta.json` names the parent's `Agent` tool_use id.
pub fn parse_subagent_meta(path: &Path) -> Option<String> {
    str_field(&read_json(path)?, &["toolUseId"])
}

/// `state_file` is `jobs/<id>/state.json`; the sibling `timeline.jsonl` is folded in.
pub fn parse_job(state_file: &Path) -> Option<JobNode> {
    let data = read_json(state_file)?;
    let dir = state_file.parent()?;
    let id = dir.file_name()?.to_string_lossy().into_owned();
    let timeline = std::fs::read_to_string(dir.join("timeline.jsonl"))
        .map(|s| parse_timeline(&s))
        .unwrap_or_default();
    Some(parse_job_value(id, &data, timeline))
}

fn parse_job_value(id: String, data: &Value, timeline: Vec<TimelineEntry>) -> JobNode {
    let intent = str_field(data, &["intent"]).unwrap_or_default();
    let name = str_field(data, &["name"])
        .or_else(|| (!intent.is_empty()).then(|| intent.chars().take(60).collect()))
        .unwrap_or_else(|| "Unknown task".into());
    let in_flight = data
        .get("inFlight")
        .map(|f| InFlight {
            tasks: int_field(f, "tasks"),
            queued: int_field(f, "queued"),
            kinds: f
                .get("kinds")
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(|k| k.as_str().map(str::to_owned)).collect())
                .unwrap_or_default(),
        })
        .unwrap_or_default();

    JobNode {
        id,
        name,
        intent,
        cwd: str_field(data, &["cwd"]).unwrap_or_default(),
        status: str_field(data, &["state", "status"]).unwrap_or_else(|| "unknown".into()),
        detail: str_field(data, &["detail", "needs"]).unwrap_or_default(),
        in_flight,
        tokens: int_field(data, "tokens"),
        created_at: str_field(data, &["createdAt"]).unwrap_or_default(),
        updated_at: str_field(data, &["updatedAt"]).unwrap_or_default(),
        suggested_reply: str_field(data, &["suggestedReply"]),
        session_id: str_field(data, &["sessionId"]),
        timeline,
    }
}

fn parse_timeline(text: &str) -> Vec<TimelineEntry> {
    text.lines()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .map(|v| TimelineEntry {
            at: str_field(&v, &["at"]).unwrap_or_default(),
            state: str_field(&v, &["state"]).unwrap_or_default(),
            detail: str_field(&v, &["detail"]).unwrap_or_default(),
            text: str_field(&v, &["text"]),
        })
        .collect()
}

/// Tracks a read offset per transcript so each call returns only newly appended lines.
#[derive(Default)]
pub struct TranscriptTail {
    positions: HashMap<PathBuf, u64>,
}

impl TranscriptTail {
    pub fn read_new_lines(&mut self, path: &Path) -> Vec<Value> {
        let Ok(mut file) = File::open(path) else { return Vec::new() };
        let Ok(size) = file.metadata().map(|m| m.len()) else { return Vec::new() };

        let known = self.positions.get(path).copied().filter(|&p| p <= size);
        // First sight (or the file shrank, i.e. was rewritten): start near the end.
        let start = known.unwrap_or_else(|| size.saturating_sub(INITIAL_TAIL_BYTES));
        if size <= start {
            self.positions.insert(path.to_owned(), start);
            return Vec::new();
        }

        let mut buf = Vec::with_capacity((size - start) as usize);
        if file.seek(SeekFrom::Start(start)).is_err()
            || file.take(size - start).read_to_end(&mut buf).is_err()
        {
            return Vec::new();
        }

        // Only consume complete lines; a trailing partial line is re-read next time.
        let Some(last_nl) = buf.iter().rposition(|&b| b == b'\n') else {
            self.positions.insert(path.to_owned(), start);
            return Vec::new();
        };
        self.positions.insert(path.to_owned(), start + last_nl as u64 + 1);

        let mut lines = buf[..last_nl].split(|&b| b == b'\n');
        // Starting mid-file means the first line is almost certainly cut off.
        if known.is_none() && start > 0 {
            lines.next();
        }
        lines
            .filter_map(|l| serde_json::from_slice::<Value>(l).ok())
            .collect()
    }
}

#[derive(Debug, PartialEq)]
pub struct Completion {
    pub id: String,
    pub failed: bool,
    pub at: i64,
}

/// Text inside `<tag>...</tag>`, if present.
fn tag_text<'a>(s: &'a str, tag: &str) -> Option<&'a str> {
    let open = format!("<{tag}>");
    let start = s.find(&open)? + open.len();
    let len = s[start..].find(&format!("</{tag}>"))?;
    Some(s[start..start + len].trim())
}

/// Every string a record carries as message content (plain or text blocks), plus the
/// top-level `content` of queue records.
fn record_texts(msg: &Value) -> impl Iterator<Item = &str> {
    let top = msg.get("content").and_then(Value::as_str);
    let content = msg.pointer("/message/content");
    let plain = content.and_then(Value::as_str);
    let blocks = content
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|b| b.get("text").and_then(Value::as_str));
    top.into_iter().chain(plain).chain(blocks)
}

/// A background agent's tool_result only acknowledges the launch; it finishes later.
fn is_async_launch(msg: &Value, block: &Value) -> bool {
    if msg.pointer("/toolUseResult/status").and_then(Value::as_str) == Some("async_launched") {
        return true;
    }
    let text = match block.get("content") {
        Some(Value::String(s)) => s.as_str(),
        Some(Value::Array(a)) => a.first().and_then(|b| b.get("text")).and_then(Value::as_str).unwrap_or(""),
        _ => "",
    };
    text.starts_with("Async agent launched")
}

/// Extracts `Agent` tool calls (spawns) and completions: synchronous tool results and
/// `<task-notification>`s for background agents.
pub fn parse_agent_events(messages: &[Value]) -> (Vec<AgentNode>, Vec<Completion>) {
    let mut spawned = Vec::new();
    let mut completed = Vec::new();

    for msg in messages {
        let at = parse_timestamp(msg);

        for text in record_texts(msg) {
            let Some(note) = tag_text(text, "task-notification") else { continue };
            if let (Some(id), Some(status)) = (tag_text(note, "tool-use-id"), tag_text(note, "status")) {
                completed.push(Completion {
                    id: id.to_owned(),
                    failed: status != "completed",
                    at: at.unwrap_or_else(now_ms),
                });
            }
        }

        let Some(blocks) = msg.pointer("/message/content").and_then(Value::as_array) else {
            continue;
        };
        let kind = msg.get("type").and_then(Value::as_str);

        for block in blocks {
            let block_type = block.get("type").and_then(Value::as_str);
            match (kind, block_type) {
                (Some("assistant"), Some("tool_use"))
                    if block.get("name").and_then(Value::as_str) == Some("Agent") =>
                {
                    let Some(id) = str_field(block, &["id"]) else { continue };
                    let input = block.get("input").unwrap_or(&Value::Null);
                    let prompt = str_field(input, &["prompt"]).unwrap_or_default();
                    let description = str_field(input, &["description"])
                        .or_else(|| (!prompt.is_empty()).then(|| prompt.chars().take(80).collect()))
                        .unwrap_or_else(|| "Agent".into());
                    spawned.push(AgentNode {
                        id,
                        session_id: str_field(msg, &["sessionId"]).unwrap_or_default(),
                        parent_id: None,
                        description,
                        prompt,
                        status: AgentStatus::Running,
                        start_time: at.unwrap_or_else(now_ms),
                        end_time: None,
                        agent_type: str_field(input, &["subagent_type"]),
                        transcript_path: None,
                    });
                }
                (Some("user"), Some("tool_result")) => {
                    let Some(id) = str_field(block, &["tool_use_id"]) else { continue };
                    if is_async_launch(msg, block) {
                        continue;
                    }
                    completed.push(Completion {
                        id,
                        failed: block.get("is_error").and_then(Value::as_bool).unwrap_or(false),
                        at: at.unwrap_or_else(now_ms),
                    });
                }
                _ => {}
            }
        }
    }

    (spawned, completed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::Write;

    #[test]
    fn session_from_real_shape() {
        let s = parse_session_value(&json!({
            "pid": 37896, "sessionId": "49b4", "cwd": "F:\\x", "startedAt": 1792593068513i64,
            "version": "2.1.295", "kind": "interactive", "status": "busy"
        }))
        .unwrap();
        assert_eq!(s.pid, "37896");
        assert_eq!(s.started_at, 1792593068513);
        assert_eq!(s.status, "busy");
    }

    #[test]
    fn session_without_id_is_skipped() {
        assert!(parse_session_value(&json!({ "pid": 1 })).is_none());
    }

    #[test]
    fn job_fallbacks() {
        let job = parse_job_value(
            "bc8c".into(),
            &json!({ "state": "blocked", "detail": null, "needs": "Any new errors?",
                     "intent": "x".repeat(100), "tokens": 55888 }),
            vec![],
        );
        assert_eq!(job.name, "x".repeat(60));
        assert_eq!(job.status, "blocked");
        assert_eq!(job.detail, "Any new errors?");
        assert_eq!(job.in_flight, InFlight::default());
        assert_eq!(job.tokens, 55888);
    }

    #[test]
    fn timeline_null_detail_becomes_empty() {
        let t = parse_timeline("{\"at\":\"a\",\"state\":\"done\",\"detail\":null}\nnot json\n");
        assert_eq!(t.len(), 1);
        assert_eq!(t[0].detail, "");
        assert_eq!(t[0].text, None);
    }

    #[test]
    fn agent_spawn_and_completion() {
        let msgs = vec![
            json!({ "type": "assistant", "sessionId": "s1", "timestamp": "2026-10-21T10:00:00.000Z",
                    "message": { "content": [
                        { "type": "text", "text": "hi" },
                        { "type": "tool_use", "name": "Agent", "id": "tu1",
                          "input": { "description": "Explore", "prompt": "find it", "subagent_type": "Explore" } },
                        { "type": "tool_use", "name": "Read", "id": "tu2", "input": {} }
                    ]}}),
            json!({ "type": "user", "timestamp": "2026-10-21T10:01:00.000Z",
                    "message": { "content": [ { "type": "tool_result", "tool_use_id": "tu1", "is_error": true } ] }}),
            json!({ "type": "user", "message": { "content": "plain string content" } }),
        ];
        let (spawned, completed) = parse_agent_events(&msgs);
        assert_eq!(spawned.len(), 1);
        assert_eq!(spawned[0].id, "tu1");
        assert_eq!(spawned[0].description, "Explore");
        assert_eq!(spawned[0].agent_type.as_deref(), Some("Explore"));
        assert_eq!(spawned[0].start_time, 1792576800000);
        assert_eq!(
            completed,
            vec![Completion { id: "tu1".into(), failed: true, at: 1792576860000 }]
        );
    }

    #[test]
    fn async_launch_is_not_completion_but_notification_is() {
        let note = "<task-notification>\n<task-id>a4d2</task-id>\n<tool-use-id>tu9</tool-use-id>\n\
                    <status>completed</status>\n<summary>done</summary>\n</task-notification>";
        let msgs = vec![
            json!({ "type": "user", "toolUseResult": { "status": "async_launched" },
                    "message": { "content": [ { "type": "tool_result", "tool_use_id": "tu9",
                        "content": [ { "type": "text", "text": "Async agent launched successfully." } ] } ] }}),
            json!({ "type": "queue-operation", "timestamp": "2026-10-21T10:05:00.000Z", "content": note }),
            json!({ "type": "user", "message": { "content": [ { "type": "text",
                    "text": note.replace("completed", "failed") } ] }}),
        ];
        let (_, completed) = parse_agent_events(&msgs);
        assert_eq!(
            completed,
            vec![
                Completion { id: "tu9".into(), failed: false, at: 1792577100000 },
                Completion { id: "tu9".into(), failed: true, at: completed[1].at },
            ]
        );
    }

    #[test]
    fn tail_reads_only_complete_new_lines() {
        let dir = std::env::temp_dir().join(format!("subagents-tail-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("t.jsonl");
        let mut f = File::create(&path).unwrap();
        let mut tail = TranscriptTail::default();

        write!(f, "{{\"n\":1}}\n{{\"n\":").unwrap();
        f.flush().unwrap();
        assert_eq!(tail.read_new_lines(&path), vec![json!({ "n": 1 })]);

        write!(f, "2}}\n").unwrap();
        f.flush().unwrap();
        assert_eq!(tail.read_new_lines(&path), vec![json!({ "n": 2 })]);
        assert!(tail.read_new_lines(&path).is_empty());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn tail_skips_cut_first_line_when_starting_mid_file() {
        let dir = std::env::temp_dir().join(format!("subagents-tail-big-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("big.jsonl");
        let filler = format!("{{\"pad\":\"{}\"}}\n", "x".repeat(1000));
        let mut content = filler.repeat(100); // > 64 KiB
        content.push_str("{\"last\":true}\n");
        std::fs::write(&path, content).unwrap();

        let lines = TranscriptTail::default().read_new_lines(&path);
        assert!(lines.iter().all(|l| l.get("pad").is_some() || l.get("last").is_some()));
        assert_eq!(lines.last(), Some(&json!({ "last": true })));

        std::fs::remove_dir_all(&dir).ok();
    }
}

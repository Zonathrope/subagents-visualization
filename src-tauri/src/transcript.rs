//! Turns a transcript JSONL file into a compact list of display items for the detail view.

use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use serde::Serialize;
use serde_json::{Map, Value};

/// Long main-session transcripts are only read from the end.
const MAX_READ_BYTES: u64 = 32 * 1024 * 1024;
/// Most recent records kept for display.
const MAX_RECORDS: usize = 1500;
/// Single records above this (inline images, huge tool output) are skipped unparsed.
const MAX_LINE_BYTES: usize = 1024 * 1024;
/// Caps per string inside tool inputs (e.g. a `Write`'s file content).
const MAX_INPUT_CHARS: usize = 20_000;
const MAX_RESULT_CHARS: usize = 6_000;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptView {
    pub items: Vec<Item>,
    /// Older records were left out; only the tail of the transcript is shown.
    pub truncated: bool,
}

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Item {
    User { text: String, at: String },
    Assistant { text: String, at: String },
    /// `text` is empty when the reasoning was not stored, only its signature.
    Thinking { text: String, at: String },
    Tool(ToolCall),
    /// A system message worth showing, e.g. a background agent's completion notice.
    Notice { text: String, at: String },
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub at: String,
    pub input: Value,
    pub result: Option<ToolOutput>,
    /// Unified-diff hunks for file edits (`structuredPatch` from the tool result).
    pub patch: Option<Value>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolOutput {
    pub text: String,
    pub is_error: bool,
}

/// One file edit within a session, from the main session or one of its agents.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionChange {
    /// Agent tool_use id, or `None` for the main session's own edit.
    pub agent_id: Option<String>,
    pub actor: String,
    /// Repo-relative path, so worktree copies group with the original file.
    pub file: String,
    pub call: ToolCall,
}

const EDIT_TOOLS: &[&str] = &["Edit", "Write", "MultiEdit", "NotebookEdit"];

/// All edits across a session's transcripts, oldest first.
pub fn session_changes(sources: &[(std::path::PathBuf, Option<(String, String)>)]) -> Vec<SessionChange> {
    let mut changes = Vec::new();
    for (path, agent) in sources {
        let Ok(view) = load(path) else { continue };
        for item in view.items {
            let Item::Tool(call) = item else { continue };
            if !EDIT_TOOLS.contains(&call.name.as_str()) {
                continue;
            }
            let raw = call.input.get("file_path").or_else(|| call.input.get("notebook_path")).and_then(Value::as_str);
            let Some(raw) = raw else { continue };
            changes.push(SessionChange {
                agent_id: agent.as_ref().map(|(id, _)| id.clone()),
                actor: agent.as_ref().map_or_else(|| "Main session".to_owned(), |(_, d)| d.clone()),
                file: crate::stats::repo_relative(raw, None),
                call,
            });
        }
    }
    changes.sort_by(|a, b| a.call.at.cmp(&b.call.at));
    changes
}

pub fn load(path: &Path) -> std::io::Result<TranscriptView> {
    let mut file = File::open(path)?;
    let size = file.metadata()?.len();
    let start = size.saturating_sub(MAX_READ_BYTES);
    file.seek(SeekFrom::Start(start))?;
    let mut buf = Vec::new();
    file.read_to_end(&mut buf)?;

    let mut lines: Vec<&[u8]> = buf.split(|&b| b == b'\n').collect();
    if start > 0 {
        lines.remove(0); // cut off mid-line
    }
    lines.retain(|l| !l.is_empty() && l.len() <= MAX_LINE_BYTES);
    let skip = lines.len().saturating_sub(MAX_RECORDS);
    let records: Vec<Value> = lines[skip..].iter().filter_map(|l| serde_json::from_slice(l).ok()).collect();
    Ok(TranscriptView { items: build_items(&records), truncated: start > 0 || skip > 0 })
}

fn truncate(s: &str, max: usize) -> String {
    match s.char_indices().nth(max) {
        Some((i, _)) => format!("{}\n… [{} more chars]", &s[..i], s[i..].chars().count()),
        None => s.to_owned(),
    }
}

fn truncate_strings(v: &Value) -> Value {
    match v {
        Value::String(s) => Value::String(truncate(s, MAX_INPUT_CHARS)),
        Value::Array(a) => Value::Array(a.iter().map(truncate_strings).collect()),
        Value::Object(o) => {
            Value::Object(o.iter().map(|(k, v)| (k.clone(), truncate_strings(v))).collect::<Map<_, _>>())
        }
        other => other.clone(),
    }
}

/// Tool results are either a string or a list of content blocks.
fn result_text(content: Option<&Value>) -> String {
    match content {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(blocks)) => blocks
            .iter()
            .map(|b| match b.get("type").and_then(Value::as_str) {
                Some("text") => b.get("text").and_then(Value::as_str).unwrap_or("").to_owned(),
                Some(other) => format!("[{other}]"),
                None => String::new(),
            })
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

/// Harness-injected user text that isn't something a person typed.
fn is_system_text(text: &str) -> bool {
    let t = text.trim_start();
    ["<system-reminder>", "<command-", "<local-command-", "<task-notification>", "Caveat:"]
        .iter()
        .any(|p| t.starts_with(p))
}

fn notification_summary(text: &str) -> Option<String> {
    let body = text.split_once("<task-notification>")?.1;
    let tag = |name: &str| {
        let open = format!("<{name}>");
        let s = body.find(&open)? + open.len();
        let e = body[s..].find(&format!("</{name}>"))?;
        Some(body[s..s + e].trim().to_owned())
    };
    let summary = tag("summary").unwrap_or_else(|| "Background task finished".into());
    Some(match tag("status") {
        Some(status) => format!("{summary} ({status})"),
        None => summary,
    })
}

fn build_items(records: &[Value]) -> Vec<Item> {
    let mut items = Vec::new();
    let mut tool_index: HashMap<String, usize> = HashMap::new();

    for rec in records {
        let at = rec.get("timestamp").and_then(Value::as_str).unwrap_or("").to_owned();
        let is_meta = rec.get("isMeta").and_then(Value::as_bool).unwrap_or(false);
        let content = rec.pointer("/message/content");

        match rec.get("type").and_then(Value::as_str) {
            Some("user") => {
                let push_text = |text: &str, items: &mut Vec<Item>| {
                    if let Some(note) = notification_summary(text) {
                        items.push(Item::Notice { text: note, at: at.clone() });
                    } else if !is_meta && !is_system_text(text) && !text.trim().is_empty() {
                        items.push(Item::User { text: text.to_owned(), at: at.clone() });
                    }
                };
                match content {
                    Some(Value::String(s)) => push_text(s, &mut items),
                    Some(Value::Array(blocks)) => {
                        for block in blocks {
                            match block.get("type").and_then(Value::as_str) {
                                Some("text") => {
                                    push_text(block.get("text").and_then(Value::as_str).unwrap_or(""), &mut items)
                                }
                                Some("tool_result") => {
                                    let id = block.get("tool_use_id").and_then(Value::as_str).unwrap_or("");
                                    let Some(Item::Tool(call)) = tool_index.get(id).map(|&i| &mut items[i]) else {
                                        continue;
                                    };
                                    call.result = Some(ToolOutput {
                                        text: truncate(&result_text(block.get("content")), MAX_RESULT_CHARS),
                                        is_error: block.get("is_error").and_then(Value::as_bool).unwrap_or(false),
                                    });
                                    call.patch = rec
                                        .pointer("/toolUseResult/structuredPatch")
                                        .filter(|p| p.as_array().is_some_and(|a| !a.is_empty()))
                                        .cloned();
                                }
                                _ => {}
                            }
                        }
                    }
                    _ => {}
                }
            }
            Some("assistant") => {
                for block in content.and_then(Value::as_array).into_iter().flatten() {
                    let text = |key: &str| block.get(key).and_then(Value::as_str).unwrap_or("").to_owned();
                    match block.get("type").and_then(Value::as_str) {
                        Some("text") if !text("text").trim().is_empty() => {
                            items.push(Item::Assistant { text: text("text"), at: at.clone() })
                        }
                        Some("thinking") => items.push(Item::Thinking { text: text("thinking"), at: at.clone() }),
                        Some("redacted_thinking") => items.push(Item::Thinking { text: String::new(), at: at.clone() }),
                        Some("tool_use") => {
                            let id = text("id");
                            tool_index.insert(id.clone(), items.len());
                            items.push(Item::Tool(ToolCall {
                                id,
                                name: text("name"),
                                at: at.clone(),
                                input: truncate_strings(block.get("input").unwrap_or(&Value::Null)),
                                result: None,
                                patch: None,
                            }));
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    items
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn builds_conversation_with_tool_results_and_patch() {
        let records = vec![
            json!({ "type": "user", "timestamp": "t0", "message": { "content": "Fix the bug" } }),
            json!({ "type": "user", "isMeta": true, "message": { "content": "<system-reminder>x</system-reminder>" } }),
            json!({ "type": "assistant", "timestamp": "t1", "message": { "content": [
                { "type": "thinking", "thinking": "", "signature": "abc" },
                { "type": "text", "text": "Editing now." },
                { "type": "tool_use", "id": "e1", "name": "Edit",
                  "input": { "file_path": "a.ts", "old_string": "a", "new_string": "b" } }
            ]}}),
            json!({ "type": "user", "toolUseResult": { "structuredPatch": [ { "oldStart": 1, "lines": ["-a", "+b"] } ] },
                    "message": { "content": [ { "type": "tool_result", "tool_use_id": "e1",
                        "content": [ { "type": "text", "text": "ok" } ] } ] } }),
            json!({ "type": "user", "message": { "content": [ { "type": "text",
                "text": "<task-notification>\n<status>completed</status>\n<summary>Agent \"x\" finished</summary>\n</task-notification>" } ] } }),
        ];
        let items = build_items(&records);
        assert_eq!(items.len(), 5, "{items:#?}");
        assert!(matches!(&items[0], Item::User { text, .. } if text == "Fix the bug"));
        assert!(matches!(&items[1], Item::Thinking { text, .. } if text.is_empty()));
        assert!(matches!(&items[2], Item::Assistant { .. }));
        let Item::Tool(call) = &items[3] else { panic!() };
        assert_eq!(call.result.as_ref().unwrap().text, "ok");
        assert_eq!(call.patch.as_ref().unwrap()[0]["lines"][1], "+b");
        assert!(matches!(&items[4], Item::Notice { text, .. } if text == "Agent \"x\" finished (completed)"));
    }

    #[test]
    fn truncates_by_chars() {
        assert_eq!(truncate("héllo", 10), "héllo");
        assert_eq!(truncate("héllo", 2), "hé\n… [3 more chars]");
    }
}

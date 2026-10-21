//! Wire types shared with the frontend (`client/src/types.ts`).
//! Field names are serialized camelCase so the JSON matches the TS interfaces exactly.

use serde::Serialize;

use crate::stats::TranscriptStats;

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentStatus {
    Running,
    Completed,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionInfo {
    pub id: String,
    pub pid: String,
    pub cwd: String,
    pub status: String,
    pub started_at: i64,
    pub version: String,
    pub kind: String,
    /// Main transcript (`projects/<project>/<sessionId>.jsonl`), once seen.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transcript_path: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentNode {
    pub id: String,
    pub session_id: String,
    pub parent_id: Option<String>,
    pub description: String,
    pub prompt: String,
    pub status: AgentStatus,
    pub start_time: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_time: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_type: Option<String>,
    /// The subagent's own transcript (`.../subagents/agent-<id>.jsonl`), once linked.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transcript_path: Option<String>,
    /// Tokens, last activity and edited files, from the agent's own transcript.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stats: Option<TranscriptStats>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct InFlight {
    pub tasks: i64,
    pub queued: i64,
    pub kinds: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobNode {
    pub id: String,
    pub name: String,
    pub intent: String,
    pub cwd: String,
    pub status: String,
    pub detail: String,
    pub in_flight: InFlight,
    pub tokens: i64,
    pub created_at: String,
    pub updated_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggested_reply: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    /// The conversation a background session continues; its transcript is named after this.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resume_session_id: Option<String>,
    pub timeline: Vec<TimelineEntry>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct TimelineEntry {
    pub at: String,
    pub state: String,
    pub detail: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

/// What a session's main transcript says about it. Covers sessions that have ended,
/// which no longer appear under `sessions/`.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSummary {
    pub id: String,
    pub cwd: String,
    /// Timestamp (ms) of the newest record seen.
    pub last_activity: i64,
    pub transcript_path: String,
    /// Stats for the main session's own work (edits it made itself, its tokens).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stats: Option<TranscriptStats>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct GraphState {
    pub sessions: Vec<SessionInfo>,
    pub agents: Vec<AgentNode>,
    pub jobs: Vec<JobNode>,
    pub summaries: Vec<SessionSummary>,
    /// False until the startup scan has finished. Changes seen before then are history
    /// being loaded, not live transitions.
    pub ready: bool,
}

/// Incremental update pushed to the frontend on the `graph-event` channel.
/// Serializes as `{ "type": "session_update", "data": ... }`, same as the old `WsMessage`.
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum GraphEvent {
    SessionUpdate { data: SessionInfo },
    SessionRemoved { id: String },
    AgentUpdate { data: AgentNode },
    JobUpdate { data: JobNode },
    JobRemoved { id: String },
    SessionSummary { data: SessionSummary },
    /// The startup scan has finished.
    Ready,
    /// New lines were appended to a transcript; open detail views should reload it.
    TranscriptChanged { path: String },
}

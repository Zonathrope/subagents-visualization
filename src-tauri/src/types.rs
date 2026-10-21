//! Wire types shared with the frontend (`client/src/types.ts`).
//! Field names are serialized camelCase so the JSON matches the TS interfaces exactly.

use serde::Serialize;

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

#[derive(Clone, Debug, Default, Serialize)]
pub struct GraphState {
    pub sessions: Vec<SessionInfo>,
    pub agents: Vec<AgentNode>,
    pub jobs: Vec<JobNode>,
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
    /// New lines were appended to a transcript; open detail views should reload it.
    TranscriptChanged { path: String },
}

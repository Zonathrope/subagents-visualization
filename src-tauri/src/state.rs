use std::collections::HashMap;

use crate::stats::TranscriptStats;
use crate::types::{AgentNode, AgentStatus, GraphState, JobNode, SessionInfo, SessionSummary};

#[derive(Default)]
pub struct GraphStore {
    sessions: HashMap<String, SessionInfo>,
    pid_to_session: HashMap<String, String>,
    agents: HashMap<String, AgentNode>,
    jobs: HashMap<String, JobNode>,
    /// `Agent` tool_use id → that subagent's transcript path.
    agent_transcripts: HashMap<String, String>,
    /// Session id → main transcript path.
    session_transcripts: HashMap<String, String>,
    summaries: HashMap<String, SessionSummary>,
    /// `Agent` tool_use id → stats from that subagent's transcript.
    agent_stats: HashMap<String, TranscriptStats>,
}

impl GraphStore {
    pub fn snapshot(&self) -> GraphState {
        GraphState {
            sessions: self.sessions.values().cloned().collect(),
            agents: self.agents.values().cloned().collect(),
            jobs: self.jobs.values().cloned().collect(),
            summaries: self.summaries.values().cloned().collect(),
        }
    }

    pub fn has_session_transcript(&self, session_id: &str) -> bool {
        self.session_transcripts.contains_key(session_id)
    }

    /// Folds new transcript activity into the session's summary. Returns it if it changed.
    pub fn update_summary(
        &mut self,
        session_id: &str,
        path: &str,
        cwd: Option<String>,
        last_activity: Option<i64>,
    ) -> Option<SessionSummary> {
        let is_new = !self.summaries.contains_key(session_id);
        let entry = self.summaries.entry(session_id.to_owned()).or_insert_with(|| SessionSummary {
            id: session_id.to_owned(),
            cwd: String::new(),
            last_activity: 0,
            transcript_path: path.to_owned(),
            stats: None,
        });
        let before = entry.clone();
        if let Some(cwd) = cwd {
            entry.cwd = cwd;
        }
        entry.last_activity = entry.last_activity.max(last_activity.unwrap_or(0));
        (is_new || *entry != before).then(|| entry.clone())
    }

    pub fn update_session(&mut self, mut info: SessionInfo) -> SessionInfo {
        info.transcript_path = self.session_transcripts.get(&info.id).cloned();
        self.pid_to_session.insert(info.pid.clone(), info.id.clone());
        self.sessions.insert(info.id.clone(), info.clone());
        info
    }

    /// Records a session's main transcript. Returns the session if it changed.
    pub fn link_session_transcript(&mut self, session_id: &str, path: &str) -> Option<SessionInfo> {
        self.session_transcripts.insert(session_id.to_owned(), path.to_owned());
        let session = self.sessions.get_mut(session_id)?;
        if session.transcript_path.as_deref() == Some(path) {
            return None;
        }
        session.transcript_path = Some(path.to_owned());
        Some(session.clone())
    }

    /// Records stats for a subagent. Returns the agent if it changed.
    pub fn set_agent_stats(&mut self, tool_use_id: &str, stats: TranscriptStats) -> Option<AgentNode> {
        self.agent_stats.insert(tool_use_id.to_owned(), stats.clone());
        let agent = self.agents.get_mut(tool_use_id)?;
        if agent.stats.as_ref() == Some(&stats) {
            return None;
        }
        agent.stats = Some(stats);
        Some(agent.clone())
    }

    /// Records stats for a session's main transcript. Returns the summary if it changed.
    pub fn set_session_stats(&mut self, session_id: &str, stats: TranscriptStats) -> Option<SessionSummary> {
        let summary = self.summaries.get_mut(session_id)?;
        if summary.stats.as_ref() == Some(&stats) {
            return None;
        }
        summary.stats = Some(stats);
        Some(summary.clone())
    }

    /// Records a subagent's transcript. Returns the agent if it changed.
    pub fn link_agent_transcript(&mut self, tool_use_id: &str, path: &str) -> Option<AgentNode> {
        self.agent_transcripts.insert(tool_use_id.to_owned(), path.to_owned());
        let agent = self.agents.get_mut(tool_use_id)?;
        if agent.transcript_path.as_deref() == Some(path) {
            return None;
        }
        agent.transcript_path = Some(path.to_owned());
        Some(agent.clone())
    }

    /// Returns the removed session's id.
    pub fn remove_session_by_pid(&mut self, pid: &str) -> Option<String> {
        let id = self.pid_to_session.remove(pid)?;
        self.sessions.remove(&id);
        Some(id)
    }

    pub fn update_agent(&mut self, mut agent: AgentNode) -> AgentNode {
        if let Some(existing) = self.agents.get(&agent.id) {
            agent.end_time = agent.end_time.or(existing.end_time);
            agent.parent_id = agent.parent_id.or_else(|| existing.parent_id.clone());
        }
        agent.transcript_path = self.agent_transcripts.get(&agent.id).cloned();
        agent.stats = self.agent_stats.get(&agent.id).cloned();
        self.agents.insert(agent.id.clone(), agent.clone());
        agent
    }

    /// Marks a running agent finished. Returns `None` for unknown ids (non-Agent tool results)
    /// and for agents already finished: the first completion wins, because background agents
    /// are reported more than once (queued notification, then delivered message).
    pub fn complete_agent(&mut self, id: &str, failed: bool, at: i64) -> Option<AgentNode> {
        let agent = self.agents.get_mut(id).filter(|a| a.status == AgentStatus::Running)?;
        agent.status = if failed { AgentStatus::Failed } else { AgentStatus::Completed };
        agent.end_time = Some(at);
        Some(agent.clone())
    }

    pub fn update_job(&mut self, job: JobNode) {
        self.jobs.insert(job.id.clone(), job);
    }

    pub fn remove_job(&mut self, id: &str) -> bool {
        self.jobs.remove(id).is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(id: &str, pid: &str) -> SessionInfo {
        SessionInfo {
            id: id.into(),
            pid: pid.into(),
            cwd: String::new(),
            status: "busy".into(),
            started_at: 0,
            version: String::new(),
            kind: String::new(),
            transcript_path: None,
        }
    }

    fn agent(id: &str) -> AgentNode {
        AgentNode {
            id: id.into(),
            session_id: "s".into(),
            parent_id: None,
            description: "d".into(),
            prompt: String::new(),
            status: AgentStatus::Running,
            start_time: 1,
            end_time: None,
            agent_type: None,
            transcript_path: None,
            stats: None,
        }
    }

    #[test]
    fn session_removed_by_pid() {
        let mut s = GraphStore::default();
        s.update_session(session("a", "10"));
        assert_eq!(s.remove_session_by_pid("10").as_deref(), Some("a"));
        assert!(s.snapshot().sessions.is_empty());
        assert_eq!(s.remove_session_by_pid("10"), None);
    }

    #[test]
    fn complete_known_agent_only() {
        let mut s = GraphStore::default();
        s.update_agent(agent("x"));
        assert!(s.complete_agent("other", false, 5).is_none());
        let done = s.complete_agent("x", true, 5).unwrap();
        assert_eq!(done.status, AgentStatus::Failed);
        assert_eq!(done.end_time, Some(5));
        assert!(s.complete_agent("x", false, 9).is_none(), "first completion wins");
    }

    #[test]
    fn summary_reports_only_changes() {
        let mut s = GraphStore::default();
        assert!(s.update_summary("a", "a.jsonl", None, None).is_some(), "first sight is reported");
        assert!(s.update_summary("a", "a.jsonl", None, None).is_none());
        let up = s.update_summary("a", "a.jsonl", Some("F:/p".into()), Some(10)).unwrap();
        assert_eq!((up.cwd.as_str(), up.last_activity), ("F:/p", 10));
        assert!(s.update_summary("a", "a.jsonl", None, Some(5)).is_none(), "older activity ignored");
    }

    #[test]
    fn transcript_links_apply_in_either_order() {
        let mut s = GraphStore::default();
        assert!(s.link_agent_transcript("early", "e.jsonl").is_none());
        assert_eq!(s.update_agent(agent("early")).transcript_path.as_deref(), Some("e.jsonl"));

        s.update_agent(agent("late"));
        let linked = s.link_agent_transcript("late", "l.jsonl").unwrap();
        assert_eq!(linked.transcript_path.as_deref(), Some("l.jsonl"));
        assert!(s.link_agent_transcript("late", "l.jsonl").is_none(), "unchanged link is silent");

        s.update_session(session("sid", "1"));
        assert!(s.link_session_transcript("sid", "m.jsonl").is_some());
        assert_eq!(s.update_session(session("sid", "1")).transcript_path.as_deref(), Some("m.jsonl"));
    }
}

use std::collections::HashMap;

use crate::types::{AgentNode, AgentStatus, GraphState, JobNode, SessionInfo};

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
}

impl GraphStore {
    pub fn snapshot(&self) -> GraphState {
        GraphState {
            sessions: self.sessions.values().cloned().collect(),
            agents: self.agents.values().cloned().collect(),
            jobs: self.jobs.values().cloned().collect(),
        }
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

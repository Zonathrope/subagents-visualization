use std::collections::HashMap;

use crate::types::{AgentNode, AgentStatus, GraphState, JobNode, SessionInfo};

#[derive(Default)]
pub struct GraphStore {
    sessions: HashMap<String, SessionInfo>,
    pid_to_session: HashMap<String, String>,
    agents: HashMap<String, AgentNode>,
    jobs: HashMap<String, JobNode>,
}

impl GraphStore {
    pub fn snapshot(&self) -> GraphState {
        GraphState {
            sessions: self.sessions.values().cloned().collect(),
            agents: self.agents.values().cloned().collect(),
            jobs: self.jobs.values().cloned().collect(),
        }
    }

    pub fn update_session(&mut self, info: SessionInfo) {
        self.pid_to_session.insert(info.pid.clone(), info.id.clone());
        self.sessions.insert(info.id.clone(), info);
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
        }
        self.agents.insert(agent.id.clone(), agent.clone());
        agent
    }

    /// Marks a running agent finished. Returns `None` for unknown ids (non-Agent tool results).
    pub fn complete_agent(&mut self, id: &str, failed: bool, at: i64) -> Option<AgentNode> {
        let agent = self.agents.get_mut(id)?;
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
    }
}

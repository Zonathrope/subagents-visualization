import type { AgentNode, GraphState, JobNode, SessionInfo } from './types.js'

export class GraphStateManager {
  private sessions = new Map<string, SessionInfo>()
  private pidToSession = new Map<string, string>()
  private agents = new Map<string, AgentNode>()
  private jobs = new Map<string, JobNode>()

  getSnapshot(): GraphState {
    return {
      sessions: Array.from(this.sessions.values()),
      agents: Array.from(this.agents.values()),
      jobs: Array.from(this.jobs.values()),
    }
  }

  updateSession(info: SessionInfo) {
    this.sessions.set(info.id, info)
    this.pidToSession.set(info.pid, info.id)
  }

  removeSessionByPid(pid: string) {
    const sessionId = this.pidToSession.get(pid)
    if (sessionId) {
      this.sessions.delete(sessionId)
      this.pidToSession.delete(pid)
    }
    return sessionId
  }

  updateAgent(agent: AgentNode) {
    const existing = this.agents.get(agent.id)
    this.agents.set(agent.id, existing ? { ...existing, ...agent } : agent)
    return this.agents.get(agent.id)!
  }

  completeAgent(id: string, failed: boolean): AgentNode | null {
    const agent = this.agents.get(id)
    if (!agent) return null
    const status: AgentNode['status'] = failed ? 'failed' : 'completed'
    const updated = { ...agent, status, endTime: Date.now() }
    this.agents.set(id, updated)
    return updated
  }

  updateJob(job: JobNode) {
    this.jobs.set(job.id, job)
  }

  removeJob(id: string) {
    this.jobs.delete(id)
  }
}

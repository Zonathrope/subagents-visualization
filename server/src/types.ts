export type AgentStatus = 'running' | 'completed' | 'failed' | 'blocked' | 'idle'

export interface SessionInfo {
  id: string
  pid: string
  cwd: string
  status: string
  startedAt: number
  version: string
  kind: string
}

export interface AgentNode {
  id: string
  sessionId: string
  parentId: string | null
  description: string
  prompt: string
  status: AgentStatus
  startTime: number
  endTime?: number
  agentType?: string
}

export interface JobNode {
  id: string
  name: string
  intent: string
  cwd: string
  status: string
  detail: string
  inFlight: { tasks: number; queued: number; kinds: string[] }
  tokens: number
  createdAt: string
  updatedAt: string
  suggestedReply?: string
  sessionId?: string
  timeline: TimelineEntry[]
}

export interface TimelineEntry {
  at: string
  state: string
  detail: string
  text?: string
}

export interface GraphState {
  sessions: SessionInfo[]
  agents: AgentNode[]
  jobs: JobNode[]
}

export type WsMessage =
  | { type: 'snapshot'; data: GraphState }
  | { type: 'session_update'; data: SessionInfo }
  | { type: 'session_removed'; id: string }
  | { type: 'agent_update'; data: AgentNode }
  | { type: 'job_update'; data: JobNode }
  | { type: 'job_removed'; id: string }

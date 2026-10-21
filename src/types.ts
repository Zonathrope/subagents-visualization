/** `stopped` is derived in the UI: still running per its transcript, but its session ended. */
export type AgentStatus = 'running' | 'completed' | 'failed' | 'stopped'

/** Mirrors src-tauri/src/stats.rs. Each API response is counted once. */
export interface TokenTotals {
  input: number
  output: number
  cacheWrite5m: number
  cacheWrite1h: number
  cacheRead: number
}

export interface TranscriptStats {
  tokens: TokenTotals
  /** Context size of the latest response. */
  contextTokens: number
  model?: string
  /** Newest record timestamp (ms); 0 if none seen. */
  lastActivity: number
  /** Edited files relative to the repo root (worktree prefix stripped). */
  editedFiles: string[]
}

export interface SessionInfo extends Record<string, unknown> {
  id: string
  pid: string
  cwd: string
  status: string
  startedAt: number
  version: string
  kind: string
  transcriptPath?: string
}

export interface AgentNode extends Record<string, unknown> {
  id: string
  sessionId: string
  parentId: string | null
  description: string
  prompt: string
  status: AgentStatus
  startTime: number
  endTime?: number
  agentType?: string
  transcriptPath?: string
  stats?: TranscriptStats
}

export interface JobNode extends Record<string, unknown> {
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
  resumeSessionId?: string
  timeline: TimelineEntry[]
}

export interface TimelineEntry {
  at: string
  state: string
  detail: string
  text?: string
}

/** What a session's transcript says about it, including sessions that have ended. */
export interface SessionSummary extends Record<string, unknown> {
  id: string
  cwd: string
  lastActivity: number
  transcriptPath: string
  stats?: TranscriptStats
}

export interface GraphState {
  sessions: SessionInfo[]
  agents: AgentNode[]
  jobs: JobNode[]
  summaries: SessionSummary[]
  /** False until the backend's startup scan has finished. */
  ready: boolean
}

export type WsMessage =
  | { type: 'snapshot'; data: GraphState }
  | { type: 'session_update'; data: SessionInfo }
  | { type: 'session_removed'; id: string }
  | { type: 'agent_update'; data: AgentNode }
  | { type: 'job_update'; data: JobNode }
  | { type: 'job_removed'; id: string }
  | { type: 'session_summary'; data: SessionSummary }
  | { type: 'ready' }
  | { type: 'transcript_changed'; path: string }

// ── Transcript detail view (mirrors src-tauri/src/transcript.rs) ──────────────

export interface DiffHunk {
  oldStart: number
  oldLines: number
  newStart: number
  newLines: number
  lines: string[]
}

export interface ToolCall {
  kind: 'tool'
  id: string
  name: string
  at: string
  input: Record<string, unknown>
  result: { text: string; isError: boolean } | null
  patch: DiffHunk[] | null
}

export type TranscriptItem =
  | { kind: 'user'; text: string; at: string }
  | { kind: 'assistant'; text: string; at: string }
  | { kind: 'thinking'; text: string; at: string }
  | { kind: 'notice'; text: string; at: string }
  | ToolCall

export interface TranscriptView {
  items: TranscriptItem[]
  truncated: boolean
}

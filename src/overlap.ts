import type { GraphState } from './types'

export interface OverlapActor {
  label: string
  /** Graph node to open: `agent-…` or `session-…`/`past-…`. */
  nodeId: string
}

export interface FileOverlap {
  file: string
  actors: OverlapActor[]
}

export interface Overlaps {
  /** Agent id → files it shares with another agent or the main session. */
  byAgent: Map<string, string[]>
  /** Session id → overlapping files with who touched them. */
  bySession: Map<string, FileOverlap[]>
}

/**
 * Files edited by more than one actor (agent or the main session) within a session.
 * Paths arrive repo-relative from the backend, so worktree copies of a file compare equal.
 */
export function computeOverlaps(state: GraphState): Overlaps {
  const perSession = new Map<string, Map<string, OverlapActor[]>>()
  const touch = (sessionId: string, file: string, actor: OverlapActor) => {
    const files = perSession.get(sessionId) ?? new Map<string, OverlapActor[]>()
    perSession.set(sessionId, files)
    const actors = files.get(file) ?? []
    if (!actors.some(a => a.nodeId === actor.nodeId)) actors.push(actor)
    files.set(file, actors)
  }

  for (const a of state.agents) {
    for (const f of a.stats?.editedFiles ?? []) {
      touch(a.sessionId, f, { label: a.description, nodeId: `agent-${a.id}` })
    }
  }
  const live = new Set(state.sessions.map(s => s.id))
  for (const s of state.summaries) {
    const nodeId = live.has(s.id) ? `session-${s.id}` : `past-${s.id}`
    for (const f of s.stats?.editedFiles ?? []) touch(s.id, f, { label: 'Main session', nodeId })
  }

  const byAgent = new Map<string, string[]>()
  const bySession = new Map<string, FileOverlap[]>()
  for (const [sessionId, files] of perSession) {
    const shared = [...files].filter(([, actors]) => actors.length > 1)
    if (shared.length === 0) continue
    bySession.set(
      sessionId,
      shared.map(([file, actors]) => ({ file, actors })).sort((a, b) => a.file.localeCompare(b.file)),
    )
    for (const [file, actors] of shared) {
      for (const actor of actors) {
        if (!actor.nodeId.startsWith('agent-')) continue
        const id = actor.nodeId.slice('agent-'.length)
        byAgent.set(id, [...(byAgent.get(id) ?? []), file])
      }
    }
  }
  return { byAgent, bySession }
}

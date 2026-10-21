import { useCallback, useEffect, useRef, useState } from 'react'
import type { AgentNode, GraphState } from './types'
import { playUrgent } from './sound'

export type AttentionKind = 'waiting' | 'blocked' | 'failed' | 'stuck' | 'finished'

export interface AttentionItem {
  /** Stable per situation, e.g. `waiting-<sessionId>`; re-raising replaces the item. */
  key: string
  kind: AttentionKind
  /** Graph node the item points at. */
  nodeId: string
  title: string
  detail?: string
  /** For blocked jobs: Claude's suggested answer. */
  suggestedReply?: string
  at: number
}

/** Everything except a successful finish plays the "needs you" sound. */
export const URGENT: ReadonlySet<AttentionKind> = new Set(['waiting', 'blocked', 'failed', 'stuck'])

interface Seen {
  sessionStatus: Map<string, string>
  agentStatus: Map<string, string>
  blockedJobs: Set<string>
  stuckAgents: Set<string>
}

function lastActivity(a: AgentNode): number {
  return Math.max(a.stats?.lastActivity ?? 0, a.startTime)
}

function shortCwd(cwd: string) {
  return cwd.split(/[/\\]/).filter(Boolean).slice(-2).join('/')
}

/**
 * The attention inbox. Items come only from transitions observed while the app runs,
 * so starting the app doesn't flood it with old events; jobs already blocked at startup
 * are listed, but silently. Items clear when their situation resolves.
 */
export function useAttention(state: GraphState, now: number, stuckMinutes: number, sounds: boolean) {
  const [items, setItems] = useState<AttentionItem[]>([])
  const seen = useRef<Seen | null>(null)

  useEffect(() => {
    // While the backend is still loading history, agents can appear "running" and then
    // finish (spawn and completion may sit in different transcripts). Wait for the scan
    // to end and take that as the baseline.
    if (!state.ready) return
    const stuckMs = stuckMinutes * 60_000
    const prev = seen.current
    const initial = prev === null
    const raised: AttentionItem[] = []
    const resolved = new Set<string>()
    const t = Date.now()

    const next: Seen = {
      sessionStatus: new Map(state.sessions.map(s => [s.id, s.status])),
      agentStatus: new Map(state.agents.map(a => [a.id, a.status])),
      blockedJobs: new Set(state.jobs.filter(j => j.status === 'blocked').map(j => j.id)),
      stuckAgents: new Set(
        state.agents.filter(a => a.status === 'running' && now - lastActivity(a) > stuckMs).map(a => a.id),
      ),
    }

    for (const s of state.sessions) {
      const key = `waiting-${s.id}`
      if (s.status !== 'idle') resolved.add(key)
      else if (prev?.sessionStatus.get(s.id) === 'busy') {
        raised.push({ key, kind: 'waiting', nodeId: `session-${s.id}`, title: shortCwd(s.cwd), detail: 'Waiting for your next prompt', at: t })
      }
    }
    for (const id of prev?.sessionStatus.keys() ?? []) {
      if (!next.sessionStatus.has(id)) resolved.add(`waiting-${id}`)
    }

    for (const j of state.jobs) {
      const key = `blocked-${j.id}`
      if (j.status !== 'blocked') resolved.add(key)
      else if (!prev || !prev.blockedJobs.has(j.id)) {
        raised.push({
          key, kind: 'blocked', nodeId: `job-${j.id}`, title: j.name,
          detail: j.detail || 'Needs an answer', suggestedReply: j.suggestedReply, at: t,
        })
      }
    }

    for (const a of state.agents) {
      const before = prev?.agentStatus.get(a.id)
      if (before === 'running' && a.status === 'failed') {
        raised.push({ key: `failed-${a.id}`, kind: 'failed', nodeId: `agent-${a.id}`, title: a.description, detail: 'Agent failed', at: t })
      } else if (before === 'running' && a.status === 'completed') {
        raised.push({ key: `finished-${a.id}`, kind: 'finished', nodeId: `agent-${a.id}`, title: a.description, detail: 'Agent finished', at: t })
      }
      const stuckKey = `stuck-${a.id}`
      if (!next.stuckAgents.has(a.id)) resolved.add(stuckKey)
      else if (!prev?.stuckAgents.has(a.id)) {
        const mins = Math.round((now - lastActivity(a)) / 60_000)
        raised.push({
          key: stuckKey, kind: 'stuck', nodeId: `agent-${a.id}`, title: a.description,
          detail: `No activity for ${mins} min (long tool call, or waiting for permission?)`, at: t,
        })
      }
    }

    seen.current = next
    if (raised.length === 0 && resolved.size === 0) return

    setItems(current => {
      const raisedKeys = new Set(raised.map(r => r.key))
      const kept = current.filter(i => !resolved.has(i.key) && !raisedKeys.has(i.key))
      return [...raised, ...kept]
    })
    // Startup listings are silent; only new situations chime.
    if (sounds && !initial && raised.some(r => URGENT.has(r.kind))) playUrgent()
  }, [state, now, stuckMinutes, sounds])

  const dismiss = useCallback((key: string) => setItems(cur => cur.filter(i => i.key !== key)), [])
  /** Opening a node acknowledges everything pointing at it. */
  const acknowledge = useCallback((nodeId: string) => setItems(cur => cur.filter(i => i.nodeId !== nodeId)), [])

  return { items, dismiss, acknowledge }
}

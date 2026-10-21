import type { GraphState } from './types'

/** Finished agents fade after this long, so busy sessions stay readable. */
export const DIM_AFTER_MS = 5 * 60 * 1000

/**
 * Ids of sessions that are still running: interactive sessions under `sessions/`, plus
 * background sessions (jobs), whose transcripts may be named after either id.
 */
export function liveSessionIds(state: GraphState): Set<string> {
  const ids = new Set(state.sessions.map(s => s.id))
  for (const j of state.jobs) {
    if (j.sessionId) ids.add(j.sessionId)
    if (j.resumeSessionId) ids.add(j.resumeSessionId)
  }
  return ids
}

/**
 * An agent whose session has ended never gets a completion record, so its transcript
 * still says "running". Show those as `stopped` instead.
 */
export function withDerivedStatus(state: GraphState): GraphState {
  const live = liveSessionIds(state)
  return {
    ...state,
    agents: state.agents.map(a =>
      a.status === 'running' && !live.has(a.sessionId) ? { ...a, status: 'stopped' } : a,
    ),
  }
}

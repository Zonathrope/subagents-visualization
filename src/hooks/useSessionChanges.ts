import { useEffect, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import type { ToolCall, WsMessage } from '../types'

/** Mirrors `SessionChange` in src-tauri/src/transcript.rs. */
export interface SessionChange {
  /** Agent tool_use id; absent for the main session's own edits. */
  agentId?: string
  actor: string
  /** Repo-relative path (worktree copies group with the original). */
  file: string
  call: ToolCall
}

const RELOAD_THROTTLE_MS = 600

/**
 * Every edit in a session (its own and all its agents'), reloaded when any of the given
 * transcripts changes. Disabled when `sessionId` is undefined.
 */
export function useSessionChanges(sessionId: string | undefined, watchPaths: string[]) {
  const [changes, setChanges] = useState<SessionChange[] | null>(null)
  const watchKey = watchPaths.join('\n')

  useEffect(() => {
    setChanges(null)
    if (!sessionId) return
    const watched = new Set(watchKey.split('\n'))
    let disposed = false
    let timer: ReturnType<typeof setTimeout> | null = null

    const load = () =>
      invoke<SessionChange[]>('load_session_changes', { sessionId })
        .then(c => !disposed && setChanges(c))
        .catch(() => !disposed && setChanges([]))

    void load()
    const sub = listen<WsMessage>('graph-event', ev => {
      if (ev.payload.type !== 'transcript_changed' || !watched.has(ev.payload.path) || timer) return
      timer = setTimeout(() => {
        timer = null
        void load()
      }, RELOAD_THROTTLE_MS)
    })
    return () => {
      disposed = true
      if (timer) clearTimeout(timer)
      void sub.then(unlisten => unlisten())
    }
  }, [sessionId, watchKey])

  return changes
}

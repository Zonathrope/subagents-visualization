import { useEffect, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import type { GraphState, WsMessage } from '../types'

function applyMessage(state: GraphState, msg: WsMessage): GraphState {
  switch (msg.type) {
    case 'snapshot':
      return msg.data

    case 'session_update': {
      const sessions = state.sessions.filter(s => s.id !== msg.data.id)
      return { ...state, sessions: [...sessions, msg.data] }
    }

    case 'session_removed':
      return { ...state, sessions: state.sessions.filter(s => s.id !== msg.id) }

    case 'agent_update': {
      const agents = state.agents.filter(a => a.id !== msg.data.id)
      return { ...state, agents: [...agents, msg.data] }
    }

    case 'job_update': {
      const jobs = state.jobs.filter(j => j.id !== msg.data.id)
      return { ...state, jobs: [...jobs, msg.data] }
    }

    case 'job_removed':
      return { ...state, jobs: state.jobs.filter(j => j.id !== msg.id) }

    default:
      return state
  }
}

const EMPTY: GraphState = { sessions: [], agents: [], jobs: [] }

export function useAgentStream() {
  const [graphState, setGraphState] = useState<GraphState>(EMPTY)
  const [connected, setConnected] = useState(false)

  useEffect(() => {
    let disposed = false
    // Events that arrive before the snapshot are replayed on top of it (all updates are idempotent).
    let queued: WsMessage[] | null = []

    const unlistenPromise = listen<WsMessage>('graph-event', ev => {
      if (queued) queued.push(ev.payload)
      else setGraphState(prev => applyMessage(prev, ev.payload))
    })

    unlistenPromise
      .then(() => invoke<GraphState>('snapshot'))
      .then(snapshot => {
        if (disposed) return
        const pending = queued ?? []
        queued = null
        setGraphState(pending.reduce(applyMessage, snapshot))
        setConnected(true)
      })
      .catch(err => console.error('failed to load snapshot', err))

    return () => {
      disposed = true
      unlistenPromise.then(unlisten => unlisten())
    }
  }, [])

  return { graphState, connected }
}

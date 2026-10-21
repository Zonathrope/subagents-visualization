import { useEffect, useRef, useState } from 'react'
import type { GraphState, WsMessage } from '../types'

const WS_URL = `ws://localhost:3001`

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
  const ws = useRef<WebSocket | null>(null)
  const reconnectTimer = useRef<ReturnType<typeof setTimeout> | null>(null)

  useEffect(() => {
    function connect() {
      const socket = new WebSocket(WS_URL)
      ws.current = socket

      socket.onopen = () => setConnected(true)

      socket.onmessage = ev => {
        try {
          const msg: WsMessage = JSON.parse(ev.data)
          setGraphState(prev => applyMessage(prev, msg))
        } catch { /* skip bad frames */ }
      }

      socket.onclose = () => {
        setConnected(false)
        reconnectTimer.current = setTimeout(connect, 2000)
      }

      socket.onerror = () => socket.close()
    }

    connect()

    return () => {
      if (reconnectTimer.current) clearTimeout(reconnectTimer.current)
      ws.current?.close()
    }
  }, [])

  return { graphState, connected }
}

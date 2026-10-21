import { useMemo } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { useAgentStream } from './hooks/useAgentStream'
import { useMirroredInbox } from './inboxSync'
import { URGENT } from './attention'
import { withDerivedStatus } from './liveness'
import './App.css'

/** Compact always-on-top view: what needs you, and what's running. */
export default function MiniApp() {
  const { graphState: raw } = useAgentStream()
  const state = useMemo(() => withDerivedStatus(raw), [raw])
  const { items, dismiss } = useMirroredInbox()
  const running = state.agents.filter(a => a.status === 'running').sort((a, b) => a.startTime - b.startTime)
  const open = (nodeId: string) => void invoke('focus_node', { nodeId })

  return (
    <div className="mini">
      <div className="mini-section">
        Needs you{items.length > 0 && <span className="inbox-count">{items.length}</span>}
      </div>
      {items.length === 0 && <div className="inbox-empty">All clear.</div>}
      {items.map(item => (
        <div
          key={item.key}
          className={`mini-item inbox-${item.kind} ${URGENT.has(item.kind) ? 'inbox-urgent' : ''}`}
          onClick={() => open(item.nodeId)}
          title={item.detail}
        >
          <span className="mini-title">{item.title}</span>
          <span className="inbox-kind">{item.kind}</span>
          <button
            className="inbox-dismiss"
            onClick={e => {
              e.stopPropagation()
              dismiss(item.key)
            }}
          >
            ×
          </button>
        </div>
      ))}

      <div className="mini-section">
        Running{running.length > 0 && <span className="tab-count">{running.length}</span>}
      </div>
      {running.length === 0 && <div className="inbox-empty">No agents running.</div>}
      {running.map(a => (
        <div key={a.id} className="mini-item" onClick={() => open(`agent-${a.id}`)} title={a.description}>
          <span className="mini-dot" />
          <span className="mini-title">{a.description}</span>
          <span className="mini-meta">{minutes(a.startTime)}</span>
        </div>
      ))}
    </div>
  )
}

function minutes(start: number) {
  const m = Math.floor((Date.now() - start) / 60_000)
  return m < 1 ? '<1m' : `${m}m`
}

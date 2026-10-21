import { useCallback, useEffect, useMemo, useState } from 'react'
import { useAgentStream } from './hooks/useAgentStream'
import { useTerminals } from './hooks/useTerminals'
import type { HostedTerminal } from './hooks/useTerminals'
import AgentGraph from './components/AgentGraph'
import DetailPanel from './components/DetailPanel'
import TerminalDock from './components/TerminalDock'
import type { DetailTarget, PromptRoute } from './components/DetailPanel'
import type { GraphState } from './types'
import { liveSessionIds, withDerivedStatus } from './liveness'
import './App.css'

interface Selection {
  target: DetailTarget
  /** The top-level session that owns this node; prompts go through its terminal. */
  sessionId: string
  /** Subagent id (from `agent-<id>.jsonl`), the address SendMessage uses. */
  agentId?: string
  /** False once the owning session has ended; ended sessions can't be prompted. */
  live: boolean
}

/** Resolves a graph node id (`agent-…`, `session-…`, `past-…`) to what the detail panel shows. */
function resolveSelection(state: GraphState, nodeId: string | null): Selection | null {
  const live = liveSessionIds(state)
  if (nodeId?.startsWith('agent-')) {
    const a = state.agents.find(a => `agent-${a.id}` === nodeId)
    if (!a) return null
    const file = a.transcriptPath?.split(/[/\\]/).pop()
    return {
      target: {
        kind: 'agent',
        title: a.description,
        subtitle: a.agentType,
        status: a.status,
        transcriptPath: a.transcriptPath,
      },
      sessionId: a.sessionId,
      agentId: file?.replace(/^agent-/, '').replace(/\.jsonl$/, ''),
      live: live.has(a.sessionId),
    }
  }
  if (nodeId?.startsWith('session-')) {
    const s = state.sessions.find(s => `session-${s.id}` === nodeId)
    if (!s) return null
    return {
      target: {
        kind: 'session',
        title: s.cwd,
        subtitle: `v${s.version} · ${s.kind}`,
        status: s.status,
        transcriptPath: s.transcriptPath ?? state.summaries.find(x => x.id === s.id)?.transcriptPath,
      },
      sessionId: s.id,
      live: true,
    }
  }
  if (nodeId?.startsWith('past-')) {
    const s = state.summaries.find(s => `past-${s.id}` === nodeId)
    if (!s) return null
    return {
      target: {
        kind: 'session',
        title: s.cwd || s.id,
        subtitle: s.lastActivity ? `last active ${new Date(s.lastActivity).toLocaleString()}` : undefined,
        status: 'ended',
        transcriptPath: s.transcriptPath,
      },
      sessionId: s.id,
      live: false,
    }
  }
  return null
}

function promptRoute(
  sel: Selection,
  terminal: HostedTerminal | undefined,
  send: (id: number, text: string) => Promise<void>,
): PromptRoute {
  if (!sel.live) return { unavailable: 'View only: this session has ended.' }
  if (!terminal) {
    return {
      unavailable:
        sel.target.kind === 'agent'
          ? "View only: this agent's session runs in another terminal. Start sessions with “+ New session” to message their agents here."
          : 'View only: this session runs in another terminal. Start sessions with “+ New session” to prompt them here.',
    }
  }
  if (sel.target.kind === 'session') {
    return { send: text => send(terminal.id, text), hint: 'Typed into the session’s terminal' }
  }
  if (!sel.agentId) return { unavailable: 'Waiting for this agent’s transcript…' }
  const agentId = sel.agentId
  return {
    send: text =>
      send(
        terminal.id,
        `Use SendMessage to send the following message to agent ${agentId} (to: "${agentId}"), ` +
          `then let it continue:\n\n${text}`,
      ),
    hint: 'Delivered by the parent session via SendMessage',
  }
}

const HISTORY_KEY = 'subagents.showHistory'

function loadShowHistory(): boolean {
  try {
    return localStorage.getItem(HISTORY_KEY) === '1'
  } catch {
    return false
  }
}

/** Re-renders periodically so finished agents dim on schedule. */
function useNow(intervalMs: number) {
  const [now, setNow] = useState(() => Date.now())
  useEffect(() => {
    const t = setInterval(() => setNow(Date.now()), intervalMs)
    return () => clearInterval(t)
  }, [intervalMs])
  return now
}

export default function App() {
  const { graphState: rawState, connected } = useAgentStream()
  const graphState = useMemo(() => withDerivedStatus(rawState), [rawState])
  const now = useNow(30_000)
  const [showHistory, setShowHistory] = useState(loadShowHistory)
  const toggleHistory = () =>
    setShowHistory(on => {
      try {
        localStorage.setItem(HISTORY_KEY, on ? '0' : '1')
      } catch {
        /* preference just won't persist */
      }
      return !on
    })
  const terminals = useTerminals()
  const [dockOpen, setDockOpen] = useState(true)
  const [dockHeight, setDockHeight] = useState(320)
  const [selectedId, setSelectedId] = useState<string | null>(null)
  const close = useCallback(() => setSelectedId(null), [])
  const selection = resolveSelection(graphState, selectedId)
  const target = selection?.target ?? null

  const run = (action: () => Promise<unknown>) =>
    action()
      .then(() => setDockOpen(true))
      .catch(e => alert(`Could not start Claude Code: ${e}`))

  const activeSessions = graphState.sessions.filter(s => s.status === 'busy' || s.status === 'active').length
  const runningJobs = graphState.jobs.filter(j => j.status === 'running' || j.inFlight.tasks > 0).length
  const runningAgents = graphState.agents.filter(a => a.status === 'running').length

  return (
    <div className="app">
      <header className="header">
        <div className="header-left">
          <span className="logo">◎</span>
          <span className="title">Subagents</span>
        </div>
        <div className="header-stats">
          {activeSessions > 0 && (
            <Chip label="sessions" count={activeSessions} color="#3b82f6" />
          )}
          {runningJobs > 0 && (
            <Chip label="jobs" count={runningJobs} color="#06b6d4" pulse />
          )}
          {runningAgents > 0 && (
            <Chip label="agents" count={runningAgents} color="#a855f7" pulse />
          )}
        </div>
        <div className="header-actions">
          <button className="header-button" onClick={() => run(terminals.startSession)}>+ New session</button>
          <button
            className={`header-button ${showHistory ? 'header-button-on' : ''}`}
            aria-pressed={showHistory}
            title="Show sessions that ended in the last 24 hours, with their agents"
            onClick={toggleHistory}
          >
            History
          </button>
          {terminals.terminals.length > 0 && (
            <button className="header-button" onClick={() => setDockOpen(o => !o)}>
              {dockOpen ? 'Hide' : 'Show'} terminals ({terminals.terminals.length})
            </button>
          )}
        </div>
        <div className="header-right">
          <span className={`conn-dot ${connected ? 'conn-on' : 'conn-off'}`} />
          <span className="conn-label">{connected ? 'live' : 'loading…'}</span>
        </div>
      </header>

      <div className="workspace">
      <div className="body">
        <AgentGraph
          state={graphState}
          showHistory={showHistory}
          now={now}
          selectedId={target ? selectedId : null}
          onSelect={setSelectedId}
        />

        {target ? (
          <DetailPanel
            key={selectedId}
            target={target}
            prompt={promptRoute(selection!, terminals.forSession(selection!.sessionId), terminals.send)}
            onClose={close}
          />
        ) : graphState.jobs.length > 0 && (
          <aside className="sidebar">
            <div className="sidebar-title">Background jobs</div>
            {graphState.jobs.map(job => (
              <div key={job.id} className="sidebar-job">
                <div className="sidebar-job-name">
                  {job.name}
                  <button
                    className="sidebar-attach"
                    title="Open this background session in a terminal (claude attach)"
                    onClick={() => run(() => terminals.attach(job))}
                  >
                    Attach
                  </button>
                </div>
                <div className="sidebar-job-status">
                  <span
                    className="status-badge"
                    style={{ background: statusBg(job.status) }}
                  >
                    {job.status}
                  </span>
                  {job.inFlight.tasks > 0 && (
                    <span className="status-badge" style={{ background: '#a855f7' }}>
                      {job.inFlight.tasks} in-flight
                    </span>
                  )}
                </div>
                {job.detail && (
                  <div className="sidebar-job-detail">{job.detail.slice(0, 120)}</div>
                )}
                {job.tokens > 0 && (
                  <div className="sidebar-job-tokens">{fmtK(job.tokens)} tokens</div>
                )}
                {job.timeline.length > 0 && (
                  <div className="timeline">
                    {job.timeline.slice(-4).map((e, i) => (
                      <div key={i} className="timeline-entry">
                        <span className={`tl-dot tl-${e.state}`} />
                        <span className="tl-text">{e.detail.slice(0, 70)}</span>
                      </div>
                    ))}
                  </div>
                )}
              </div>
            ))}
          </aside>
        )}
      </div>
      {dockOpen && terminals.terminals.length > 0 && (
        <TerminalDock
          terminals={terminals.terminals}
          activeId={terminals.activeId}
          height={dockHeight}
          onHeightChange={setDockHeight}
          onSelect={terminals.setActiveId}
          onClose={terminals.close}
          onHide={() => setDockOpen(false)}
        />
      )}
      </div>
    </div>
  )
}

function Chip({ label, count, color, pulse }: { label: string; count: number; color: string; pulse?: boolean }) {
  return (
    <div className="chip" style={{ borderColor: color }}>
      <span
        className="chip-dot"
        style={{
          background: color,
          animation: pulse ? 'pulse 1.4s ease-in-out infinite' : 'none',
        }}
      />
      <span style={{ color }}>{count}</span>
      <span className="chip-label">{label}</span>
    </div>
  )
}

function statusBg(s: string) {
  const m: Record<string, string> = {
    running: '#16a34a33',
    done: '#64748b33',
    blocked: '#d9770633',
    failed: '#dc262633',
  }
  return m[s] ?? '#64748b33'
}

function fmtK(n: number) {
  return n >= 1000 ? `${(n / 1000).toFixed(1)}k` : String(n)
}

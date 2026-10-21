import { useAgentStream } from './hooks/useAgentStream'
import AgentGraph from './components/AgentGraph'
import './App.css'

export default function App() {
  const { graphState, connected } = useAgentStream()

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
        <div className="header-right">
          <span className={`conn-dot ${connected ? 'conn-on' : 'conn-off'}`} />
          <span className="conn-label">{connected ? 'live' : 'loading…'}</span>
        </div>
      </header>

      <div className="body">
        <AgentGraph state={graphState} />

        {graphState.jobs.length > 0 && (
          <aside className="sidebar">
            <div className="sidebar-title">Background jobs</div>
            {graphState.jobs.map(job => (
              <div key={job.id} className="sidebar-job">
                <div className="sidebar-job-name">{job.name}</div>
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

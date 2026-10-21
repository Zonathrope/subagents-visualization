import { Handle, Position } from '@xyflow/react'
import type { AgentNode, JobNode, SessionInfo } from '../types'

function elapsed(startMs: number, endMs?: number): string {
  const ms = (endMs ?? Date.now()) - startMs
  if (ms < 1000) return `${ms}ms`
  if (ms < 60000) return `${(ms / 1000).toFixed(1)}s`
  return `${Math.floor(ms / 60000)}m ${Math.floor((ms % 60000) / 1000)}s`
}

function fmt(n: number) {
  if (n >= 1000) return `${(n / 1000).toFixed(1)}k`
  return String(n)
}

const STATUS_COLOR: Record<string, string> = {
  running: '#22c55e',
  active: '#22c55e',
  busy: '#22c55e',
  completed: '#64748b',
  idle: '#64748b',
  failed: '#ef4444',
  blocked: '#f59e0b',
  done: '#64748b',
  unknown: '#64748b',
}

function StatusDot({ status }: { status: string }) {
  const color = STATUS_COLOR[status] ?? '#64748b'
  const pulse = status === 'running' || status === 'busy' || status === 'active'
  return (
    <span
      style={{
        display: 'inline-block',
        width: 8,
        height: 8,
        borderRadius: '50%',
        background: color,
        flexShrink: 0,
        animation: pulse ? 'pulse 1.4s ease-in-out infinite' : 'none',
      }}
    />
  )
}

// ── Session node ──────────────────────────────────────────────────────────────

export function SessionNodeComponent({ data }: { data: SessionInfo }) {
  const short = data.cwd.split(/[/\\]/).slice(-2).join('/')
  return (
    <div className="node node-session">
      <Handle type="source" position={Position.Bottom} />
      <div className="node-header">
        <StatusDot status={data.status} />
        <span className="node-kind">SESSION</span>
      </div>
      <div className="node-title" title={data.cwd}>{short}</div>
      <div className="node-meta">
        <span>v{data.version}</span>
        <span className="dot">·</span>
        <span>{data.kind}</span>
      </div>
    </div>
  )
}

// ── Job node ──────────────────────────────────────────────────────────────────

export function JobNodeComponent({ data }: { data: JobNode }) {
  return (
    <div className="node node-job">
      <Handle type="target" position={Position.Top} />
      <Handle type="source" position={Position.Bottom} />
      <div className="node-header">
        <StatusDot status={data.status} />
        <span className="node-kind">JOB</span>
        {data.inFlight.tasks > 0 && (
          <span className="badge badge-tasks">{data.inFlight.tasks} in-flight</span>
        )}
      </div>
      <div className="node-title" title={data.name}>{data.name}</div>
      {data.detail && (
        <div className="node-detail" title={data.detail}>
          {data.detail.slice(0, 90)}{data.detail.length > 90 ? '…' : ''}
        </div>
      )}
      <div className="node-meta">
        {data.tokens > 0 && <span>{fmt(data.tokens)} tok</span>}
        {data.tokens > 0 && data.updatedAt && <span className="dot">·</span>}
        {data.updatedAt && (
          <span>{new Date(data.updatedAt).toLocaleTimeString()}</span>
        )}
      </div>
    </div>
  )
}

// ── Agent node ────────────────────────────────────────────────────────────────

export function AgentNodeComponent({ data }: { data: AgentNode }) {
  return (
    <div className="node node-agent">
      <Handle type="target" position={Position.Top} />
      <Handle type="source" position={Position.Bottom} />
      <div className="node-header">
        <StatusDot status={data.status} />
        <span className="node-kind">AGENT</span>
        {data.agentType && <span className="badge">{data.agentType}</span>}
      </div>
      <div className="node-title" title={data.description}>
        {data.description.slice(0, 80)}{data.description.length > 80 ? '…' : ''}
      </div>
      <div className="node-meta">
        <span>{elapsed(data.startTime, data.endTime)}</span>
        <span className="dot">·</span>
        <span>{data.status}</span>
      </div>
    </div>
  )
}

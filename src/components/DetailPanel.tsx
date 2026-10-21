import { useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { useTranscript } from '../hooks/useTranscript'
import type { AgentNode, ToolCall, TranscriptItem, TranscriptStats } from '../types'
import type { FileOverlap } from '../overlap'
import { estimateCost, formatUsd } from '../pricing'

export interface DetailTarget {
  kind: 'session' | 'agent'
  title: string
  subtitle?: string
  status: string
  transcriptPath?: string
  stats?: TranscriptStats
}

/** Whether, and how, the user can prompt the selected session or agent. */
export type PromptRoute =
  | { send: (text: string) => Promise<void>; hint: string }
  | { unavailable: string }

/** Session-only context: its agents for the timeline, and files edited by several actors. */
export interface SessionContext {
  agents: AgentNode[]
  overlaps: FileOverlap[]
}

const EDIT_TOOLS = new Set(['Edit', 'Write', 'MultiEdit', 'NotebookEdit'])

type Tab = 'activity' | 'changes' | 'timeline'

export default function DetailPanel({
  target,
  prompt,
  session,
  showCost,
  now,
  onStop,
  onOpenNode,
  onClose,
}: {
  target: DetailTarget
  prompt: PromptRoute
  session?: SessionContext
  showCost: boolean
  now: number
  /** Present only when stopping is possible (running agent in an app-hosted session). */
  onStop?: () => Promise<void>
  onOpenNode: (nodeId: string) => void
  onClose: () => void
}) {
  const { view, error } = useTranscript(target.transcriptPath)
  const [tab, setTab] = useState<Tab>('activity')
  const [stopState, setStopState] = useState<string | null>(null)

  useEffect(() => {
    // Escape also interrupts Claude in a terminal, so ignore it while typing anywhere.
    const onKey = (e: KeyboardEvent) => {
      const typing = (e.target as HTMLElement | null)?.closest?.('.xterm, textarea, input')
      if (e.key === 'Escape' && !typing) onClose()
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [onClose])

  const edits = useMemo(
    () => (view?.items ?? []).filter((i): i is ToolCall => i.kind === 'tool' && EDIT_TOOLS.has(i.name)),
    [view],
  )

  // Pin the agent's report to its parent once it's done. Current Claude Code hands it back
  // through the SubagentHandback tool; older transcripts end with a plain reply instead.
  const result = useMemo(() => {
    if (target.kind !== 'agent' || target.status === 'running' || !view) return undefined
    const items = [...view.items].reverse()
    const handback = items.find((i): i is ToolCall => i.kind === 'tool' && i.name === 'SubagentHandback')
    if (typeof handback?.input.message === 'string') return handback.input.message
    const last = items.find(i => i.kind === 'assistant')
    return last?.kind === 'assistant' ? last.text : undefined
  }, [target.kind, target.status, view])

  const stats = target.stats
  const cost = showCost ? estimateCost(stats) : undefined

  const stop = async () => {
    if (!onStop) return
    setStopState('Asking the parent session to stop it…')
    try {
      await onStop()
      setStopState('Stop requested')
    } catch (e) {
      setStopState(`Failed: ${e}`)
    }
  }

  return (
    <aside className="detail">
      <header className="detail-header">
        <div className="detail-heading">
          <span className={`detail-kind detail-kind-${target.kind}`}>{target.kind}</span>
          <span className="detail-status">{target.status}</span>
          {onStop && (
            <button className="detail-stop" onClick={() => void stop()} title="Ask the parent session to stop this agent">
              Stop
            </button>
          )}
          <button className="detail-close" onClick={onClose} title="Close (Esc)">×</button>
        </div>
        <div className="detail-title" title={target.title}>{target.title}</div>
        {target.subtitle && <div className="detail-subtitle">{target.subtitle}</div>}
        {stats && stats.tokens.output > 0 && (
          <div className="detail-subtitle">
            {fmtTokens(stats.contextTokens)} context · {fmtTokens(stats.tokens.output)} output
            {stats.model && ` · ${stats.model}`}
            {cost !== undefined && ` · ~${formatUsd(cost)} (estimate)`}
          </div>
        )}
        {stopState && <div className="detail-subtitle">{stopState}</div>}
        <nav className="detail-tabs">
          <button className={tab === 'activity' ? 'active' : ''} onClick={() => setTab('activity')}>
            Activity
          </button>
          <button className={tab === 'changes' ? 'active' : ''} onClick={() => setTab('changes')}>
            Changes{edits.length > 0 && <span className="tab-count">{edits.length}</span>}
            {session && session.overlaps.length > 0 && (
              <span className="tab-count tab-warn">⚠ {session.overlaps.length}</span>
            )}
          </button>
          {session && (
            <button className={tab === 'timeline' ? 'active' : ''} onClick={() => setTab('timeline')}>
              Timeline{session.agents.length > 0 && <span className="tab-count">{session.agents.length}</span>}
            </button>
          )}
        </nav>
      </header>

      {tab === 'timeline' && session ? (
        <Timeline agents={session.agents} now={now} onOpenNode={onOpenNode} />
      ) : !target.transcriptPath ? (
        <div className="detail-empty">No transcript found for this {target.kind} yet.</div>
      ) : error ? (
        <div className="detail-empty">Could not load transcript: {error}</div>
      ) : !view ? (
        <div className="detail-empty">Loading…</div>
      ) : tab === 'activity' ? (
        <Activity items={view.items} truncated={view.truncated} result={result} />
      ) : (
        <Changes edits={edits} overlaps={session?.overlaps ?? []} onOpenNode={onOpenNode} />
      )}

      <PromptBox route={prompt} placeholder={target.kind === 'agent' ? 'Message this agent…' : 'Prompt this session…'} />
    </aside>
  )
}

function fmtTokens(n: number) {
  return n >= 1000 ? `${(n / 1000).toFixed(1)}k` : String(n)
}

// ── Timeline ──────────────────────────────────────────────────────────────────

function Timeline({ agents, now, onOpenNode }: { agents: AgentNode[]; now: number; onOpenNode: (id: string) => void }) {
  if (agents.length === 0) return <div className="detail-empty">This session hasn't started any agents.</div>
  const sorted = [...agents].sort((a, b) => a.startTime - b.startTime)
  // A bar ends at its finish; a running one at "now"; a stopped one at its last activity.
  const endOf = (a: AgentNode) =>
    a.endTime ?? (a.status === 'running' ? now : Math.max(a.stats?.lastActivity ?? 0, a.startTime))
  const start = sorted[0].startTime
  // Span the agents' activity, not the idle time since, so bars stay readable.
  const end = Math.max(...sorted.map(endOf))
  const span = Math.max(end - start, 1)
  const pct = (t: number) => ((t - start) / span) * 100

  return (
    <div className="detail-body">
      <div className="timeline-axis">
        <span>{new Date(start).toLocaleTimeString()}</span>
        <span>{new Date(end).toLocaleTimeString()}</span>
      </div>
      {sorted.map(a => {
        const left = pct(a.startTime)
        const width = Math.max(pct(endOf(a)) - left, 0.5)
        return (
          <button key={a.id} className="timeline-row" onClick={() => onOpenNode(`agent-${a.id}`)} title={a.description}>
            <span className="timeline-label">{a.description}</span>
            <span className="timeline-track">
              <span className={`timeline-bar timeline-${a.status}`} style={{ left: `${left}%`, width: `${width}%` }} />
            </span>
          </button>
        )
      })}
    </div>
  )
}

/** Opens a changed file with the system default app (scripts are revealed instead). */
function OpenFileButton({ path }: { path: string }) {
  const [note, setNote] = useState<string | null>(null)
  const open = async (e: React.MouseEvent) => {
    e.stopPropagation()
    try {
      const outcome = await invoke<'opened' | 'revealed'>('open_file', { path })
      setNote(outcome === 'revealed' ? 'Shown in Explorer (scripts aren’t opened)' : null)
    } catch (err) {
      setNote(String(err))
    }
  }
  return (
    <>
      <button className="link-button open-file" onClick={open} title={`Open ${path}`}>Open</button>
      {note && <span className="open-note">{note}</span>}
    </>
  )
}

// ── Prompt box ────────────────────────────────────────────────────────────────

function PromptBox({ route, placeholder }: { route: PromptRoute; placeholder: string }) {
  const [text, setText] = useState('')
  const [status, setStatus] = useState<string | null>(null)

  if ('unavailable' in route) return <footer className="prompt prompt-unavailable">{route.unavailable}</footer>

  const submit = async () => {
    const value = text.trim()
    if (!value) return
    try {
      await route.send(value)
      setText('')
      setStatus('Sent')
    } catch (e) {
      setStatus(`Failed: ${e}`)
    }
    setTimeout(() => setStatus(null), 2500)
  }

  return (
    <footer className="prompt">
      <textarea
        value={text}
        placeholder={placeholder}
        rows={2}
        onChange={e => setText(e.target.value)}
        onKeyDown={e => {
          if (e.key === 'Enter' && !e.shiftKey) {
            e.preventDefault()
            void submit()
          }
        }}
      />
      <div className="prompt-meta">
        <span>{status ?? route.hint}</span>
        <button onClick={() => void submit()} disabled={!text.trim()}>Send ⏎</button>
      </div>
    </footer>
  )
}

// ── Activity ──────────────────────────────────────────────────────────────────

function Activity({ items, truncated, result }: { items: TranscriptItem[]; truncated: boolean; result?: string }) {
  const scrollRef = useRef<HTMLDivElement>(null)
  const pinnedToBottom = useRef(true)

  useLayoutEffect(() => {
    const el = scrollRef.current
    if (el && pinnedToBottom.current) el.scrollTop = el.scrollHeight
  }, [items])

  const onScroll = () => {
    const el = scrollRef.current
    if (el) pinnedToBottom.current = el.scrollHeight - el.scrollTop - el.clientHeight < 40
  }

  // Reasoning is often stored only as an encrypted signature; say so once instead of per turn.
  const hiddenReasoning = items.some(i => i.kind === 'thinking' && !i.text)
  const visible = items.filter(i => i.kind !== 'thinking' || i.text)

  return (
    <div className="detail-body" ref={scrollRef} onScroll={onScroll}>
      {result && (
        <div className="item item-result">
          <div className="item-label">Result</div>
          <Expandable text={result} limit={1200} />
        </div>
      )}
      {truncated && <div className="item-notice">Showing the most recent part of a long transcript.</div>}
      {hiddenReasoning && (
        <div className="item-thinking-hidden">Some reasoning was not saved to the transcript, only its signature.</div>
      )}
      {visible.length === 0 && <div className="detail-empty">Nothing here yet.</div>}
      {visible.map((item, i) => (
        <ItemView key={item.kind === 'tool' ? item.id : `${i}-${item.kind}`} item={item} first={i === 0} />
      ))}
    </div>
  )
}

function ItemView({ item, first }: { item: TranscriptItem; first: boolean }) {
  switch (item.kind) {
    case 'user':
      return (
        <div className="item item-user">
          <div className="item-label">{first ? 'Prompt' : 'User'} <Time at={item.at} /></div>
          <Expandable text={item.text} limit={700} />
        </div>
      )
    case 'assistant':
      return (
        <div className="item item-assistant">
          <div className="item-text">{item.text}</div>
        </div>
      )
    case 'thinking':
      return (
        <details className="item item-thinking">
          <summary>Reasoning</summary>
          <div className="item-text">{item.text}</div>
        </details>
      )
    case 'notice':
      return <div className="item-notice">{item.text}</div>
    case 'tool':
      return <ToolView call={item} />
  }
}

function ToolView({ call }: { call: ToolCall }) {
  const [open, setOpen] = useState(false)
  const isEdit = EDIT_TOOLS.has(call.name)
  const diff = isEdit ? diffFor(call) : null
  const state = !call.result ? 'pending' : call.result.isError ? 'error' : 'ok'

  return (
    <div className={`tool tool-${state}`}>
      <button className="tool-row" onClick={() => setOpen(o => !o)}>
        <span className="tool-caret">{open ? '▾' : '▸'}</span>
        <span className="tool-name">{call.name}</span>
        <span className="tool-summary" title={toolSummary(call)}>{toolSummary(call)}</span>
        {diff && <DiffStat diff={diff} />}
        <span className={`tool-state tool-state-${state}`}>
          {state === 'pending' ? '…' : state === 'error' ? '✗' : '✓'}
        </span>
      </button>
      {open && (
        <div className="tool-body">
          {diff ? <DiffView diff={diff} /> : <ToolInput call={call} />}
          {call.result && (!isEdit || call.result.isError) && (
            <pre className={`tool-result ${call.result.isError ? 'tool-result-error' : ''}`}>{call.result.text}</pre>
          )}
        </div>
      )}
    </div>
  )
}

function ToolInput({ call }: { call: ToolCall }) {
  const command = call.input.command
  if (typeof command === 'string') return <pre className="tool-input">{command}</pre>
  return <pre className="tool-input">{JSON.stringify(call.input, null, 2)}</pre>
}

// ── Changes ───────────────────────────────────────────────────────────────────

function Changes({
  edits,
  overlaps,
  onOpenNode,
}: {
  edits: ToolCall[]
  overlaps: FileOverlap[]
  onOpenNode: (nodeId: string) => void
}) {
  const files = useMemo(() => {
    const byFile = new Map<string, { adds: number; dels: number; count: number }>()
    for (const e of edits) {
      const d = diffFor(e)
      const f = byFile.get(d.file) ?? { adds: 0, dels: 0, count: 0 }
      byFile.set(d.file, { adds: f.adds + d.adds, dels: f.dels + d.dels, count: f.count + 1 })
    }
    return [...byFile.entries()]
  }, [edits])

  if (edits.length === 0 && overlaps.length === 0) return <div className="detail-empty">No file changes.</div>

  return (
    <div className="detail-body">
      {overlaps.length > 0 && (
        <div className="overlap-box">
          <div className="overlap-title">⚠ Edited by more than one agent. Check these before merging</div>
          {overlaps.map(o => (
            <div key={o.file} className="overlap-row">
              <span className="overlap-file" title={o.file}>{o.file}</span>
              <span className="overlap-actors">
                {o.actors.map(a => (
                  <button key={a.nodeId} className="link-button" onClick={() => onOpenNode(a.nodeId)} title={a.label}>
                    {a.label.length > 28 ? a.label.slice(0, 27) + '…' : a.label}
                  </button>
                ))}
              </span>
            </div>
          ))}
        </div>
      )}
      {edits.length > 0 && <div className="files-summary">
        {files.map(([file, s]) => (
          <div key={file} className="files-summary-row" title={file}>
            <span className="files-summary-path">{shortPath(file)}</span>
            <span className="diffstat-add">+{s.adds}</span>
            <span className="diffstat-del">−{s.dels}</span>
            <OpenFileButton path={file} />
          </div>
        ))}
      </div>}
      {edits.map(e => {
        const diff = diffFor(e)
        return (
          <div key={e.id} className="change">
            <div className="change-header" title={diff.file}>
              <span className="tool-name">{e.name}</span>
              <span className="change-path">{shortPath(diff.file)}</span>
              <DiffStat diff={diff} />
              <Time at={e.at} />
              <OpenFileButton path={diff.file} />
            </div>
            <DiffView diff={diff} />
            {e.result?.isError && <pre className="tool-result tool-result-error">{e.result.text}</pre>}
          </div>
        )
      })}
    </div>
  )
}

// ── Diffs ─────────────────────────────────────────────────────────────────────

type DiffLine = { type: 'add' | 'del' | 'ctx' | 'hunk'; text: string }
interface Diff {
  file: string
  lines: DiffLine[]
  adds: number
  dels: number
}

const str = (v: unknown) => (typeof v === 'string' ? v : '')
const splitLines = (s: string) => (s === '' ? [] : s.replace(/\n$/, '').split('\n'))

/** Prefers the recorded patch; falls back to the tool input while the call is in flight. */
function diffFor(call: ToolCall): Diff {
  const file = str(call.input.file_path) || str(call.input.notebook_path) || '(unknown file)'
  const lines: DiffLine[] = []

  if (call.patch) {
    for (const h of call.patch) {
      lines.push({ type: 'hunk', text: `@@ -${h.oldStart},${h.oldLines} +${h.newStart},${h.newLines} @@` })
      for (const l of h.lines) {
        const type = l[0] === '+' ? 'add' : l[0] === '-' ? 'del' : 'ctx'
        lines.push({ type, text: l.slice(1) })
      }
    }
  } else {
    const replace = (oldS: string, newS: string) => {
      for (const t of splitLines(oldS)) lines.push({ type: 'del', text: t })
      for (const t of splitLines(newS)) lines.push({ type: 'add', text: t })
    }
    if (call.name === 'Write') replace('', str(call.input.content))
    else if (call.name === 'NotebookEdit') replace('', str(call.input.new_source))
    else if (call.name === 'MultiEdit' && Array.isArray(call.input.edits)) {
      for (const e of call.input.edits as Record<string, unknown>[]) {
        lines.push({ type: 'hunk', text: '@@' })
        replace(str(e.old_string), str(e.new_string))
      }
    } else replace(str(call.input.old_string), str(call.input.new_string))
  }

  return {
    file,
    lines,
    adds: lines.filter(l => l.type === 'add').length,
    dels: lines.filter(l => l.type === 'del').length,
  }
}

function DiffView({ diff }: { diff: Diff }) {
  return (
    <pre className="diff">
      {diff.lines.map((l, i) => (
        <div key={i} className={`diff-${l.type}`}>
          <span className="diff-sign">{l.type === 'add' ? '+' : l.type === 'del' ? '−' : ' '}</span>
          {l.text}
        </div>
      ))}
    </pre>
  )
}

function DiffStat({ diff }: { diff: Diff }) {
  return (
    <span className="diffstat">
      <span className="diffstat-add">+{diff.adds}</span>
      <span className="diffstat-del">−{diff.dels}</span>
    </span>
  )
}

// ── Helpers ───────────────────────────────────────────────────────────────────

function toolSummary(call: ToolCall): string {
  const i = call.input
  const file = str(i.file_path) || str(i.notebook_path)
  if (file) return shortPath(file)
  const pick =
    str(i.command) || str(i.pattern) ||
    str(i.description) || str(i.url) || str(i.query) || str(i.prompt) ||
    Object.values(i).find((v): v is string => typeof v === 'string') || ''
  return pick.split('\n')[0]
}

function shortPath(p: string) {
  return p.split(/[/\\]/).slice(-3).join('/')
}

function Expandable({ text, limit }: { text: string; limit: number }) {
  const [full, setFull] = useState(false)
  if (text.length <= limit) return <div className="item-text">{text}</div>
  return (
    <div className="item-text">
      {full ? text : text.slice(0, limit) + '…'}
      <button className="link-button" onClick={() => setFull(f => !f)}>
        {full ? 'show less' : 'show more'}
      </button>
    </div>
  )
}

function Time({ at }: { at: string }) {
  if (!at) return null
  const d = new Date(at)
  return isNaN(d.getTime()) ? null : <span className="item-time">{d.toLocaleTimeString()}</span>
}

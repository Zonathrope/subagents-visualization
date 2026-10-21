import { useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react'
import { useTranscript } from '../hooks/useTranscript'
import type { ToolCall, TranscriptItem } from '../types'

export interface DetailTarget {
  kind: 'session' | 'agent'
  title: string
  subtitle?: string
  status: string
  transcriptPath?: string
}

const EDIT_TOOLS = new Set(['Edit', 'Write', 'MultiEdit', 'NotebookEdit'])

export default function DetailPanel({ target, onClose }: { target: DetailTarget; onClose: () => void }) {
  const { view, error } = useTranscript(target.transcriptPath)
  const [tab, setTab] = useState<'activity' | 'changes'>('activity')

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === 'Escape' && onClose()
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [onClose])

  const edits = useMemo(
    () => (view?.items ?? []).filter((i): i is ToolCall => i.kind === 'tool' && EDIT_TOOLS.has(i.name)),
    [view],
  )

  return (
    <aside className="detail">
      <header className="detail-header">
        <div className="detail-heading">
          <span className={`detail-kind detail-kind-${target.kind}`}>{target.kind}</span>
          <span className="detail-status">{target.status}</span>
          <button className="detail-close" onClick={onClose} title="Close (Esc)">×</button>
        </div>
        <div className="detail-title" title={target.title}>{target.title}</div>
        {target.subtitle && <div className="detail-subtitle">{target.subtitle}</div>}
        <nav className="detail-tabs">
          <button className={tab === 'activity' ? 'active' : ''} onClick={() => setTab('activity')}>
            Activity
          </button>
          <button className={tab === 'changes' ? 'active' : ''} onClick={() => setTab('changes')}>
            Changes{edits.length > 0 && <span className="tab-count">{edits.length}</span>}
          </button>
        </nav>
      </header>

      {!target.transcriptPath ? (
        <div className="detail-empty">No transcript found for this {target.kind} yet.</div>
      ) : error ? (
        <div className="detail-empty">Could not load transcript: {error}</div>
      ) : !view ? (
        <div className="detail-empty">Loading…</div>
      ) : tab === 'activity' ? (
        <Activity items={view.items} truncated={view.truncated} />
      ) : (
        <Changes edits={edits} />
      )}
    </aside>
  )
}

// ── Activity ──────────────────────────────────────────────────────────────────

function Activity({ items, truncated }: { items: TranscriptItem[]; truncated: boolean }) {
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

function Changes({ edits }: { edits: ToolCall[] }) {
  const files = useMemo(() => {
    const byFile = new Map<string, { adds: number; dels: number; count: number }>()
    for (const e of edits) {
      const d = diffFor(e)
      const f = byFile.get(d.file) ?? { adds: 0, dels: 0, count: 0 }
      byFile.set(d.file, { adds: f.adds + d.adds, dels: f.dels + d.dels, count: f.count + 1 })
    }
    return [...byFile.entries()]
  }, [edits])

  if (edits.length === 0) return <div className="detail-empty">No file changes.</div>

  return (
    <div className="detail-body">
      <div className="files-summary">
        {files.map(([file, s]) => (
          <div key={file} className="files-summary-row" title={file}>
            <span className="files-summary-path">{shortPath(file)}</span>
            <span className="diffstat-add">+{s.adds}</span>
            <span className="diffstat-del">−{s.dels}</span>
          </div>
        ))}
      </div>
      {edits.map(e => {
        const diff = diffFor(e)
        return (
          <div key={e.id} className="change">
            <div className="change-header" title={diff.file}>
              <span className="tool-name">{e.name}</span>
              <span className="change-path">{shortPath(diff.file)}</span>
              <DiffStat diff={diff} />
              <Time at={e.at} />
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

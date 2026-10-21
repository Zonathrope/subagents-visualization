import { useEffect, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import type { GraphState } from '../types'

/** Mirrors `SearchHit` in src-tauri/src/search.rs. */
interface SearchHit {
  source: { kind: 'session'; id: string } | { kind: 'agent'; id: string }
  at: string
  what: string
  snippet: string
}

const DEBOUNCE_MS = 300

/** Search results; shown in the sidebar while the header search box has a query. */
export default function SearchResults({
  query,
  state,
  onOpen,
}: {
  query: string
  state: GraphState
  /** Opens a node; `past` asks to show history first (the node belongs to an ended session). */
  onOpen: (nodeId: string, past: boolean) => void
}) {
  const [hits, setHits] = useState<SearchHit[] | null>(null)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    setHits(null)
    setError(null)
    if (query.trim().length < 2) return
    let cancelled = false
    const t = setTimeout(() => {
      invoke<SearchHit[]>('search_transcripts', { query })
        .then(h => !cancelled && setHits(h))
        .catch(e => !cancelled && setError(String(e)))
    }, DEBOUNCE_MS)
    return () => {
      cancelled = true
      clearTimeout(t)
    }
  }, [query])

  const live = new Set(state.sessions.map(s => s.id))
  const describe = (hit: SearchHit) => {
    if (hit.source.kind === 'agent') {
      const a = state.agents.find(a => a.id === hit.source.id)
      return {
        label: a?.description ?? 'Agent',
        nodeId: `agent-${hit.source.id}`,
        past: !!a && !live.has(a.sessionId),
      }
    }
    const id = hit.source.id
    const cwd = state.sessions.find(s => s.id === id)?.cwd ?? state.summaries.find(s => s.id === id)?.cwd ?? id
    const isLive = live.has(id)
    return {
      label: cwd.split(/[/\\]/).filter(Boolean).slice(-2).join('/'),
      nodeId: isLive ? `session-${id}` : `past-${id}`,
      past: !isLive,
    }
  }

  return (
    <aside className="sidebar search-results">
      <div className="sidebar-title">
        Search{hits && <span className="tab-count">{hits.length === 200 ? '200+' : hits.length}</span>}
      </div>
      {query.trim().length < 2 && <div className="inbox-empty">Type at least 2 characters.</div>}
      {error && <div className="inbox-empty">Search failed: {error}</div>}
      {!hits && !error && query.trim().length >= 2 && <div className="inbox-empty">Searching…</div>}
      {hits?.length === 0 && <div className="inbox-empty">No matches in the last 24 hours of transcripts.</div>}
      {hits?.map((hit, i) => {
        const d = describe(hit)
        return (
          <div key={i} className="search-hit" onClick={() => onOpen(d.nodeId, d.past)}>
            <div className="inbox-head">
              <span className="inbox-kind">{hit.what}</span>
              {hit.at && <span className="inbox-time">{new Date(hit.at).toLocaleTimeString()}</span>}
            </div>
            <div className="search-source" title={d.label}>{d.label}{d.past && ' (ended)'}</div>
            <div className="search-snippet"><Highlight text={hit.snippet} query={query.trim()} /></div>
          </div>
        )
      })}
    </aside>
  )
}

function Highlight({ text, query }: { text: string; query: string }) {
  const i = text.toLowerCase().indexOf(query.toLowerCase())
  if (i < 0) return <>{text}</>
  return (
    <>
      {text.slice(0, i)}
      <mark>{text.slice(i, i + query.length)}</mark>
      {text.slice(i + query.length)}
    </>
  )
}

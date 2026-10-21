import { useCallback, useState } from 'react'
import { Channel, invoke } from '@tauri-apps/api/core'
import { open } from '@tauri-apps/plugin-dialog'
import { Terminal } from '@xterm/xterm'
import { FitAddon } from '@xterm/addon-fit'
import type { JobNode } from '../types'

type PtyEvent = { type: 'output'; data: string } | { type: 'exit'; code: number | null }
type SpawnRequest = { kind: 'new'; cwd: string; sessionId: string } | { kind: 'attach'; id: string }

export interface HostedTerminal {
  id: number
  title: string
  /** Session ids whose transcripts this terminal produces; used to route prompts. */
  sessionIds: string[]
  exited: boolean
  term: Terminal
  fit: FitAddon
}

const THEME = {
  background: '#0c0e18',
  foreground: '#e2e8f0',
  cursor: '#a855f7',
  selectionBackground: '#a855f755',
}

export function useTerminals() {
  const [terminals, setTerminals] = useState<HostedTerminal[]>([])
  const [activeId, setActiveId] = useState<number | null>(null)

  const spawn = useCallback(async (request: SpawnRequest, title: string, sessionIds: string[]) => {
    const term = new Terminal({
      fontFamily: "'Cascadia Code', 'SF Mono', 'Fira Code', monospace",
      fontSize: 12,
      cursorBlink: true,
      scrollback: 5000,
      theme: THEME,
    })
    const fit = new FitAddon()
    term.loadAddon(fit)

    let id = 0
    const channel = new Channel<PtyEvent>()
    channel.onmessage = ev => {
      if (ev.type === 'output') {
        term.write(ev.data)
      } else {
        term.write(`\r\n\x1b[90m[process exited${ev.code != null ? ` with code ${ev.code}` : ''}]\x1b[0m\r\n`)
        setTerminals(ts => ts.map(t => (t.id === id ? { ...t, exited: true } : t)))
      }
    }

    try {
      id = await invoke<number>('pty_spawn', { request, cols: term.cols, rows: term.rows, onEvent: channel })
    } catch (e) {
      term.dispose()
      throw e
    }
    term.onData(data => void invoke('pty_write', { id, data }).catch(() => {}))
    term.onResize(({ cols, rows }) => void invoke('pty_resize', { id, cols, rows }).catch(() => {}))

    setTerminals(ts => [...ts, { id, title, sessionIds, exited: false, term, fit }])
    setActiveId(id)
    return id
  }, [])

  /** Asks for a folder and starts a new Claude Code session there. */
  const startSession = useCallback(async () => {
    const cwd = await open({ directory: true, title: 'Start a Claude Code session in…' })
    if (typeof cwd !== 'string') return
    const sessionId = crypto.randomUUID()
    const title = cwd.split(/[/\\]/).filter(Boolean).pop() ?? cwd
    await spawn({ kind: 'new', cwd, sessionId }, title, [sessionId])
  }, [spawn])

  const attach = useCallback(
    (job: JobNode) => {
      const ids = [job.sessionId, job.resumeSessionId].filter((s): s is string => !!s)
      return spawn({ kind: 'attach', id: job.id }, job.name, ids)
    },
    [spawn],
  )

  /** Types `text` into the terminal as one pasted block, then presses Enter. */
  const send = useCallback(async (id: number, text: string) => {
    await invoke('pty_write', { id, data: `\x1b[200~${text}\x1b[201~` })
    await new Promise(r => setTimeout(r, 60))
    await invoke('pty_write', { id, data: '\r' })
  }, [])

  const close = useCallback(
    (id: number) => {
      void invoke('pty_kill', { id }).catch(() => {})
      terminals.find(t => t.id === id)?.term.dispose()
      const rest = terminals.filter(t => t.id !== id)
      setTerminals(rest)
      if (activeId === id) setActiveId(rest.at(-1)?.id ?? null)
    },
    [terminals, activeId],
  )

  const forSession = useCallback(
    (sessionId: string | undefined) =>
      sessionId ? terminals.find(t => !t.exited && t.sessionIds.includes(sessionId)) : undefined,
    [terminals],
  )

  return { terminals, activeId, setActiveId, startSession, attach, send, close, forSession }
}

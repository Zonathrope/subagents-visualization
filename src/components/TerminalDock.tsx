import { useEffect, useRef } from 'react'
import '@xterm/xterm/css/xterm.css'
import type { HostedTerminal } from '../hooks/useTerminals'

interface Props {
  terminals: HostedTerminal[]
  activeId: number | null
  height: number
  onHeightChange: (h: number) => void
  onSelect: (id: number) => void
  onClose: (id: number) => void
  onHide: () => void
}

export default function TerminalDock({ terminals, activeId, height, onHeightChange, onSelect, onClose, onHide }: Props) {
  const startDrag = (e: React.PointerEvent) => {
    const startY = e.clientY
    const startH = height
    const move = (ev: PointerEvent) =>
      onHeightChange(Math.min(window.innerHeight - 160, Math.max(120, startH + startY - ev.clientY)))
    const up = () => {
      window.removeEventListener('pointermove', move)
      window.removeEventListener('pointerup', up)
    }
    window.addEventListener('pointermove', move)
    window.addEventListener('pointerup', up)
  }

  return (
    <section className="dock" style={{ height }}>
      <div className="dock-handle" onPointerDown={startDrag} />
      <div className="dock-tabs">
        {terminals.map(t => (
          <div
            key={t.id}
            className={`dock-tab ${t.id === activeId ? 'active' : ''} ${t.exited ? 'exited' : ''}`}
            onClick={() => onSelect(t.id)}
            title={t.title}
          >
            <span className="dock-tab-title">{t.title}</span>
            <button
              className="dock-tab-close"
              title={t.exited ? 'Close' : 'Stop and close'}
              onClick={e => {
                e.stopPropagation()
                onClose(t.id)
              }}
            >
              ×
            </button>
          </div>
        ))}
        <button className="dock-hide" onClick={onHide} title="Hide terminals (they keep running)">▾</button>
      </div>
      <div className="dock-panes">
        {terminals.map(t => (
          <TerminalPane key={t.id} terminal={t} visible={t.id === activeId} />
        ))}
      </div>
    </section>
  )
}

function TerminalPane({ terminal, visible }: { terminal: HostedTerminal; visible: boolean }) {
  const ref = useRef<HTMLDivElement>(null)

  // xterm instances outlive this component (the dock can be hidden), so re-attach the
  // existing element rather than opening a new one.
  useEffect(() => {
    const el = ref.current
    if (!el) return
    const { term } = terminal
    if (term.element) el.appendChild(term.element)
    else term.open(el)
  }, [terminal])

  useEffect(() => {
    const el = ref.current
    if (!el || !visible) return
    const refit = () => {
      try {
        terminal.fit.fit()
      } catch {
        /* not laid out yet */
      }
    }
    refit()
    terminal.term.focus()
    const ro = new ResizeObserver(refit)
    ro.observe(el)
    return () => ro.disconnect()
  }, [terminal, visible])

  return <div ref={ref} className="dock-pane" style={{ display: visible ? 'block' : 'none' }} />
}

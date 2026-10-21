import { useEffect, useRef, useState } from 'react'
import type { Settings } from '../settings'
import { playUrgent } from '../sound'

export default function SettingsMenu({
  settings,
  onChange,
}: {
  settings: Settings
  onChange: (patch: Partial<Settings>) => void
}) {
  const [open, setOpen] = useState(false)
  const ref = useRef<HTMLDivElement>(null)

  useEffect(() => {
    if (!open) return
    const onDown = (e: MouseEvent) => {
      if (!ref.current?.contains(e.target as Node)) setOpen(false)
    }
    window.addEventListener('mousedown', onDown)
    return () => window.removeEventListener('mousedown', onDown)
  }, [open])

  return (
    <div className="settings" ref={ref}>
      <button className="header-button" onClick={() => setOpen(o => !o)} aria-expanded={open} title="Settings">
        ⚙
      </button>
      {open && (
        <div className="settings-popover">
          <label className="settings-row">
            <span>Agent counts as stuck after</span>
            <input
              type="number"
              min={1}
              max={120}
              value={settings.stuckMinutes}
              onChange={e => onChange({ stuckMinutes: Math.max(1, Number(e.target.value) || 1) })}
            />
            <span>min</span>
          </label>
          <label className="settings-row">
            <input type="checkbox" checked={settings.sounds} onChange={e => onChange({ sounds: e.target.checked })} />
            <span>Sound when something needs me</span>
            <button className="link-button" onClick={e => { e.preventDefault(); playUrgent() }}>test</button>
          </label>
          <label className="settings-row">
            <input
              type="checkbox"
              checked={settings.costEstimate}
              onChange={e => onChange({ costEstimate: e.target.checked })}
            />
            <span>Show cost estimates (API list prices)</span>
          </label>
        </div>
      )}
    </div>
  )
}

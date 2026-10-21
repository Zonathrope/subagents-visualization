import { useCallback, useState } from 'react'

export interface Settings {
  /** A running agent silent this long counts as stuck. */
  stuckMinutes: number
  /** Play the "needs you" sound for urgent inbox items. */
  sounds: boolean
  /** Show estimated dollar cost next to token counts. */
  costEstimate: boolean
}

const DEFAULTS: Settings = { stuckMinutes: 3, sounds: true, costEstimate: false }
const KEY = 'subagents.settings'

function load(): Settings {
  try {
    const raw = localStorage.getItem(KEY)
    return raw ? { ...DEFAULTS, ...JSON.parse(raw) } : DEFAULTS
  } catch {
    return DEFAULTS
  }
}

/** App preferences, remembered locally between launches. */
export function useSettings() {
  const [settings, setSettings] = useState<Settings>(load)
  const update = useCallback((patch: Partial<Settings>) => {
    setSettings(prev => {
      const next = { ...prev, ...patch }
      try {
        localStorage.setItem(KEY, JSON.stringify(next))
      } catch {
        /* preference just won't persist */
      }
      return next
    })
  }, [])
  return { settings, update }
}

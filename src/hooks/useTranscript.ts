import { useEffect, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import type { TranscriptView, WsMessage } from '../types'

const RELOAD_THROTTLE_MS = 400

/** Loads a transcript and reloads it whenever the watcher reports new lines in it. */
export function useTranscript(path: string | undefined) {
  const [view, setView] = useState<TranscriptView | null>(null)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    setView(null)
    setError(null)
    if (!path) return

    let disposed = false
    let timer: ReturnType<typeof setTimeout> | null = null
    let inFlight = false
    let again = false

    const load = async () => {
      if (inFlight) {
        again = true
        return
      }
      inFlight = true
      try {
        const next = await invoke<TranscriptView>('load_transcript', { path })
        if (!disposed) {
          setView(next)
          setError(null)
        }
      } catch (e) {
        if (!disposed) setError(String(e))
      } finally {
        inFlight = false
        if (again && !disposed) {
          again = false
          load()
        }
      }
    }

    load()
    const unlistenPromise = listen<WsMessage>('graph-event', ev => {
      if (ev.payload.type !== 'transcript_changed' || ev.payload.path !== path) return
      if (timer) return
      timer = setTimeout(() => {
        timer = null
        load()
      }, RELOAD_THROTTLE_MS)
    })

    return () => {
      disposed = true
      if (timer) clearTimeout(timer)
      unlistenPromise.then(unlisten => unlisten())
    }
  }, [path])

  return { view, error }
}

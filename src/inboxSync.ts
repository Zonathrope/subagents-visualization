import { useEffect, useRef, useState } from 'react'
import { emitTo, listen } from '@tauri-apps/api/event'
import type { AttentionItem } from './attention'

/**
 * The main window owns the inbox; the mini window mirrors it, so dismissing in either
 * clears both and only one window plays sounds.
 */
const ITEMS = 'inbox-items'
const REQUEST = 'inbox-request'
const DISMISS = 'inbox-dismiss'

/** Main window: publish items to the mini window and accept its dismissals. */
export function usePublishInbox(items: AttentionItem[], dismiss: (key: string) => void) {
  const latest = useRef(items)
  latest.current = items

  useEffect(() => {
    void emitTo('mini', ITEMS, items).catch(() => {})
  }, [items])

  useEffect(() => {
    const subs = [
      listen(REQUEST, () => void emitTo('mini', ITEMS, latest.current).catch(() => {})),
      listen<string>(DISMISS, ev => dismiss(ev.payload)),
    ]
    return () => subs.forEach(p => p.then(unlisten => unlisten()))
  }, [dismiss])
}

/** Mini window: the main window's current inbox. */
export function useMirroredInbox() {
  const [items, setItems] = useState<AttentionItem[]>([])
  useEffect(() => {
    const sub = listen<AttentionItem[]>(ITEMS, ev => setItems(ev.payload))
    sub.then(() => emitTo('main', REQUEST)).catch(() => {})
    return () => void sub.then(unlisten => unlisten())
  }, [])
  const dismiss = (key: string) => {
    setItems(cur => cur.filter(i => i.key !== key))
    void emitTo('main', DISMISS, key)
  }
  return { items, dismiss }
}

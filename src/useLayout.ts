import { useCallback, useEffect, useRef, useState } from 'react'
import { api } from './api'
import type { Layout } from './domain/Layout'

// The sidebar's order and groups. Loaded at start and when a job comes or goes, never on a mere refresh of the
// jobs, so a drag under way is not undone by it. A change shows at once and is saved after; if the save fails,
// the saved layout comes back. `null` until loaded, or when it cannot be read: the sidebar then lists the jobs.
// `ready` is false until the first load has answered: a change made before would save over the groups unseen.
export function useLayout(
  jobIds: string[] | null,
  onError: (error: unknown) => void,
): [Layout | null, (next: Layout) => void, boolean] {
  const [layout, setLayout] = useState<Layout | null>(null)
  const [ready, setReady] = useState(false)
  const key = jobIds ? [...jobIds].sort().join('\n') : null
  // Called through a ref, so a new function each render does not reload the layout.
  const report = useRef(onError)
  report.current = onError
  // Counts the changes: an answer to a load or a save older than the latest change is dropped, so it never puts an
  // earlier layout back on screen.
  const changes = useRef(0)
  // Saves go one at a time, so the file ends with the latest change: the one waiting, if any, goes next.
  const waiting = useRef<Layout | null>(null)
  const saving = useRef(false)

  const load = useCallback(() => {
    const at = changes.current
    return api
      .getLayout()
      .then(
        (loaded) => {
          if (at === changes.current) setLayout(loaded)
        },
        (e) => {
          if (at === changes.current) setLayout(null)
          report.current(e)
        },
      )
      .finally(() => setReady(true))
  }, [])

  // `key` is the set of job ids: a job that comes or goes is the reason to load again.
  useEffect(() => {
    if (key !== null) load()
  }, [key, load])

  const flush = useCallback(() => {
    const next = waiting.current
    if (!next) {
      saving.current = false
      return
    }
    waiting.current = null
    saving.current = true
    const at = changes.current
    api.saveLayout(next).then(
      (kept) => {
        if (at === changes.current) setLayout(kept)
        flush()
      },
      (e) => {
        saving.current = false
        waiting.current = null
        report.current(e)
        changes.current++
        load()
      },
    )
  }, [load])

  const change = useCallback(
    (next: Layout) => {
      changes.current++
      setLayout(next)
      waiting.current = next
      if (!saving.current) flush()
    },
    [flush],
  )

  return [layout, change, ready]
}

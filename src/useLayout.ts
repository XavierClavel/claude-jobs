import { useCallback, useEffect, useRef, useState } from 'react'
import { api } from './api'
import type { Layout } from './domain/Layout'

// The sidebar's order and groups. Loaded at start and when a job comes or goes, never on a mere refresh of the
// jobs, so a drag under way is not undone by it. A change shows at once and is saved after; if the save fails,
// the saved layout comes back. `null` until loaded, or when it cannot be read: the sidebar then lists the jobs.
export function useLayout(
  jobIds: string[] | null,
  onError: (error: unknown) => void,
): [Layout | null, (next: Layout) => void] {
  const [layout, setLayout] = useState<Layout | null>(null)
  const key = jobIds ? [...jobIds].sort().join('\n') : null
  // Called through a ref, so a new function each render does not reload the layout.
  const report = useRef(onError)
  report.current = onError

  const load = useCallback(
    () =>
      api.getLayout().then(setLayout, (e) => {
        setLayout(null)
        report.current(e)
      }),
    [],
  )

  // `key` is the set of job ids: a job that comes or goes is the reason to load again.
  useEffect(() => {
    if (key !== null) load()
  }, [key, load])

  const change = useCallback(
    (next: Layout) => {
      setLayout(next)
      api.saveLayout(next).then(setLayout, (e) => {
        report.current(e)
        load()
      })
    },
    [load],
  )

  return [layout, change]
}

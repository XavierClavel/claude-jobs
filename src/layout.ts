import type { Tone } from './components/Chip'
import type { JobGroup } from './domain/JobGroup'
import type { JobView } from './domain/JobView'
import type { Layout } from './domain/Layout'
import { jobTone } from './jobs'

// The sidebar's order and groups (`layout.json`), changed here by pure functions, then saved.

// Where the jobs in no group are, as a container beside the groups.
export const UNGROUPED = 'ungrouped'

// The same rules as `layout::reconcile` in Rust: every job once, in its first place, the others at the bottom of
// the jobs in no group. The sidebar shows this, so a job the saved layout does not know yet still shows.
export function reconcile(layout: Layout | null, jobIds: string[]): Layout {
  const exists = new Set(jobIds)
  const placed = new Set<string>()
  const keep = (list: string[]) =>
    list.filter((id) => exists.has(id) && !placed.has(id) && placed.add(id))
  const ungrouped = keep(layout?.ungrouped ?? [])
  const seen = new Set<string>()
  const groups = (layout?.groups ?? [])
    .filter((g) => !seen.has(g.id) && seen.add(g.id))
    .map((g) => ({ ...g, jobs: keep(g.jobs) }))
  return { ungrouped: [...ungrouped, ...jobIds.filter((id) => !placed.has(id))], groups }
}

export function containerOf(layout: Layout, jobId: string): string | null {
  if (layout.ungrouped.includes(jobId)) return UNGROUPED
  return layout.groups.find((g) => g.jobs.includes(jobId))?.id ?? null
}

const insert = <T>(list: T[], item: T, index: number) => {
  const at = Math.max(0, Math.min(index, list.length))
  return [...list.slice(0, at), item, ...list.slice(at)]
}

// `index` is the place in the target list once the job is out of where it was.
export function moveJob(layout: Layout, jobId: string, container: string, index: number): Layout {
  if (container !== UNGROUPED && !layout.groups.some((g) => g.id === container)) return layout
  const without = (list: string[]) => list.filter((id) => id !== jobId)
  const place = (list: string[], here: boolean) =>
    here ? insert(without(list), jobId, index) : without(list)
  return {
    ungrouped: place(layout.ungrouped, container === UNGROUPED),
    groups: layout.groups.map((g) => ({ ...g, jobs: place(g.jobs, g.id === container) })),
  }
}

export function moveGroup(layout: Layout, groupId: string, index: number): Layout {
  const group = layout.groups.find((g) => g.id === groupId)
  if (!group) return layout
  return {
    ...layout,
    groups: insert(
      layout.groups.filter((g) => g.id !== groupId),
      group,
      index,
    ),
  }
}

const changeGroup = (layout: Layout, id: string, change: (g: JobGroup) => JobGroup): Layout => ({
  ...layout,
  groups: layout.groups.map((g) => (g.id === id ? change(g) : g)),
})

export const addGroup = (layout: Layout, id: string, name: string): Layout => ({
  ...layout,
  groups: [...layout.groups, { id, name, collapsed: false, jobs: [] }],
})

export function renameGroup(layout: Layout, id: string, name: string): Layout {
  const trimmed = name.trim()
  return trimmed ? changeGroup(layout, id, (g) => ({ ...g, name: trimmed })) : layout
}

// Its jobs are not deleted: they go to the bottom of the jobs in no group.
export function removeGroup(layout: Layout, id: string): Layout {
  const jobs = layout.groups.find((g) => g.id === id)?.jobs ?? []
  return {
    ungrouped: [...layout.ungrouped, ...jobs],
    groups: layout.groups.filter((g) => g.id !== id),
  }
}

export const toggleGroup = (layout: Layout, id: string): Layout =>
  changeGroup(layout, id, (g) => ({ ...g, collapsed: !g.collapsed }))

// What a collapsed group's dot says: the worst of its jobs, so a failure does not hide in it.
const WORST_FIRST: Tone[] = ['bad', 'warn', 'running', 'ok', 'muted']

export function groupTone(jobs: JobView[]): Tone {
  const tones = new Set(jobs.map(jobTone))
  return WORST_FIRST.find((tone) => tones.has(tone)) ?? 'muted'
}

let made = 0
export const newGroupId = () => `g-${Date.now().toString(36)}-${(made++).toString(36)}`

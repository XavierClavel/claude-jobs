import { useState } from 'react'
import { type Checks, waitsFor } from '../conditions'
import type { JobGroup } from '../domain/JobGroup'
import type { JobView } from '../domain/JobView'
import type { Layout } from '../domain/Layout'
import { day, midSentence, STATUS_LABEL, scheduleShort } from '../format'
import { failed, isRunning, jobTone, unmetOf } from '../jobs'
import { groupTone, reconcile, removeGroup, renameGroup, toggleGroup } from '../layout'
import { Elapsed } from './Elapsed'
import { Icon } from './Icon'

// What a job is doing, in the line under its name: of a run, its time so far first, which the ellipsis spares.
export function JobLine({ job, checks }: { job: JobView; checks: Checks }) {
  const last = job.last_run
  if (last && isRunning(job))
    return last.progress ? (
      <>
        <Elapsed since={last.started_at} /> · {last.progress.text}
      </>
    ) : (
      <>
        Running · <Elapsed since={last.started_at} /> so far
      </>
    )
  const unmet = unmetOf(job)
  if (job.waiting) return <>Waiting for {waitsFor(unmet, checks) || 'its conditions'}</>
  if (!job.enabled) return <>Paused</>
  if (!job.loaded) return <>Not loaded in launchd</>
  const schedule = scheduleShort(job)
  if (unmet.length) return <>{`${schedule} · waits for ${waitsFor(unmet, checks)}`}</>
  if (job.watch?.problem) return <>{`${schedule} · cannot check GitHub`}</>
  if (last && failed(last.status))
    return <>{`${schedule} · last run ${STATUS_LABEL[last.status].toLowerCase()}`}</>
  if (last?.status === 'warning') return <>{`${schedule} · last run with a warning`}</>
  if (!job.next_run) return <>{schedule}</>
  return <>{`${schedule} · next ${midSentence(day(job.next_run))}`}</>
}

export function JobRow({
  job,
  selected,
  checks,
  onSelect,
}: {
  job: JobView
  selected: boolean
  checks: Checks
  onSelect: (id: string) => void
}) {
  return (
    <button
      type="button"
      className={`side-row${selected ? ' selected' : ''}${job.enabled ? '' : ' paused'}`}
      aria-current={selected ? 'page' : undefined}
      onClick={() => onSelect(job.id)}
    >
      <span className={`dot dot-${jobTone(job)}`} />
      <span className="side-row-text">
        <span className="side-row-name">{job.name}</span>
        <span className="side-row-sub">
          <JobLine job={job} checks={checks} />
        </span>
      </span>
    </button>
  )
}

// A group's name, turned into a field to rename it: Enter or leaving it keeps the name, Escape drops it.
function GroupName({
  group,
  renaming,
  onRename,
  onStartRename,
  onDone,
}: {
  group: JobGroup
  renaming: boolean
  onRename: (name: string) => void
  onStartRename: () => void
  onDone: () => void
}) {
  const [draft, setDraft] = useState(group.name)
  if (!renaming)
    return (
      // biome-ignore lint/a11y/noStaticElementInteractions: the menu's Rename does the same from the keyboard.
      <span className="side-group-name" onDoubleClick={onStartRename}>
        {group.name}
      </span>
    )
  const finish = (keep: boolean) => {
    if (keep && draft.trim() && draft.trim() !== group.name) onRename(draft)
    onDone()
  }
  return (
    <input
      className="side-group-input"
      aria-label="Group name"
      value={draft}
      // biome-ignore lint/a11y/noAutofocus: a group just made or asked to be renamed waits for its name.
      autoFocus
      onFocus={(e) => e.currentTarget.select()}
      onChange={(e) => setDraft(e.currentTarget.value)}
      onBlur={() => finish(true)}
      onKeyDown={(e) => {
        if (e.key === 'Enter') finish(true)
        if (e.key === 'Escape') finish(false)
      }}
    />
  )
}

export function GroupHeader({
  group,
  jobs,
  renaming,
  layout,
  onLayout,
  onRenaming,
}: {
  group: JobGroup
  jobs: JobView[]
  renaming: boolean
  layout: Layout
  onLayout: (layout: Layout) => void
  onRenaming: (id: string | null) => void
}) {
  const [menu, setMenu] = useState(false)
  return (
    <div className="side-group-head">
      <button
        type="button"
        className="side-group-toggle"
        aria-label={`${group.collapsed ? 'Expand' : 'Collapse'} ${group.name}`}
        aria-expanded={!group.collapsed}
        onClick={() => onLayout(toggleGroup(layout, group.id))}
      >
        <span className={`side-group-chevron${group.collapsed ? '' : ' open'}`}>
          <Icon name="chevron" size={12} />
        </span>
      </button>
      <GroupName
        // A new key restarts the draft from the current name each time the field opens.
        key={renaming ? 'renaming' : 'name'}
        group={group}
        renaming={renaming}
        onRename={(name) => onLayout(renameGroup(layout, group.id, name))}
        onStartRename={() => onRenaming(group.id)}
        onDone={() => onRenaming(null)}
      />
      <span className="side-group-count">· {jobs.length}</span>
      {group.collapsed && jobs.length > 0 && <span className={`dot dot-${groupTone(jobs)}`} />}
      <span className="side-group-more">
        <button
          type="button"
          className="link"
          aria-label={`More for ${group.name}`}
          aria-expanded={menu}
          onClick={() => setMenu(!menu)}
        >
          ⋯
        </button>
        {menu && (
          <span role="menu" className="side-group-menu">
            <button
              type="button"
              role="menuitem"
              onClick={() => {
                setMenu(false)
                onRenaming(group.id)
              }}
            >
              Rename
            </button>
            <button
              type="button"
              role="menuitem"
              onClick={() => {
                setMenu(false)
                onLayout(removeGroup(layout, group.id))
              }}
            >
              Delete group
            </button>
          </span>
        )}
      </span>
    </div>
  )
}

// The jobs in no group, then each group. The layout is reconciled with the jobs here too, so a job the saved one
// does not know yet still shows.
export function SideJobs({
  jobs,
  layout: saved,
  selected,
  checks,
  editing,
  renaming,
  onRenaming,
  onSelect,
  onLayout,
}: {
  jobs: JobView[]
  layout: Layout | null
  selected: string | null
  checks: Checks
  editing: boolean
  renaming: string | null
  onRenaming: (id: string | null) => void
  onSelect: (id: string) => void
  onLayout: (layout: Layout) => void
}) {
  const byId = new Map(jobs.map((j) => [j.id, j]))
  const layout = reconcile(
    saved,
    jobs.map((j) => j.id),
  )
  const viewsOf = (ids: string[]) => ids.flatMap((id) => byId.get(id) ?? [])
  const row = (job: JobView) => (
    <JobRow
      key={job.id}
      job={job}
      selected={job.id === selected}
      checks={checks}
      onSelect={onSelect}
    />
  )
  return (
    <nav aria-label="Jobs" className="side-jobs" inert={editing}>
      {viewsOf(layout.ungrouped).map(row)}
      {layout.groups.map((group) => {
        const groupJobs = viewsOf(group.jobs)
        return (
          // biome-ignore lint/a11y/useSemanticElements: a fieldset is for form controls, these are links.
          <div key={group.id} role="group" aria-label={group.name} className="side-group">
            <GroupHeader
              group={group}
              jobs={groupJobs}
              renaming={renaming === group.id}
              layout={layout}
              onLayout={onLayout}
              onRenaming={onRenaming}
            />
            {!group.collapsed &&
              (groupJobs.length ? (
                <div className="side-group-jobs">{groupJobs.map(row)}</div>
              ) : (
                <span className="side-group-empty muted small-text">No jobs</span>
              ))}
          </div>
        )
      })}
    </nav>
  )
}

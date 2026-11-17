import {
  type CollisionDetection,
  closestCenter,
  DndContext,
  type DragEndEvent,
  type DragOverEvent,
  DragOverlay,
  type DragStartEvent,
  PointerSensor,
  pointerWithin,
  useDroppable,
  useSensor,
  useSensors,
} from '@dnd-kit/core'
import { SortableContext, useSortable, verticalListSortingStrategy } from '@dnd-kit/sortable'
import { CSS } from '@dnd-kit/utilities'
import { type ReactNode, useState } from 'react'
import { type Checks, waitsFor } from '../conditions'
import type { JobGroup } from '../domain/JobGroup'
import type { JobView } from '../domain/JobView'
import type { Layout } from '../domain/Layout'
import { day, midSentence, STATUS_LABEL, scheduleShort } from '../format'
import { failed, isRunning, jobTone, unmetOf } from '../jobs'
import {
  containerOf,
  groupTone,
  moveGroup,
  moveJob,
  reconcile,
  removeGroup,
  renameGroup,
  toggleGroup,
  UNGROUPED,
} from '../layout'
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

// Drag and drop: a job's id is its own, a group's is `group:<id>`, and a list that takes jobs (the ones in no group,
// or a group's) is `container:<id>`, so an empty or collapsed group still takes a job dropped on it.
const GROUP = 'group:'
const CONTAINER = 'container:'

type Drag =
  | { type: 'job'; container: string }
  | { type: 'group' }
  | { type: 'container'; id: string }

const dragOf = (data: { current?: unknown } | undefined) => data?.current as Drag | undefined

// A group only lands among groups. A job lands in the list under the pointer, next to the closest of its jobs.
const collision: CollisionDetection = (args) => {
  const kind = dragOf(args.active.data)?.type
  const of = (type: Drag['type']) =>
    args.droppableContainers.filter((d) => dragOf(d.data)?.type === type)
  if (kind === 'group') return closestCenter({ ...args, droppableContainers: of('group') })
  const lists = pointerWithin({ ...args, droppableContainers: of('container') })
  const list = lists[0]
  if (!list) return []
  const listId = String(list.id).slice(CONTAINER.length)
  const jobs = of('job').filter((d) => {
    const drag = dragOf(d.data)
    return drag?.type === 'job' && drag.container === listId
  })
  return jobs.length ? closestCenter({ ...args, droppableContainers: jobs }) : lists
}

// Where a job dropped on `over` goes: the list and the place in it.
function target(layout: Layout, over: string): { container: string; index: number } | null {
  const list = (id: string) =>
    id === UNGROUPED ? layout.ungrouped : (layout.groups.find((g) => g.id === id)?.jobs ?? [])
  if (over.startsWith(CONTAINER)) {
    const container = over.slice(CONTAINER.length)
    return { container, index: list(container).length }
  }
  const container = containerOf(layout, over)
  return container === null ? null : { container, index: list(container).indexOf(over) }
}

function SortableJob({
  id,
  container,
  disabled,
  children,
}: {
  id: string
  container: string
  disabled: boolean
  children: ReactNode
}) {
  const { setNodeRef, listeners, transform, transition, isDragging } = useSortable({
    id,
    data: { type: 'job', container } satisfies Drag,
    disabled,
  })
  return (
    <div
      ref={setNodeRef}
      className={`side-sortable${isDragging ? ' dragging' : ''}`}
      style={{ transform: CSS.Translate.toString(transform), transition }}
      {...listeners}
    >
      {children}
    </div>
  )
}

function JobList({
  id,
  className,
  children,
}: {
  id: string
  className: string
  children: ReactNode
}) {
  const { setNodeRef, isOver } = useDroppable({
    id: CONTAINER + id,
    data: { type: 'container', id } satisfies Drag,
  })
  return (
    <div ref={setNodeRef} className={`${className}${isOver ? ' drop-over' : ''}`}>
      {children}
    </div>
  )
}

function SortableGroup({
  group,
  renaming,
  disabled,
  children,
}: {
  group: JobGroup
  renaming: boolean
  disabled: boolean
  children: (handle: { ref: (node: HTMLElement | null) => void; listeners: object }) => ReactNode
}) {
  const { setNodeRef, setActivatorNodeRef, listeners, transform, transition, isDragging } =
    useSortable({
      id: GROUP + group.id,
      data: { type: 'group' } satisfies Drag,
      // Selecting the name being typed must not drag the group.
      disabled: renaming || disabled,
    })
  return (
    // biome-ignore lint/a11y/useSemanticElements: a fieldset is for form controls, these are links.
    <div
      ref={setNodeRef}
      role="group"
      aria-label={group.name}
      className={`side-group${isDragging ? ' dragging' : ''}`}
      style={{ transform: CSS.Translate.toString(transform), transition }}
    >
      {children({ ref: setActivatorNodeRef, listeners: listeners ?? {} })}
    </div>
  )
}

// The jobs in no group, then each group. The layout is reconciled with the jobs here too, so a job the saved one
// does not know yet still shows. While a job or a group is dragged, the sidebar shows where it would land, and
// saves once it is dropped.
export function SideJobs({
  jobs,
  layout: saved,
  layoutReady,
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
  layoutReady: boolean
  selected: string | null
  checks: Checks
  editing: boolean
  renaming: string | null
  onRenaming: (id: string | null) => void
  onSelect: (id: string) => void
  onLayout: (layout: Layout) => void
}) {
  // A few pixels before a drag starts, so a click still selects the job.
  const sensors = useSensors(useSensor(PointerSensor, { activationConstraint: { distance: 5 } }))
  const [draft, setDraft] = useState<Layout | null>(null)
  const [dragged, setDragged] = useState<string | null>(null)
  const byId = new Map(jobs.map((j) => [j.id, j]))
  const ids = jobs.map((j) => j.id)
  const layout = reconcile(draft ?? saved, ids)
  const viewsOf = (list: string[]) => list.flatMap((id) => byId.get(id) ?? [])
  const row = (job: JobView, container: string) => (
    <SortableJob key={job.id} id={job.id} container={container} disabled={!layoutReady}>
      <JobRow job={job} selected={job.id === selected} checks={checks} onSelect={onSelect} />
    </SortableJob>
  )

  const start = ({ active }: DragStartEvent) => {
    setDraft(reconcile(saved, ids))
    setDragged(String(active.id))
  }
  // A job taken to another list moves there as it goes, so that list makes room for it.
  const over = ({ active, over }: DragOverEvent) => {
    if (!draft || !over || dragOf(active.data)?.type !== 'job') return
    const id = String(active.id)
    const to = target(draft, String(over.id))
    if (to && to.container !== containerOf(draft, id))
      setDraft(moveJob(draft, id, to.container, to.index))
  }
  const end = ({ active, over }: DragEndEvent) => {
    const from = draft
    setDraft(null)
    setDragged(null)
    if (!from || !over) return
    const id = String(active.id)
    let next = from
    if (id.startsWith(GROUP)) {
      const index = from.groups.findIndex((g) => GROUP + g.id === String(over.id))
      if (index >= 0) next = moveGroup(from, id.slice(GROUP.length), index)
    } else {
      const to = target(from, String(over.id))
      if (to) next = moveJob(from, id, to.container, to.index)
    }
    if (JSON.stringify(next) !== JSON.stringify(reconcile(saved, ids))) onLayout(next)
  }
  const cancel = () => {
    setDraft(null)
    setDragged(null)
  }

  const draggedJob = dragged && !dragged.startsWith(GROUP) ? byId.get(dragged) : undefined
  const draggedGroup = dragged?.startsWith(GROUP)
    ? layout.groups.find((g) => GROUP + g.id === dragged)
    : undefined

  return (
    <nav aria-label="Jobs" className="side-jobs" inert={editing}>
      <DndContext
        sensors={sensors}
        collisionDetection={collision}
        onDragStart={start}
        onDragOver={over}
        onDragEnd={end}
        onDragCancel={cancel}
      >
        <JobList
          id={UNGROUPED}
          className={`side-ungrouped${draggedJob && !layout.ungrouped.length ? ' empty' : ''}`}
        >
          <SortableContext items={layout.ungrouped} strategy={verticalListSortingStrategy}>
            {viewsOf(layout.ungrouped).map((job) => row(job, UNGROUPED))}
          </SortableContext>
          {/* Every job is in a group: this is where one comes out of its group. */}
          {draggedJob && !layout.ungrouped.length && (
            <span className="side-ungrouped-hint muted small-text">Drop here for no group</span>
          )}
        </JobList>
        <SortableContext
          items={layout.groups.map((g) => GROUP + g.id)}
          strategy={verticalListSortingStrategy}
        >
          {layout.groups.map((group) => {
            const groupJobs = viewsOf(group.jobs)
            return (
              <SortableGroup
                key={group.id}
                group={group}
                renaming={renaming === group.id}
                disabled={!layoutReady}
              >
                {(handle) => (
                  <JobList id={group.id} className="side-group-body">
                    <div ref={handle.ref} {...handle.listeners}>
                      <GroupHeader
                        group={group}
                        jobs={groupJobs}
                        renaming={renaming === group.id}
                        layout={layout}
                        onLayout={onLayout}
                        onRenaming={onRenaming}
                      />
                    </div>
                    {!group.collapsed &&
                      (groupJobs.length ? (
                        <div className="side-group-jobs">
                          <SortableContext
                            items={group.jobs}
                            strategy={verticalListSortingStrategy}
                          >
                            {groupJobs.map((job) => row(job, group.id))}
                          </SortableContext>
                        </div>
                      ) : (
                        <span className="side-group-empty muted small-text">No jobs</span>
                      ))}
                  </JobList>
                )}
              </SortableGroup>
            )
          })}
        </SortableContext>
        <DragOverlay>
          {draggedJob ? (
            <div className="side-drag">
              <JobRow job={draggedJob} selected={false} checks={checks} onSelect={() => {}} />
            </div>
          ) : draggedGroup ? (
            <div className="side-drag side-drag-group">
              <span className="side-group-name">{draggedGroup.name}</span>
            </div>
          ) : null}
        </DragOverlay>
      </DndContext>
    </nav>
  )
}

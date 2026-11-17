import type { JobGroup } from './domain/JobGroup'
import type { Layout } from './domain/Layout'
import {
  addGroup,
  containerOf,
  groupTone,
  moveGroup,
  moveJob,
  newGroupId,
  reconcile,
  removeGroup,
  renameGroup,
  toggleGroup,
  UNGROUPED,
} from './layout'
import { job, run } from './test/fixtures'

const group = (id: string, jobs: string[] = [], more: Partial<JobGroup> = {}): JobGroup => ({
  id,
  name: id.toUpperCase(),
  collapsed: false,
  jobs,
  ...more,
})

const layout = (ungrouped: string[], groups: JobGroup[] = []): Layout => ({ ungrouped, groups })

describe('reconcile', () => {
  it('lists every job ungrouped, in order, when there is no layout', () => {
    expect(reconcile(null, ['a', 'b'])).toEqual(layout(['a', 'b']))
  })

  it('drops the jobs that no longer exist', () => {
    expect(reconcile(layout(['x'], [group('g', ['a', 'y'])]), ['a'])).toEqual(
      layout([], [group('g', ['a'])]),
    )
  })

  it('appends the new jobs to the ones in no group', () => {
    expect(reconcile(layout(['b'], [group('g', ['a'])]), ['a', 'b', 'c'])).toEqual(
      layout(['b', 'c'], [group('g', ['a'])]),
    )
  })

  it('keeps a job listed twice in its first place', () => {
    expect(reconcile(layout(['a'], [group('g', ['a'])]), ['a'])).toEqual(
      layout(['a'], [group('g', [])]),
    )
  })
})

describe('containerOf', () => {
  it('says where a job is', () => {
    const l = layout(['a'], [group('g', ['b'])])
    expect(containerOf(l, 'a')).toBe(UNGROUPED)
    expect(containerOf(l, 'b')).toBe('g')
    expect(containerOf(l, 'z')).toBeNull()
  })
})

describe('moveJob', () => {
  it('reorders the jobs in no group', () => {
    expect(moveJob(layout(['a', 'b', 'c']), 'c', UNGROUPED, 0).ungrouped).toEqual(['c', 'a', 'b'])
  })

  it('puts a job in a group at a place', () => {
    expect(moveJob(layout(['a'], [group('g', ['b', 'c'])]), 'a', 'g', 1)).toEqual(
      layout([], [group('g', ['b', 'a', 'c'])]),
    )
  })

  it('takes a job out of its group', () => {
    expect(moveJob(layout(['a'], [group('g', ['b'])]), 'b', UNGROUPED, 1)).toEqual(
      layout(['a', 'b'], [group('g', [])]),
    )
  })

  it('puts a job in an empty group', () => {
    expect(moveJob(layout(['a'], [group('g')]), 'a', 'g', 0)).toEqual(
      layout([], [group('g', ['a'])]),
    )
  })

  it('appends a job when the place is past the end', () => {
    expect(moveJob(layout(['a'], [group('g', ['b'])]), 'a', 'g', 9).groups[0].jobs).toEqual([
      'b',
      'a',
    ])
  })

  it('leaves the layout as it is for a group that does not exist', () => {
    const l = layout(['a'])
    expect(moveJob(l, 'a', 'nope', 0)).toEqual(l)
  })
})

describe('groups', () => {
  it('moves a group', () => {
    const l = layout([], [group('g1'), group('g2'), group('g3')])
    expect(moveGroup(l, 'g3', 0).groups.map((g) => g.id)).toEqual(['g3', 'g1', 'g2'])
  })

  it('adds an expanded, empty group at the end', () => {
    expect(addGroup(layout([], [group('g1')]), 'g2', 'New group').groups[1]).toEqual({
      id: 'g2',
      name: 'New group',
      collapsed: false,
      jobs: [],
    })
  })

  it('renames a group, and keeps its name for a blank one', () => {
    const l = layout([], [group('g')])
    expect(renameGroup(l, 'g', '  Monitors ').groups[0].name).toBe('Monitors')
    expect(renameGroup(l, 'g', '  ').groups[0].name).toBe('G')
  })

  it('deletes a group and keeps its jobs, in no group', () => {
    expect(removeGroup(layout(['a'], [group('g', ['b', 'c'])]), 'g')).toEqual(
      layout(['a', 'b', 'c']),
    )
  })

  it('collapses and expands a group', () => {
    const collapsed = toggleGroup(layout([], [group('g')]), 'g')
    expect(collapsed.groups[0]?.collapsed).toBe(true)
    expect(toggleGroup(collapsed, 'g').groups[0]?.collapsed).toBe(false)
  })

  it('makes new group ids that differ', () => {
    expect(newGroupId()).not.toBe(newGroupId())
  })
})

describe('groupTone', () => {
  it('is the worst state of the group’s jobs', () => {
    const ok = job()
    const bad = job({ last_run: run({ status: 'failed' }) })
    const running = job({ last_run: run({ status: 'running' }) })
    expect(groupTone([ok, bad, running])).toBe('bad')
    expect(groupTone([ok, running])).toBe('running')
    expect(groupTone([])).toBe('muted')
  })
})

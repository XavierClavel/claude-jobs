import { render, screen, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { useState } from 'react'
import type { Checks } from '../conditions'
import type { JobView } from '../domain/JobView'
import type { Layout } from '../domain/Layout'
import { action, gcloudOut, job, run, signedIn } from '../test/fixtures'
import { Sidebar } from './Sidebar'

const noop = () => {}

function sidebar(jobs: JobView[], checks: Checks = signedIn, more = {}) {
  return render(
    <Sidebar
      version="0.6.0"
      jobs={jobs}
      selected={null}
      conditionsOpen={false}
      actionsOpen={false}
      editing={false}
      checks={checks}
      actions={{ onSignIn: noop, onCheck: () => Promise.resolve() }}
      actionList={[]}
      onCheckUpdate={() => Promise.resolve(true)}
      onNew={noop}
      onSelect={noop}
      onConditions={noop}
      onActions={noop}
      onOverview={noop}
      layout={null}
      layoutReady
      onLayout={noop}
      {...more}
    />,
  )
}

// The sidebar with its layout kept, as the app keeps it.
function Harness({ jobs }: { jobs: JobView[] }) {
  const [current, setCurrent] = useState<Layout | null>(null)
  return (
    <Sidebar
      version={null}
      jobs={jobs}
      selected={null}
      conditionsOpen={false}
      actionsOpen={false}
      editing={false}
      checks={signedIn}
      actions={{ onSignIn: noop, onCheck: () => Promise.resolve() }}
      actionList={[]}
      onCheckUpdate={() => Promise.resolve(true)}
      onNew={noop}
      onSelect={noop}
      onConditions={noop}
      onActions={noop}
      onOverview={noop}
      layout={current}
      layoutReady
      onLayout={setCurrent}
    />
  )
}

const row = (name: string) => screen.getByRole('button', { name: new RegExp(`^${name}`) })

describe('Sidebar', () => {
  it('says in a line what each job is doing', () => {
    sidebar([
      job({ id: 'a', name: 'Release digest', weekdays: [1, 3, 5], next_run: null }),
      job({ id: 'b', name: 'PR triage', enabled: false }),
      job({ id: 'e', name: 'Weekly report', weekdays: [1], next_run: null }),
      job({
        id: 'c',
        name: 'Nightly tests',
        hour: 6,
        last_run: run({ status: 'failed' }),
        next_run: null,
      }),
      job({
        id: 'd',
        name: 'BigQuery cost',
        unmet_conditions: [{ condition: 'gcloud', problem: 'signed out' }],
        waiting: { since: '2026-10-30T18:00:00Z', trigger: 'schedule', pid: 1 },
      }),
      job({ id: 'f', name: 'Photobook monitor', last_run: run({ status: 'running' }) }),
      job({
        id: 'g',
        name: 'Stuck orders',
        last_run: run({
          status: 'running',
          progress: { text: 'Investigating logs…', at: '2026-10-26T13:50:02Z' },
        }),
      }),
    ])
    expect(row('Release digest')).toHaveTextContent('Mon, Wed, Fri 09:00')
    expect(row('PR triage')).toHaveTextContent('Paused')
    expect(row('Weekly report')).toHaveTextContent('Weekly Mon 09:00')
    expect(row('Nightly tests')).toHaveTextContent('Daily 06:00 · last run failed')
    expect(row('BigQuery cost')).toHaveTextContent('Waiting for Google Cloud')
    expect(row('Photobook monitor')).toHaveTextContent(/Running · [\d:]+ so far$/)
    expect(row('Stuck orders')).toHaveTextContent(/Stuck orders[\d:]+ · Investigating logs…$/)
  })

  it('shows a job whose last run ended with a warning in orange, with what happened', () => {
    sidebar([
      job({
        id: 'w',
        name: 'Meeting prep',
        hour: 6,
        last_run: run({ status: 'warning' }),
        next_run: null,
      }),
      job({
        id: 'o',
        name: 'Daily digest',
        hour: 7,
        last_run: run({ status: 'ok' }),
        next_run: null,
      }),
    ])
    expect(row('Meeting prep')).toHaveTextContent('Daily 06:00 · last run with a warning')
    expect(row('Meeting prep').querySelector('.dot')).toHaveClass('dot-warn')
    expect(row('Daily digest').querySelector('.dot')).toHaveClass('dot-ok')
  })

  it('selects the job clicked', async () => {
    const onSelect = vi.fn()
    sidebar([job()], signedIn, { onSelect })
    await userEvent.click(row('Photobook monitor'))
    expect(onSelect).toHaveBeenCalledWith('photobook-monitor')
  })

  it('offers to sign in to a condition that is not met', async () => {
    const onSignIn = vi.fn()
    sidebar([job({ conditions: ['gcloud'] })], gcloudOut, {
      actions: { onSignIn, onCheck: () => Promise.resolve() },
    })
    const conditions = within(screen.getByRole('region', { name: 'Conditions' }))
    expect(conditions.getByText('signed in')).toBeInTheDocument()
    await userEvent.click(conditions.getByRole('button', { name: 'Sign in' }))
    expect(onSignIn).toHaveBeenCalledWith('gcloud')
  })

  it('lists the actions, and opens them', async () => {
    const onActions = vi.fn()
    sidebar([job()], signedIn, { actionList: [action()], onActions })
    const actions = within(screen.getByRole('region', { name: 'Actions' }))
    expect(actions.getByText('Alert workflow')).toBeInTheDocument()
    expect(actions.getByText('Slack')).toBeInTheDocument()
    await userEvent.click(actions.getByRole('button', { name: 'Manage' }))
    expect(onActions).toHaveBeenCalledOnce()
  })

  it('leaves Google Cloud out when no job needs it and gcloud is not there', () => {
    sidebar([job()], { ...signedIn, gcloud: { installed: false, signed_in: false, account: null } })
    expect(screen.queryByText('Google Cloud')).not.toBeInTheDocument()
  })

  it('holds the list while the editor is open', () => {
    sidebar([job()], signedIn, { editing: true })
    expect(screen.getByRole('button', { name: 'New job' })).toBeDisabled()
  })

  describe('groups', () => {
    const jobs = [
      job({ id: 'a', name: 'Slack recap' }),
      job({ id: 'b', name: 'Monitor pHash', last_run: run({ status: 'failed' }) }),
      job({ id: 'c', name: 'Matte share' }),
    ]
    const layout = (collapsed = false): Layout => ({
      ungrouped: ['a'],
      groups: [{ id: 'g', name: 'Monitors', collapsed, jobs: ['b', 'c'] }],
    })
    const names = () =>
      Array.from(document.querySelectorAll('.side-row-name, .side-group-name')).map(
        (e) => e.textContent,
      )

    it('shows the jobs without a group first, then each group with its jobs', () => {
      sidebar(jobs, signedIn, { layout: layout() })
      expect(names()).toEqual(['Slack recap', 'Monitors', 'Monitor pHash', 'Matte share'])
      const group = within(screen.getByRole('group', { name: 'Monitors' }))
      expect(group.getByRole('button', { name: /^Matte share/ })).toBeInTheDocument()
    })

    it('shows every job without a group when there is no layout', () => {
      sidebar(jobs)
      expect(names()).toEqual(['Slack recap', 'Monitor pHash', 'Matte share'])
    })

    it('shows a job the layout does not know yet, at the bottom of the jobs without a group', () => {
      sidebar([...jobs, job({ id: 'd', name: 'Prépa réunions' })], signedIn, { layout: layout() })
      expect(names()).toEqual([
        'Slack recap',
        'Prépa réunions',
        'Monitors',
        'Monitor pHash',
        'Matte share',
      ])
    })

    it('collapses a group', async () => {
      const onLayout = vi.fn()
      sidebar(jobs, signedIn, { layout: layout(), onLayout })
      await userEvent.click(screen.getByRole('button', { name: 'Collapse Monitors' }))
      expect(onLayout).toHaveBeenCalledWith(layout(true))
    })

    it('hides a collapsed group’s jobs, and shows how many and the worst state', () => {
      sidebar(jobs, signedIn, { layout: layout(true) })
      const group = screen.getByRole('group', { name: 'Monitors' })
      expect(within(group).queryByRole('button', { name: /^Matte share/ })).not.toBeInTheDocument()
      expect(group).toHaveTextContent('· 2')
      expect(group.querySelector('.dot')).toHaveClass('dot-bad')
    })

    it('renames a group on a double click', async () => {
      const onLayout = vi.fn()
      sidebar(jobs, signedIn, { layout: layout(), onLayout })
      await userEvent.dblClick(screen.getByText('Monitors'))
      const input = screen.getByRole('textbox', { name: 'Group name' })
      await userEvent.clear(input)
      await userEvent.type(input, 'Watchers{Enter}')
      expect(onLayout).toHaveBeenCalledOnce()
      expect(onLayout.mock.calls[0]?.[0].groups[0].name).toBe('Watchers')
    })

    it('keeps the name of a group renamed blank, or when the rename is cancelled', async () => {
      const onLayout = vi.fn()
      sidebar(jobs, signedIn, { layout: layout(), onLayout })
      await userEvent.dblClick(screen.getByText('Monitors'))
      await userEvent.clear(screen.getByRole('textbox', { name: 'Group name' }))
      await userEvent.keyboard('{Enter}')
      await userEvent.dblClick(screen.getByText('Monitors'))
      await userEvent.type(screen.getByRole('textbox', { name: 'Group name' }), 'x{Escape}')
      expect(onLayout).not.toHaveBeenCalled()
      expect(screen.getByText('Monitors')).toBeInTheDocument()
    })

    it('deletes a group and keeps its jobs', async () => {
      const onLayout = vi.fn()
      sidebar(jobs, signedIn, { layout: layout(), onLayout })
      await userEvent.click(screen.getByRole('button', { name: 'More for Monitors' }))
      await userEvent.click(screen.getByRole('menuitem', { name: 'Delete group' }))
      expect(onLayout).toHaveBeenCalledWith({ ungrouped: ['a', 'b', 'c'], groups: [] })
    })

    it('waits for the saved groups before adding one', () => {
      sidebar(jobs, signedIn, { layoutReady: false })
      expect(screen.getByRole('button', { name: 'New group' })).toBeDisabled()
    })

    it('adds a group, ready to be named', async () => {
      render(<Harness jobs={jobs} />)
      await userEvent.click(screen.getByRole('button', { name: 'New group' }))
      const input = screen.getByRole('textbox', { name: 'Group name' })
      expect(input).toHaveFocus()
      expect(input).toHaveValue('New group')
      await userEvent.type(input, '{Control>}a{/Control}Routines{Enter}')
      expect(screen.getByRole('group', { name: 'Routines' })).toBeInTheDocument()
    })
  })
})

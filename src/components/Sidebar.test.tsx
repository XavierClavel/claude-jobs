import { render, screen, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import type { Checks } from '../conditions'
import type { JobView } from '../domain/JobView'
import { gcloudOut, job, run, signedIn } from '../test/fixtures'
import { Sidebar } from './Sidebar'

const noop = () => {}

function sidebar(jobs: JobView[], checks: Checks = signedIn, more = {}) {
  return render(
    <Sidebar
      version="0.6.0"
      jobs={jobs}
      selected={null}
      conditionsOpen={false}
      editing={false}
      checks={checks}
      actions={{ onSignIn: noop, onCheck: () => Promise.resolve() }}
      onCheckUpdate={() => Promise.resolve(true)}
      onNew={noop}
      onSelect={noop}
      onConditions={noop}
      onOverview={noop}
      {...more}
    />,
  )
}

const row = (name: string) => screen.getByRole('button', { name: new RegExp(`^${name}`) })

describe('Sidebar', () => {
  it('says in a line what each job is doing', () => {
    sidebar([
      job({ id: 'a', name: 'Release digest', weekdays: [1, 3, 5], next_run: null }),
      job({ id: 'b', name: 'PR triage', enabled: false }),
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
    ])
    expect(row('Release digest')).toHaveTextContent('Mon, Wed, Fri 09:00')
    expect(row('PR triage')).toHaveTextContent('Paused')
    expect(row('Nightly tests')).toHaveTextContent('Daily 06:00 · last run failed')
    expect(row('BigQuery cost')).toHaveTextContent('Waiting for Google Cloud')
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

  it('leaves Google Cloud out when no job needs it and gcloud is not there', () => {
    sidebar([job()], { ...signedIn, gcloud: { installed: false, signed_in: false, account: null } })
    expect(screen.queryByText('Google Cloud')).not.toBeInTheDocument()
  })

  it('holds the list while the editor is open', () => {
    sidebar([job()], signedIn, { editing: true })
    expect(screen.getByRole('button', { name: 'New job' })).toBeDisabled()
  })
})

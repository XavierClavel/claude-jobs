import { render, screen, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { api } from '../api'
import { gcloudOut, job, run, signedIn } from '../test/fixtures'
import { needsAttention, Overview } from './Overview'

vi.mock('../api', () => ({
  api: { listRuns: vi.fn(() => Promise.resolve([])) },
}))

const noop = () => {}

describe('Overview', () => {
  it('lists the next runs, soonest first, with what they will wait for', async () => {
    const onSelect = vi.fn()
    render(
      <Overview
        jobs={[
          job({ id: 'late', name: 'Release digest', next_run: '2026-11-02T09:00:00Z' }),
          job({
            id: 'soon',
            name: 'BigQuery cost',
            conditions: ['gcloud'],
            next_run: '2026-10-31T18:00:00Z',
            unmet_conditions: [{ condition: 'gcloud', problem: 'signed out' }],
          }),
          job({ id: 'off', name: 'PR triage', enabled: false }),
        ]}
        checks={gcloudOut}
        actions={{ onSignIn: noop, onCheck: () => Promise.resolve() }}
        onSelect={onSelect}
        onOpenRun={noop}
      />,
    )
    const next = within(screen.getByRole('region', { name: 'Up next' }))
    const rows = next.getAllByRole('button')
    expect(rows.map((r) => r.textContent)).toEqual([
      expect.stringMatching(/BigQuery cost.*Waits for Google Cloud/),
      expect.stringMatching(/Release digest.*Ready/),
    ])
    expect(screen.getByText(/2 jobs scheduled, 1 paused/)).toBeInTheDocument()
    await userEvent.click(rows[1] as HTMLElement)
    expect(onSelect).toHaveBeenCalledWith('late')
    expect(api.listRuns).toHaveBeenCalledTimes(3)
  })

  it('lists the running jobs one under the other, each opening its job', async () => {
    const onSelect = vi.fn()
    render(
      <Overview
        jobs={[
          job({ id: 'bq', name: 'BigQuery cost', last_run: run({ status: 'running' }) }),
          job({ id: 'idle', name: 'Release digest' }),
          job({ id: 'tests', name: 'Nightly tests', last_run: run({ status: 'running' }) }),
        ]}
        checks={signedIn}
        actions={{ onSignIn: noop, onCheck: () => Promise.resolve() }}
        onSelect={onSelect}
        onOpenRun={noop}
      />,
    )
    const card = within(screen.getByRole('region', { name: 'Running' }))
    expect(card.getByText('2 running')).toBeInTheDocument()
    const rows = card.getAllByRole('button')
    expect(rows.map((r) => r.textContent)).toEqual([
      expect.stringMatching(/^BigQuery costsince \d\d:\d\d\d+:\d\d/),
      expect.stringMatching(/^Nightly testssince \d\d:\d\d\d+:\d\d/),
    ])
    await userEvent.click(rows[1] as HTMLElement)
    expect(onSelect).toHaveBeenCalledWith('tests')
  })
})

describe('needsAttention', () => {
  it('names a condition not met that an enabled job needs, and a last run that failed', () => {
    const items = needsAttention(
      [
        job({ id: 'bq', name: 'BigQuery cost', conditions: ['gcloud'] }),
        job({
          id: 'tests',
          name: 'Nightly tests',
          last_run: run({ status: 'failed', exit_code: 1 }),
        }),
      ],
      gcloudOut,
    )
    expect(items.map((i) => i.title)).toEqual([
      'Google Cloud is signed out',
      'Nightly tests failed',
    ])
  })

  it('leaves out a condition no enabled job needs', () => {
    const items = needsAttention(
      [job({ conditions: ['gcloud'], enabled: false }), job({ id: 'x' })],
      gcloudOut,
    )
    expect(items).toEqual([])
  })

  it('names a run whose message did not send', () => {
    const items = needsAttention(
      [
        job({
          name: 'Nightly tests',
          last_run: run({
            actions: [
              {
                action: 'alert',
                name: 'Alert workflow',
                recipient: 'ops',
                recipient_name: 'Ops channel',
                error: 'Slack refused the message (404).',
              },
              {
                action: 'alert',
                name: 'Alert workflow',
                recipient: 'me',
                recipient_name: 'Me',
                error: null,
              },
            ],
          }),
        }),
      ],
      signedIn,
    )
    expect(items.map((i) => [i.title, i.text])).toEqual([
      [
        'Nightly tests: Alert workflow to Ops channel did not send',
        'Slack refused the message (404).',
      ],
    ])
  })

  it('has nothing to say when all is well', () => {
    expect(needsAttention([job()], signedIn)).toEqual([])
  })
})

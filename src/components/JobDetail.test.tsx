import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { api } from '../api'
import type { JobView } from '../domain/JobView'
import { expired, gcloudOut, job, run, signedIn } from '../test/fixtures'
import { JobDetail } from './JobDetail'

vi.mock('../api', () => ({
  api: { listRuns: vi.fn(() => Promise.resolve([])) },
}))

const noop = () => {}

function detail(view: JobView, handlers: Partial<Parameters<typeof JobDetail>[0]> = {}) {
  return render(
    <JobDetail
      job={view}
      checks={signedIn}
      onRun={noop}
      onToggle={noop}
      onEdit={noop}
      onHistory={noop}
      onWatch={noop}
      {...handlers}
    />,
  )
}

const live = run({
  trigger: 'manual',
  status: 'running',
  ended_at: null,
  duration_s: null,
  exit_code: null,
  cost_usd: null,
  num_turns: null,
  result: null,
})

describe('JobDetail', () => {
  beforeEach(() => vi.mocked(api.listRuns).mockResolvedValue([]))
  afterEach(() => vi.useRealTimers())

  it('shows the schedule and that the job never ran', async () => {
    detail(job())
    expect(screen.getByRole('heading', { name: 'Photobook monitor' })).toBeInTheDocument()
    expect(screen.getByText('Scheduled')).toBeInTheDocument()
    expect(screen.getByText(/Every day at 09:00/)).toBeInTheDocument()
    expect(await screen.findByText('Never run')).toBeInTheDocument()
  })

  it('offers Resume on a paused job', async () => {
    const onToggle = vi.fn()
    detail(job({ enabled: false }), { onToggle })
    expect(screen.getByText('Paused')).toBeInTheDocument()
    await userEvent.click(screen.getByRole('button', { name: 'Resume' }))
    expect(onToggle).toHaveBeenCalledOnce()
  })

  it('shows how long a run has been going, and cannot start a second one meanwhile', () => {
    vi.useFakeTimers({ now: new Date('2026-10-26T14:01:56Z') })
    detail(job({ last_run: live }))
    expect(screen.getByText('15:05')).toBeInTheDocument()
    expect(screen.getByText(/^Started .* by hand · times out at/)).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Running…' })).toBeDisabled()
  })

  it('lists the recent runs, and opens the history on the one clicked', async () => {
    const onHistory = vi.fn()
    const runs = [
      run({ id: 'b', status: 'timeout', cost_usd: 2.37 }),
      run({ id: 'a', trigger: 'catch-up' }),
    ]
    vi.mocked(api.listRuns).mockResolvedValue(runs)
    detail(job({ last_run: runs[0] ?? null }), { onHistory })
    expect(await screen.findByText('Timed out')).toBeInTheDocument()
    expect(screen.getByText('Catch-up')).toBeInTheDocument()
    await userEvent.click(screen.getByText('$2.37'))
    expect(onHistory).toHaveBeenLastCalledWith('b')
    await userEvent.click(screen.getByRole('button', { name: 'All 2 runs' }))
    expect(onHistory).toHaveBeenLastCalledWith()
  })
})

describe('JobDetail conditions', () => {
  beforeEach(() => vi.mocked(api.listRuns).mockResolvedValue([]))

  it('warns that the next run will wait for its conditions', () => {
    detail(job({ unmet_conditions: [expired] }))
    expect(screen.getByText('Conditions not met')).toBeInTheDocument()
    expect(screen.getByText(/login expired.*Its next run waits until then/)).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Run now' })).toBeEnabled()
  })

  it('says nothing of them on a paused job', () => {
    detail(job({ enabled: false, unmet_conditions: [expired] }))
    expect(screen.queryByText('Conditions not met')).not.toBeInTheDocument()
  })

  it('shows a run waiting to start, which cannot be started twice', () => {
    detail(
      job({
        unmet_conditions: [expired],
        waiting: { since: '2026-10-30T08:07:24Z', trigger: 'schedule', pid: 1 },
      }),
    )
    expect(screen.getAllByText('Waiting to start')).toHaveLength(2)
    expect(screen.getByText(/Waiting since .*login expired.*starts as soon/)).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Waiting…' })).toBeDisabled()
  })

  it('lists what the job starts when, with Google Cloud only on the jobs that tick it', () => {
    render(
      <JobDetail
        job={job({ conditions: ['gcloud'] })}
        checks={gcloudOut}
        onRun={noop}
        onToggle={noop}
        onEdit={noop}
        onHistory={noop}
        onWatch={noop}
      />,
    )
    expect(screen.getByText('Claude Code signed in')).toBeInTheDocument()
    expect(screen.getByText('Google Cloud signed in')).toBeInTheDocument()
  })
})

describe('JobDetail on an interactive job', () => {
  beforeEach(() => vi.mocked(api.listRuns).mockResolvedValue([]))

  it('offers Watch only while a run is live', async () => {
    const onWatch = vi.fn()
    const { rerender } = detail(
      job({ interactive: true, last_run: { ...live, interactive: true } }),
      { onWatch },
    )
    expect(screen.getByText(/Watch opens it in Terminal/)).toBeInTheDocument()
    await userEvent.click(screen.getByRole('button', { name: 'Watch' }))
    expect(onWatch).toHaveBeenCalledOnce()
    rerender(
      <JobDetail
        job={job({ interactive: true })}
        checks={signedIn}
        onRun={noop}
        onToggle={noop}
        onEdit={noop}
        onHistory={noop}
        onWatch={onWatch}
      />,
    )
    expect(screen.queryByRole('button', { name: 'Watch' })).not.toBeInTheDocument()
  })
})

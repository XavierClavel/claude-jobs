import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { api } from '../api'
import type { JobView } from '../domain/JobView'
import { action, expired, gcloudOut, job, run, signedIn, use } from '../test/fixtures'
import { JobDetail } from './JobDetail'

vi.mock('../api', () => ({
  api: { listRuns: vi.fn(() => Promise.resolve([])), openOnGithub: vi.fn(() => Promise.resolve()) },
}))

const noop = () => {}

function detail(view: JobView, handlers: Partial<Parameters<typeof JobDetail>[0]> = {}) {
  return render(
    <JobDetail
      job={view}
      checks={signedIn}
      actions={[]}
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

  it('shows the trigger, each repository a chip that opens it on GitHub', async () => {
    detail(
      job({
        event: { kind: 'pull_request_opened', repos: ['o/api', 'o/web'], mine: true },
        watch: { checked_at: null, problem: null, pending: 0 },
      }),
    )
    expect(screen.getByText('Trigger')).toBeInTheDocument()
    expect(screen.getByText(/Each pull request you open in/)).toBeInTheDocument()
    expect(screen.getByText('First check within 5 minutes')).toBeInTheDocument()
    await userEvent.click(screen.getByRole('button', { name: 'web' }))
    expect(api.openOnGithub).toHaveBeenCalledWith('o/web')
  })

  it('shows the next run on its own line, none while paused', () => {
    const { unmount } = detail(job({ next_run: '2026-11-05T09:00:00+01:00' }))
    expect(screen.getByText('Next run')).toBeInTheDocument()
    expect(screen.getByText('Every day at 09:00')).toBeInTheDocument()
    unmount()
    detail(job({ enabled: false, next_run: null }))
    expect(screen.getByText('None while paused')).toBeInTheDocument()
  })

  it('folds the allowed tools, one per line, until asked for', async () => {
    detail(job({ allowed_tools: ['Read', 'Bash(gh pr view:*)'] }))
    const fold = screen.getByText('2 tools')
    expect(screen.getByText('Bash(gh pr view:*)')).not.toBeVisible()
    await userEvent.click(fold)
    expect(screen.getByText('Bash(gh pr view:*)')).toBeVisible()
    expect(screen.getByText('Read').tagName).toBe('LI')
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

  it('says what the run is doing, once Claude has said it', () => {
    const progress = { text: 'Retrieving orders from BigQuery…', at: '2026-10-26T13:50:02Z' }
    detail(job({ last_run: { ...live, progress } }))
    const now = screen.getByRole('region', { name: 'Current run' })
    expect(now).toHaveTextContent(/^Running\s*Retrieving orders from BigQuery…\s*Started/)
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

  it('shows the pull request that started a run as a chip that opens it, not the run', async () => {
    const onHistory = vi.fn()
    const pr = {
      repo: 'o/api',
      number: 21,
      title: 'Add a column',
      url: 'https://github.com/o/api/pull/21',
      author: 'me',
      head: 'feat',
      base: 'main',
      draft: false,
      created_at: '2026-11-04T10:00:00Z',
      merged_at: null,
    }
    vi.mocked(api.listRuns).mockResolvedValue([run({ trigger: 'pull-request', pull_request: pr })])
    detail(job(), { onHistory })
    await userEvent.click(await screen.findByRole('button', { name: '#21 api' }))
    expect(api.openOnGithub).toHaveBeenCalledWith('o/api', 21)
    expect(onHistory).not.toHaveBeenCalled()
  })
})

describe('JobDetail conditions', () => {
  beforeEach(() => vi.mocked(api.listRuns).mockResolvedValue([]))

  it('warns that the next run will wait for its conditions', () => {
    detail(job({ unmet_conditions: [expired] }))
    expect(screen.getByText('Conditions not met')).toBeInTheDocument()
    expect(screen.getByRole('status')).toHaveTextContent(/login expired/)
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
    expect(screen.getByText(/Waiting since .*login expired/)).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Waiting…' })).toBeDisabled()
  })

  it('lists what the job starts when, with Google Cloud only on the jobs that tick it', () => {
    render(
      <JobDetail
        job={job({ conditions: ['gcloud'] })}
        checks={gcloudOut}
        actions={[]}
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

  it('lists what the job does when a run ends', () => {
    detail(job({ on_failure: [use()] }), { actions: [action()] })
    expect(screen.getByText('On success').nextElementSibling).toHaveTextContent('Nothing')
    expect(screen.getByText('On failure').nextElementSibling).toHaveTextContent(
      'Alert workflow → Ops channel',
    )
    expect(screen.getByText('On complete').nextElementSibling).toHaveTextContent('Nothing')
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
    await userEvent.click(screen.getByRole('button', { name: 'Watch' }))
    expect(onWatch).toHaveBeenCalledOnce()
    rerender(
      <JobDetail
        job={job({ interactive: true })}
        checks={signedIn}
        actions={[]}
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

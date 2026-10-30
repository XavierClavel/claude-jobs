import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import type { JobView } from '../domain/JobView'
import { JobCard } from './JobCard'

function job(overrides: Partial<JobView> = {}): JobView {
  return {
    id: 'photobook-monitor',
    name: 'Photobook monitor',
    prompt: '/photobook-monitor',
    hour: 9,
    minute: 0,
    weekdays: [],
    month_days: [],
    last_day_fallback: false,
    cwd: '/Users/me',
    allowed_tools: [],
    model: '',
    timeout_minutes: 30,
    enabled: true,
    claude: '/bin/claude',
    path: '/usr/bin',
    interactive: false,
    conditions: [],
    label: 'com.claude-jobs.photobook-monitor',
    loaded: true,
    next_run: '2026-10-27T09:00:00+01:00',
    last_run: null,
    waiting: null,
    unmet_conditions: [],
    ...overrides,
  }
}

const noop = () => {}

describe('JobCard', () => {
  afterEach(() => {
    vi.useRealTimers()
  })

  it('shows the schedule and that the job never ran', () => {
    render(
      <JobCard
        job={job()}
        onRun={noop}
        onToggle={noop}
        onEdit={noop}
        onHistory={noop}
        onWatch={noop}
      />,
    )
    expect(screen.getByText('Scheduled')).toBeInTheDocument()
    expect(screen.getByText(/Every day at 09:00/)).toBeInTheDocument()
    expect(screen.getByText('Never run')).toBeInTheDocument()
  })

  it('offers Resume on a paused job', async () => {
    const onToggle = vi.fn()
    render(
      <JobCard
        job={job({ enabled: false })}
        onRun={noop}
        onToggle={onToggle}
        onEdit={noop}
        onHistory={noop}
        onWatch={noop}
      />,
    )
    expect(screen.getByText('Paused')).toBeInTheDocument()
    await userEvent.click(screen.getByRole('button', { name: 'Resume' }))
    expect(onToggle).toHaveBeenCalledOnce()
  })

  it('shows how long a run has been going, and cannot start a second one meanwhile', () => {
    vi.useFakeTimers({ now: new Date('2026-10-26T14:01:56Z') })
    const running = job({
      last_run: {
        id: '20261026T134651Z',
        job: 'photobook-monitor',
        trigger: 'manual',
        status: 'running',
        started_at: '2026-10-26T13:46:51Z',
        ended_at: null,
        duration_s: null,
        exit_code: null,
        cost_usd: null,
        num_turns: null,
        session_id: null,
        result: null,
        error: null,
        pid: 1,
        claude_pid: null,
        prompt: '/photobook-monitor',
        interactive: false,
        tmux_session: null,
      },
    })
    render(
      <JobCard
        job={running}
        onRun={noop}
        onToggle={noop}
        onEdit={noop}
        onHistory={noop}
        onWatch={noop}
      />,
    )
    expect(screen.getByText('15:05')).toBeInTheDocument()
    expect(screen.getByText(/^Running since .* · started by hand$/)).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Running…' })).toBeDisabled()
  })

  it('says when the last run caught up a missed time', () => {
    const caughtUp = job({
      last_run: {
        id: '20261027T081700Z',
        job: 'photobook-monitor',
        trigger: 'catch-up',
        status: 'ok',
        started_at: '2026-10-27T08:17:00Z',
        ended_at: '2026-10-27T08:18:41Z',
        duration_s: 101,
        exit_code: 0,
        cost_usd: 0.65,
        num_turns: 23,
        session_id: null,
        result: 'done',
        error: null,
        pid: 1,
        claude_pid: null,
        prompt: '/photobook-monitor',
        interactive: true,
        tmux_session: null,
      },
    })
    render(
      <JobCard
        job={caughtUp}
        onRun={noop}
        onToggle={noop}
        onEdit={noop}
        onHistory={noop}
        onWatch={noop}
      />,
    )
    expect(screen.getByText(/caught up at login/)).toBeInTheDocument()
  })
})

describe('JobCard conditions', () => {
  const expired = {
    condition: 'claude' as const,
    problem: "Claude Code's login expired: run claude, then /login.",
  }

  it('warns that the next run will wait for its conditions', () => {
    render(
      <JobCard
        job={job({ unmet_conditions: [expired] })}
        onRun={noop}
        onToggle={noop}
        onEdit={noop}
        onHistory={noop}
        onWatch={noop}
      />,
    )
    expect(screen.getByText('Conditions not met')).toBeInTheDocument()
    expect(screen.getByText(/login expired.*Its next run waits until then/)).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Run now' })).toBeEnabled()
  })

  it('says nothing of them on a paused job', () => {
    render(
      <JobCard
        job={job({ enabled: false, unmet_conditions: [expired] })}
        onRun={noop}
        onToggle={noop}
        onEdit={noop}
        onHistory={noop}
        onWatch={noop}
      />,
    )
    expect(screen.queryByText('Conditions not met')).not.toBeInTheDocument()
  })

  it('shows a run waiting to start, which cannot be started twice', () => {
    render(
      <JobCard
        job={job({
          unmet_conditions: [expired],
          waiting: { since: '2026-10-30T08:07:24Z', trigger: 'schedule', pid: 1 },
        })}
        onRun={noop}
        onToggle={noop}
        onEdit={noop}
        onHistory={noop}
        onWatch={noop}
      />,
    )
    expect(screen.getByText('Waiting to start')).toBeInTheDocument()
    expect(screen.getByText(/Waiting since .*login expired.*starts as soon/)).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Waiting…' })).toBeDisabled()
  })
})

describe('JobCard on an interactive job', () => {
  it('offers Watch only while a run is live', async () => {
    const onWatch = vi.fn()
    const live = job({
      interactive: true,
      last_run: {
        id: '20261026T134651Z',
        job: 'photobook-monitor',
        trigger: 'schedule',
        status: 'running',
        started_at: '2026-10-26T13:46:51Z',
        ended_at: null,
        duration_s: null,
        exit_code: null,
        cost_usd: null,
        num_turns: null,
        session_id: null,
        result: null,
        error: null,
        pid: 1,
        claude_pid: null,
        prompt: '/photobook-monitor',
        interactive: true,
        tmux_session: 'cj-photobook-monitor',
      },
    })
    const { rerender } = render(
      <JobCard
        job={live}
        onRun={noop}
        onToggle={noop}
        onEdit={noop}
        onHistory={noop}
        onWatch={onWatch}
      />,
    )
    await userEvent.click(screen.getByRole('button', { name: 'Watch' }))
    expect(onWatch).toHaveBeenCalledOnce()
    rerender(
      <JobCard
        job={job({ interactive: true })}
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

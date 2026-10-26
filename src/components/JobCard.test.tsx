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
    cwd: '/Users/me',
    allowed_tools: [],
    model: '',
    timeout_minutes: 30,
    enabled: true,
    claude: '/bin/claude',
    path: '/usr/bin',
    label: 'com.claude-jobs.photobook-monitor',
    loaded: true,
    next_run: '2026-10-27T09:00:00+01:00',
    last_run: null,
    ...overrides,
  }
}

const noop = () => {}

describe('JobCard', () => {
  it('shows the schedule and that the job never ran', () => {
    render(<JobCard job={job()} onRun={noop} onToggle={noop} onEdit={noop} onHistory={noop} />)
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
      />,
    )
    expect(screen.getByText('Paused')).toBeInTheDocument()
    await userEvent.click(screen.getByRole('button', { name: 'Resume' }))
    expect(onToggle).toHaveBeenCalledOnce()
  })

  it('cannot start a second run while one is running', () => {
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
      },
    })
    render(<JobCard job={running} onRun={noop} onToggle={noop} onEdit={noop} onHistory={noop} />)
    expect(screen.getByRole('button', { name: 'Running…' })).toBeDisabled()
  })
})

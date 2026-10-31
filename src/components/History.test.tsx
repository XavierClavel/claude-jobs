import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { api } from '../api'
import { job, run } from '../test/fixtures'
import { History, summary } from './History'

vi.mock('../api', () => ({
  api: { listRuns: vi.fn(), getRun: vi.fn() },
  errorsOf: () => [],
}))

const runs = [
  run({ id: 'new', started_at: '2026-10-29T07:30:00Z', cost_usd: 0.84 }),
  run({
    id: 'slow',
    started_at: '2026-10-28T07:30:00Z',
    status: 'timeout',
    duration_s: 1800,
    exit_code: 124,
    cost_usd: 2.37,
    num_turns: 88,
    session_id: '7c2e9a41',
    result: 'Checked 212 orders.',
    error: 'Timed out after 30 minutes.',
  }),
  run({ id: 'old', started_at: '2026-09-12T07:30:00Z', status: 'failed', cost_usd: 0.02 }),
]

describe('History', () => {
  beforeEach(() => {
    vi.mocked(api.listRuns).mockResolvedValue(runs)
    vi.mocked(api.getRun).mockImplementation((_, id) =>
      Promise.resolve({ ...(runs.find((r) => r.id === id) ?? run()), out: '', err: '' }),
    )
  })

  it('sums the runs up', () => {
    expect(summary(runs)).toBe('3 runs since 12 Sep · 1 OK · 1 failed · 1 timed out · $3.23 in all')
  })

  it('opens on the run asked for, and on another when it is picked', async () => {
    render(<History job={job()} run="slow" onBack={() => {}} />)
    expect(await screen.findByText('Checked 212 orders.')).toBeInTheDocument()
    expect(screen.getByText('Duration · hit the 30 min timeout')).toBeInTheDocument()
    expect(screen.getByText('claude --resume 7c2e9a41')).toBeInTheDocument()
    expect(screen.getByText('Timed out after 30 minutes.')).toBeInTheDocument()
    await userEvent.click(screen.getByRole('button', { name: /\$0\.84/ }))
    expect(await screen.findByText('done')).toBeInTheDocument()
  })

  it('opens on the newest run otherwise, and goes back to the job', async () => {
    const onBack = vi.fn()
    render(<History job={job()} run={null} onBack={onBack} />)
    expect(await screen.findByText('done')).toBeInTheDocument()
    expect(api.getRun).toHaveBeenCalledWith('photobook-monitor', 'new')
    await userEvent.click(screen.getByRole('button', { name: 'Back to Photobook monitor' }))
    expect(onBack).toHaveBeenCalledOnce()
  })
})

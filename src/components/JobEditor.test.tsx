import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { api } from '../api'
import { JobEditor } from './JobEditor'

vi.mock('../api', () => ({ api: { saveJob: vi.fn() }, errorsOf: () => [] }))

const defaults = { claude: '/bin/claude', path: '/usr/bin:/bin', cwd: '/Users/me' }

async function newJob() {
  const onSaved = vi.fn()
  render(<JobEditor job={null} defaults={defaults} onClose={() => {}} onSaved={onSaved} />)
  await userEvent.type(screen.getByLabelText('Name'), 'Monthly report')
  await userEvent.type(screen.getByLabelText('Prompt'), '/report')
  return onSaved
}

describe('JobEditor', () => {
  beforeEach(() => vi.mocked(api.saveJob).mockReset())

  it('starts daily, with no days to pick', async () => {
    await newJob()
    expect(screen.getByRole('radio', { name: 'Daily' })).toBeChecked()
    expect(screen.queryByRole('group', { name: 'Days of the week' })).not.toBeInTheDocument()
    expect(screen.getByText('Every day at 09:00')).toBeInTheDocument()
  })

  it('picks days of the week for a weekly job, starting on Monday', async () => {
    await newJob()
    await userEvent.click(screen.getByRole('radio', { name: 'Weekly' }))
    expect(screen.getByLabelText('Mon')).toBeChecked()
    await userEvent.click(screen.getByLabelText('Fri'))
    expect(screen.getByText('Mon, Fri at 09:00')).toBeInTheDocument()
  })

  it('saves a job that runs on days of the month', async () => {
    const onSaved = await newJob()
    await userEvent.click(screen.getByRole('radio', { name: 'Monthly' }))
    expect(screen.queryByRole('group', { name: 'Days of the week' })).not.toBeInTheDocument()
    await userEvent.click(screen.getByLabelText('15'))
    expect(screen.getByText('Monthly on the 1st, 15th at 09:00')).toBeInTheDocument()
    await userEvent.click(screen.getByRole('button', { name: 'Save' }))
    expect(api.saveJob).toHaveBeenCalledWith(
      expect.objectContaining({ id: 'monthly-report', weekdays: [], month_days: [1, 15] }),
      null,
    )
    expect(onSaved).toHaveBeenCalledOnce()
  })

  it('warns that a month without the day skips it', async () => {
    await newJob()
    await userEvent.click(screen.getByRole('radio', { name: 'Monthly' }))
    expect(screen.queryByText(/does not run in April/)).not.toBeInTheDocument()
    await userEvent.click(screen.getByLabelText('31'))
    expect(screen.getByText(/does not run in April/)).toBeInTheDocument()
  })

  it('can run on the last day of shorter months instead', async () => {
    await newJob()
    await userEvent.click(screen.getByRole('radio', { name: 'Monthly' }))
    expect(screen.queryByLabelText('Run on the last day of shorter months')).not.toBeInTheDocument()
    await userEvent.click(screen.getByLabelText('31'))
    await userEvent.click(screen.getByLabelText('Run on the last day of shorter months'))
    expect(screen.getByText(/runs on 30 April/)).toBeInTheDocument()
    await userEvent.click(screen.getByRole('button', { name: 'Save' }))
    expect(api.saveJob).toHaveBeenCalledWith(
      expect.objectContaining({ month_days: [1, 31], last_day_fallback: true }),
      null,
    )
  })

  it('has no catch-up option: every job catches up', async () => {
    await newJob()
    expect(screen.queryByLabelText(/Catch up/)).not.toBeInTheDocument()
  })

  it('wants a day before saving', async () => {
    await newJob()
    await userEvent.click(screen.getByRole('radio', { name: 'Weekly' }))
    await userEvent.click(screen.getByLabelText('Mon'))
    await userEvent.click(screen.getByRole('button', { name: 'Save' }))
    expect(screen.getByRole('alert')).toHaveTextContent('Pick at least one day of the week.')
    expect(api.saveJob).not.toHaveBeenCalled()
  })
})

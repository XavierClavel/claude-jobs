import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { api } from '../api'
import { JobEditor } from './JobEditor'

vi.mock('../api', () => ({
  api: { saveJob: vi.fn(), listSkills: vi.fn(() => Promise.resolve([])) },
  errorsOf: () => [],
}))

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

describe('JobEditor prompt', () => {
  const skills = [
    { name: 'photobook-monitor', description: 'Refresh the photobook monitor', argument_hint: '' },
    { name: 'prompt-review:prompt-review', description: 'Grade recent prompts', argument_hint: '' },
    { name: 'code-review', description: 'Review the diff', argument_hint: '[low|high]' },
  ]

  async function openEditor() {
    vi.mocked(api.listSkills).mockClear().mockResolvedValue(skills)
    render(<JobEditor job={null} defaults={defaults} onClose={() => {}} onSaved={() => {}} />)
    await waitFor(() =>
      expect(api.listSkills).toHaveBeenCalledWith('/bin/claude', '/usr/bin:/bin', '/Users/me'),
    )
    return screen.getByLabelText('Prompt')
  }

  it('suggests the skills that match what follows the slash', async () => {
    const prompt = await openEditor()
    await userEvent.type(prompt, '/')
    expect(screen.getAllByRole('option')).toHaveLength(3)
    await userEvent.type(prompt, 'pr')
    expect(screen.getAllByRole('option').map((o) => o.textContent)).toEqual([
      '/prompt-review:prompt-reviewGrade recent prompts',
    ])
  })

  it('puts in the skill picked with the arrows and Enter', async () => {
    const prompt = await openEditor()
    await userEvent.type(prompt, '/re')
    expect(screen.getByRole('option', { selected: true })).toHaveTextContent('/prompt-review')
    await userEvent.keyboard('{ArrowDown}')
    expect(screen.getByRole('option', { selected: true })).toHaveTextContent('/code-review')
    await userEvent.keyboard('{Enter}')
    expect(prompt).toHaveValue('/code-review ')
    expect(screen.queryByRole('listbox')).not.toBeInTheDocument()
    await userEvent.type(prompt, 'high')
    expect(prompt).toHaveValue('/code-review high')
  })

  it('puts in the skill clicked', async () => {
    const prompt = await openEditor()
    await userEvent.type(prompt, 'Run /pho')
    await userEvent.click(screen.getByRole('option', { name: /photobook-monitor/ }))
    expect(prompt).toHaveValue('Run /photobook-monitor ')
    expect(prompt).toHaveFocus()
  })

  it('hides the list on Escape, and shows it again on the next key', async () => {
    const prompt = await openEditor()
    await userEvent.type(prompt, '/p')
    await userEvent.keyboard('{Escape}')
    expect(screen.queryByRole('listbox')).not.toBeInTheDocument()
    await userEvent.type(prompt, 'h')
    expect(screen.getByRole('option')).toHaveTextContent('/photobook-monitor')
  })
})

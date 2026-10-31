import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { api } from '../api'
import { command } from '../test/fixtures'
import { CommandConditionEditor } from './CommandConditionEditor'

vi.mock('../api', () => ({
  api: { saveCondition: vi.fn(), checkCondition: vi.fn() },
  errorsOf: (e: unknown) => [String(e)],
}))

describe('CommandConditionEditor', () => {
  beforeEach(() => {
    vi.mocked(api.saveCondition).mockReset()
    vi.mocked(api.checkCondition).mockReset()
  })

  it('saves a new condition on the exit code, with an id from its name', async () => {
    const onSaved = vi.fn()
    vi.mocked(api.saveCondition).mockImplementation((c) => Promise.resolve(c))
    render(<CommandConditionEditor condition={null} onCancel={() => {}} onSaved={onSaved} />)
    await userEvent.type(screen.getByLabelText('Name'), 'Database up')
    await userEvent.type(screen.getByLabelText('Command'), 'pg_isready')
    await userEvent.clear(screen.getByLabelText('Expected exit code'))
    await userEvent.type(screen.getByLabelText('Expected exit code'), '0')
    await userEvent.click(screen.getByRole('switch', { name: /Every job/ }))
    await userEvent.click(screen.getByRole('button', { name: 'Save' }))
    expect(api.saveCondition).toHaveBeenCalledWith(
      {
        id: 'database-up',
        name: 'Database up',
        command: 'pg_isready',
        expect: { kind: 'exit_code', code: 0 },
        every_job: true,
        hint: '',
      },
      null,
    )
    expect(onSaved).toHaveBeenCalledOnce()
  })

  it('checks what the command prints, all of it or within it', async () => {
    vi.mocked(api.saveCondition).mockImplementation((c) => Promise.resolve(c))
    render(<CommandConditionEditor condition={command()} onCancel={() => {}} onSaved={() => {}} />)
    expect(screen.getByRole('radio', { name: 'Output is' })).toBeChecked()
    expect(screen.getByLabelText('Id')).toBeDisabled()
    await userEvent.click(screen.getByRole('radio', { name: 'Output contains' }))
    await userEvent.clear(screen.getByLabelText('Text in the output'))
    await userEvent.type(screen.getByLabelText('Text in the output'), 'Connected')
    await userEvent.click(screen.getByRole('button', { name: 'Save' }))
    expect(api.saveCondition).toHaveBeenCalledWith(
      expect.objectContaining({ expect: { kind: 'stdout', text: 'Connected', contains: true } }),
      'vpn',
    )
  })

  it('tests the command before it is saved, and says what it did', async () => {
    vi.mocked(api.checkCondition).mockResolvedValue({
      problem: 'Connect to the VPN: the command printed "Disconnected", not "Connected".',
      exit_code: 0,
      stdout: 'Disconnected\n',
    })
    render(<CommandConditionEditor condition={command()} onCancel={() => {}} onSaved={() => {}} />)
    await userEvent.click(screen.getByRole('button', { name: 'Test' }))
    const result = screen.getByRole('status')
    expect(result).toHaveTextContent('Not met')
    expect(result).toHaveTextContent('exit code 0')
    expect(result).toHaveTextContent('Disconnected')
    expect(api.saveCondition).not.toHaveBeenCalled()
    // The result was for the command as it was.
    await userEvent.type(screen.getByLabelText('Command'), ' ')
    expect(screen.queryByRole('status')).not.toBeInTheDocument()
  })

  it('shows what the Rust side refuses', async () => {
    vi.mocked(api.saveCondition).mockRejectedValue('gcloud is the id of a built-in condition.')
    render(<CommandConditionEditor condition={null} onCancel={() => {}} onSaved={() => {}} />)
    await userEvent.type(screen.getByLabelText('Name'), 'gcloud')
    await userEvent.type(screen.getByLabelText('Command'), 'true')
    await userEvent.click(screen.getByRole('button', { name: 'Save' }))
    expect(screen.getByRole('alert')).toHaveTextContent('built-in condition')
  })
})

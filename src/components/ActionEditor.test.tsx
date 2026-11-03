import { render, screen, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { api } from '../api'
import { action } from '../test/fixtures'
import { ActionEditor } from './ActionEditor'

vi.mock('../api', () => ({
  api: { saveAction: vi.fn(), testAction: vi.fn() },
  errorsOf: (e: unknown) => [String(e)],
}))

const HOOK = 'https://hooks.slack.com/triggers/T0123/456/s3cr3t'

describe('ActionEditor', () => {
  beforeEach(() => {
    vi.mocked(api.saveAction).mockReset()
    vi.mocked(api.testAction).mockReset()
  })

  it('saves a new Slack action to a user, with an id from its name', async () => {
    const onSaved = vi.fn()
    vi.mocked(api.saveAction).mockImplementation((a) => Promise.resolve(a))
    render(<ActionEditor action={null} onCancel={() => {}} onSaved={onSaved} />)
    await userEvent.type(screen.getByLabelText('Name'), 'Me on Slack')
    await userEvent.type(screen.getByLabelText('Webhook URL'), HOOK)
    expect(screen.getByRole('radio', { name: 'Channel' })).toBeChecked()
    await userEvent.click(screen.getByRole('radio', { name: 'User' }))
    await userEvent.type(screen.getByLabelText('User ID'), 'U0123ABCD')
    await userEvent.click(screen.getByRole('button', { name: 'Save' }))
    expect(api.saveAction).toHaveBeenCalledWith(
      {
        id: 'me-on-slack',
        name: 'Me on Slack',
        target: { kind: 'slack', webhook_url: HOOK, to: 'user', recipient: 'U0123ABCD' },
      },
      null,
    )
    expect(onSaved).toHaveBeenCalledOnce()
  })

  it('edits an action without changing its id', async () => {
    vi.mocked(api.saveAction).mockImplementation((a) => Promise.resolve(a))
    render(<ActionEditor action={action()} onCancel={() => {}} onSaved={() => {}} />)
    expect(screen.getByLabelText('Id')).toBeDisabled()
    expect(screen.getByLabelText('Channel ID')).toHaveValue('C0123ABCD')
    await userEvent.clear(screen.getByLabelText('Channel ID'))
    await userEvent.type(screen.getByLabelText('Channel ID'), 'C0999ZZZZ')
    await userEvent.click(screen.getByRole('button', { name: 'Save' }))
    expect(api.saveAction).toHaveBeenCalledWith(
      expect.objectContaining({ target: expect.objectContaining({ recipient: 'C0999ZZZZ' }) }),
      'ops',
    )
  })

  it('sends a test message before it is saved, and says how it went', async () => {
    vi.mocked(api.testAction).mockResolvedValueOnce()
    render(<ActionEditor action={action()} onCancel={() => {}} onSaved={() => {}} />)
    await userEvent.click(screen.getByRole('button', { name: 'Send test' }))
    expect(api.testAction).toHaveBeenCalledWith(action())
    expect(screen.getByRole('status')).toHaveTextContent('Sent to channel C0123ABCD')
    // The result was for the webhook as it was.
    await userEvent.type(screen.getByLabelText('Channel ID'), '1')
    expect(screen.queryByRole('status')).not.toBeInTheDocument()
    vi.mocked(api.testAction).mockRejectedValueOnce('Slack refused the message (404 no_service).')
    await userEvent.click(screen.getByRole('button', { name: 'Send test' }))
    expect(screen.getByRole('status')).toHaveTextContent('Not sent')
    expect(screen.getByRole('status')).toHaveTextContent('404 no_service')
    expect(api.saveAction).not.toHaveBeenCalled()
  })

  it('says how to make the workflow, for a channel or a user', async () => {
    render(<ActionEditor action={null} onCancel={() => {}} onSaved={() => {}} />)
    const setup = within(screen.getByRole('region', { name: 'Workflow setup' }))
    expect(setup.getByText('Send a message to a channel')).toBeInTheDocument()
    expect(setup.getByRole('row', { name: /^channel Channel ID/ })).toBeInTheDocument()
    for (const key of ['text', 'summary', 'result', 'job', 'status'])
      expect(setup.getByRole('row', { name: new RegExp(`^${key} Text`) })).toBeInTheDocument()
    await userEvent.click(screen.getByRole('radio', { name: 'User' }))
    expect(setup.getByText('Send a message to a person')).toBeInTheDocument()
    expect(setup.getByRole('row', { name: /^user User ID/ })).toBeInTheDocument()
    expect(screen.getByLabelText('User ID')).toHaveAttribute('placeholder', 'U012AB3CD')
  })

  it('shows what the Rust side refuses', async () => {
    vi.mocked(api.saveAction).mockRejectedValue(
      'The webhook URL starts with https://hooks.slack.com/.',
    )
    render(<ActionEditor action={null} onCancel={() => {}} onSaved={() => {}} />)
    await userEvent.type(screen.getByLabelText('Name'), 'Ops')
    await userEvent.type(screen.getByLabelText('Webhook URL'), 'https://example.com')
    await userEvent.click(screen.getByRole('button', { name: 'Save' }))
    expect(screen.getByRole('alert')).toHaveTextContent('hooks.slack.com')
  })
})

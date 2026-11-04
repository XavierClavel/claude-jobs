import { render, screen, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { api } from '../api'
import { action, recipient } from '../test/fixtures'
import { ActionEditor } from './ActionEditor'

vi.mock('../api', () => ({
  api: { saveAction: vi.fn(), testAction: vi.fn() },
  errorsOf: (e: unknown) => [String(e)],
}))

const HOOK = 'https://hooks.slack.com/triggers/T0123/456/s3cr3t'
const me = recipient({ id: 'me', name: 'Xavier', slack_id: 'U0123ABCD' })

describe('ActionEditor', () => {
  beforeEach(() => {
    vi.mocked(api.saveAction).mockReset()
    vi.mocked(api.testAction).mockReset()
  })

  it('saves a new Slack action with its recipients, ids from their names', async () => {
    const onSaved = vi.fn()
    vi.mocked(api.saveAction).mockImplementation((a) => Promise.resolve(a))
    render(<ActionEditor action={null} onCancel={() => {}} onSaved={onSaved} />)
    await userEvent.type(screen.getByLabelText('Name'), 'Private message')
    await userEvent.type(screen.getByLabelText('Webhook URL'), HOOK)
    const recipients = within(screen.getByRole('group', { name: 'Recipients' }))
    for (const [name, id] of [
      ['Xavier', 'U0123ABCD'],
      ['Ops', 'C0123ABCD'],
      ['Ops', 'C0999ZZZZ'],
    ] as const) {
      await userEvent.click(recipients.getByRole('button', { name: 'Add recipient' }))
      const at = recipients.getAllByLabelText(/^Name of recipient/).length
      await userEvent.type(recipients.getByLabelText(`Name of recipient ${at}`), name)
      await userEvent.type(recipients.getByLabelText(`Slack ID of recipient ${at}`), id)
    }
    expect(recipients.getByText('User')).toBeInTheDocument()
    expect(recipients.getAllByText('Channel')).toHaveLength(2)
    await userEvent.click(screen.getByRole('button', { name: 'Save' }))
    expect(api.saveAction).toHaveBeenCalledWith(
      {
        id: 'private-message',
        name: 'Private message',
        target: { kind: 'slack', webhook_url: HOOK },
        recipients: [
          { id: 'xavier', name: 'Xavier', slack_id: 'U0123ABCD' },
          { id: 'ops', name: 'Ops', slack_id: 'C0123ABCD' },
          { id: 'ops-2', name: 'Ops', slack_id: 'C0999ZZZZ' },
        ],
      },
      null,
    )
    expect(onSaved).toHaveBeenCalledOnce()
  })

  it('edits an action, keeping its id and its recipients ids when they are renamed', async () => {
    vi.mocked(api.saveAction).mockImplementation((a) => Promise.resolve(a))
    render(
      <ActionEditor
        action={action({ recipients: [recipient(), me] })}
        onCancel={() => {}}
        onSaved={() => {}}
      />,
    )
    expect(screen.getByLabelText('Id')).toBeDisabled()
    await userEvent.clear(screen.getByLabelText('Name of recipient 1'))
    await userEvent.type(screen.getByLabelText('Name of recipient 1'), 'Ops alerts')
    await userEvent.click(screen.getByRole('button', { name: 'Remove recipient 2' }))
    await userEvent.click(screen.getByRole('button', { name: 'Save' }))
    expect(api.saveAction).toHaveBeenCalledWith(
      expect.objectContaining({
        id: 'alert',
        recipients: [{ id: 'ops', name: 'Ops alerts', slack_id: 'C0123ABCD' }],
      }),
      'alert',
    )
  })

  it('sends a test to the recipient picked, before it is saved, and says how it went', async () => {
    vi.mocked(api.testAction).mockResolvedValueOnce()
    const action2 = action({ recipients: [recipient(), me] })
    render(<ActionEditor action={action2} onCancel={() => {}} onSaved={() => {}} />)
    await userEvent.selectOptions(screen.getByLabelText('Send the test to'), 'me')
    await userEvent.click(screen.getByRole('button', { name: 'Send test to' }))
    expect(api.testAction).toHaveBeenCalledWith(action2, me)
    expect(screen.getByRole('status')).toHaveTextContent('Sent to Xavier')
    // The result was for the webhook as it was.
    await userEvent.type(screen.getByLabelText('Webhook URL'), '1')
    expect(screen.queryByRole('status')).not.toBeInTheDocument()
    vi.mocked(api.testAction).mockRejectedValueOnce('Slack refused the message (404 no_service).')
    await userEvent.click(screen.getByRole('button', { name: 'Send test to' }))
    expect(screen.getByRole('status')).toHaveTextContent('Not sent')
    expect(screen.getByRole('status')).toHaveTextContent('404 no_service')
    expect(api.saveAction).not.toHaveBeenCalled()
  })

  it('saves and tests an action with no recipient, and offers only complete ones to test', async () => {
    vi.mocked(api.testAction).mockResolvedValueOnce()
    vi.mocked(api.saveAction).mockImplementation((a) => Promise.resolve(a))
    const bare = action({ recipients: [] })
    render(<ActionEditor action={bare} onCancel={() => {}} onSaved={() => {}} />)
    await userEvent.click(screen.getByRole('button', { name: 'Send test to' }))
    expect(api.testAction).toHaveBeenCalledWith(bare, null)
    expect(screen.getByRole('status')).toHaveTextContent(/^Sent$/)
    await userEvent.click(screen.getByRole('button', { name: 'Add recipient' }))
    await userEvent.type(screen.getByLabelText('Name of recipient 1'), 'Xavier')
    await userEvent.type(screen.getByLabelText('Slack ID of recipient 1'), '@xavier')
    const options = screen.getByLabelText('Send the test to').querySelectorAll('option')
    expect([...options].map((o) => o.textContent)).toEqual(['No recipient'])
    await userEvent.click(screen.getByRole('button', { name: 'Remove recipient 1' }))
    await userEvent.click(screen.getByRole('button', { name: 'Save' }))
    expect(api.saveAction).toHaveBeenCalledWith(bare, 'alert')
  })

  it('says how to make the workflow, for users and for channels', () => {
    render(<ActionEditor action={null} onCancel={() => {}} onSaved={() => {}} />)
    const setup = within(screen.getByRole('region', { name: 'Workflow setup' }))
    expect(setup.getByText('Send a message to a channel')).toBeInTheDocument()
    expect(setup.getByText('Send a message to a person')).toBeInTheDocument()
    expect(setup.getByRole('row', { name: /^channel Channel ID/ })).toBeInTheDocument()
    expect(setup.getByRole('row', { name: /^user User ID/ })).toBeInTheDocument()
    for (const key of ['text', 'summary', 'result', 'job', 'status'])
      expect(setup.getByRole('row', { name: new RegExp(`^${key} Text`) })).toBeInTheDocument()
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

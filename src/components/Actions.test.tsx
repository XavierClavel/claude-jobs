import { render, screen, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { api } from '../api'
import { action, job, recipient, use } from '../test/fixtures'
import { Actions } from './Actions'

vi.mock('../api', () => ({
  api: { deleteAction: vi.fn(), saveAction: vi.fn(), testAction: vi.fn() },
  errorsOf: () => [],
}))

const noop = () => {}

describe('Actions', () => {
  it('shows an action, its recipients, and which jobs send with it', async () => {
    const onSelect = vi.fn()
    render(
      <Actions
        actions={[action()]}
        jobs={[
          job({ id: 'a', name: 'Release digest', on_failure: [use()] }),
          job({ id: 'b', name: 'PR triage' }),
        ]}
        onChanged={() => Promise.resolve()}
        onSelect={onSelect}
      />,
    )
    const card = within(screen.getByRole('region', { name: 'Alert workflow' }))
    expect(card.getByText('Slack workflow')).toBeInTheDocument()
    expect(card.getByText('Ops channel', { selector: '.tag' })).toHaveAttribute(
      'title',
      'Channel C0123ABCD',
    )
    expect(
      card.getByText('https://hooks.slack.com/triggers/T0123/456/••••••••'),
    ).toBeInTheDocument()
    expect(card.queryByText(/s3cr3t/)).not.toBeInTheDocument()
    expect(card.getAllByText('No job')).toHaveLength(3)
    await userEvent.click(card.getByRole('button', { name: 'Release digest' }))
    expect(onSelect).toHaveBeenCalledWith('a')
  })

  it('sends a test to one of its recipients, and deletes on a second click', async () => {
    const onChanged = vi.fn(() => Promise.resolve())
    vi.mocked(api.testAction).mockResolvedValue()
    vi.mocked(api.deleteAction).mockResolvedValue()
    render(<Actions actions={[action()]} jobs={[]} onChanged={onChanged} onSelect={noop} />)
    const card = within(screen.getByRole('region', { name: 'Alert workflow' }))
    await userEvent.click(card.getByRole('button', { name: 'Send test to' }))
    expect(api.testAction).toHaveBeenCalledWith(action(), recipient())
    expect(card.getByRole('status')).toHaveTextContent('Sent to Ops channel')
    await userEvent.click(card.getByRole('button', { name: 'Delete' }))
    expect(api.deleteAction).not.toHaveBeenCalled()
    await userEvent.click(card.getByRole('button', { name: 'Click again to delete' }))
    expect(api.deleteAction).toHaveBeenCalledWith('alert')
    expect(onChanged).toHaveBeenCalledOnce()
  })

  it('says when an action has no recipient, and tests it with none', async () => {
    vi.mocked(api.testAction).mockResolvedValue()
    render(
      <Actions
        actions={[action({ recipients: [] })]}
        jobs={[]}
        onChanged={() => Promise.resolve()}
        onSelect={noop}
      />,
    )
    expect(screen.getByText('None')).toBeInTheDocument()
    await userEvent.click(screen.getByRole('button', { name: 'Send test to' }))
    expect(api.testAction).toHaveBeenCalledWith(action({ recipients: [] }), null)
  })

  it('opens the form to add an action, and to edit one', async () => {
    render(
      <Actions
        actions={[action()]}
        jobs={[]}
        onChanged={() => Promise.resolve()}
        onSelect={noop}
      />,
    )
    await userEvent.click(screen.getByRole('button', { name: 'Add action' }))
    expect(screen.getByRole('form', { name: 'New action' })).toBeInTheDocument()
    await userEvent.click(screen.getByRole('button', { name: 'Cancel' }))
    await userEvent.click(screen.getByRole('button', { name: 'Edit' }))
    expect(screen.getByRole('form', { name: 'Edit Alert workflow' })).toBeInTheDocument()
    expect(screen.queryByRole('region', { name: 'Alert workflow' })).not.toBeInTheDocument()
  })
})

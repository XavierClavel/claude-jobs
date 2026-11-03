import { render, screen, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { api } from '../api'
import { action, job } from '../test/fixtures'
import { Actions } from './Actions'

vi.mock('../api', () => ({
  api: { deleteAction: vi.fn(), saveAction: vi.fn(), testAction: vi.fn() },
  errorsOf: () => [],
}))

const noop = () => {}

describe('Actions', () => {
  it('shows where an action posts, and which jobs take it on success or failure', async () => {
    const onSelect = vi.fn()
    render(
      <Actions
        actions={[action()]}
        jobs={[
          job({ id: 'a', name: 'Release digest', on_failure: ['ops'] }),
          job({ id: 'b', name: 'PR triage' }),
        ]}
        onChanged={() => Promise.resolve()}
        onSelect={onSelect}
      />,
    )
    const card = within(screen.getByRole('region', { name: 'Ops channel' }))
    expect(card.getByText('Posts in channel C0123ABCD')).toBeInTheDocument()
    expect(card.getByText('Slack workflow')).toBeInTheDocument()
    expect(
      card.getByText('https://hooks.slack.com/triggers/T0123/456/••••••••'),
    ).toBeInTheDocument()
    expect(card.queryByText(/s3cr3t/)).not.toBeInTheDocument()
    expect(card.getAllByText('No job')).toHaveLength(2)
    await userEvent.click(card.getByRole('button', { name: 'Release digest' }))
    expect(onSelect).toHaveBeenCalledWith('a')
  })

  it('sends a test, and deletes on a second click', async () => {
    const onChanged = vi.fn(() => Promise.resolve())
    vi.mocked(api.testAction).mockResolvedValue()
    vi.mocked(api.deleteAction).mockResolvedValue()
    render(<Actions actions={[action()]} jobs={[]} onChanged={onChanged} onSelect={noop} />)
    const card = within(screen.getByRole('region', { name: 'Ops channel' }))
    await userEvent.click(card.getByRole('button', { name: 'Send test' }))
    expect(api.testAction).toHaveBeenCalledWith(action())
    expect(card.getByRole('status')).toHaveTextContent('Sent to channel C0123ABCD')
    await userEvent.click(card.getByRole('button', { name: 'Delete' }))
    expect(api.deleteAction).not.toHaveBeenCalled()
    await userEvent.click(card.getByRole('button', { name: 'Click again to delete' }))
    expect(api.deleteAction).toHaveBeenCalledWith('ops')
    expect(onChanged).toHaveBeenCalledOnce()
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
    expect(screen.getByRole('form', { name: 'Edit Ops channel' })).toBeInTheDocument()
    expect(screen.queryByRole('region', { name: 'Ops channel' })).not.toBeInTheDocument()
  })
})

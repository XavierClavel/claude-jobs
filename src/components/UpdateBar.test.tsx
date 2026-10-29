import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { UpdateBar } from './UpdateBar'

const update = { version: '0.2.0', current_version: '0.1.0', notes: null }

describe('UpdateBar', () => {
  it('offers the newer version', async () => {
    const onInstall = vi.fn()
    render(<UpdateBar update={update} installing={false} onInstall={onInstall} />)
    expect(screen.getByText('Claude Jobs 0.2.0 is out (you have 0.1.0).')).toBeInTheDocument()
    await userEvent.click(screen.getByRole('button', { name: 'Install and restart' }))
    expect(onInstall).toHaveBeenCalledOnce()
  })

  it('waits while the update installs', () => {
    render(<UpdateBar update={update} installing onInstall={() => {}} />)
    expect(screen.getByRole('button', { name: 'Installing…' })).toBeDisabled()
  })

  it('shows nothing when up to date', () => {
    const { container } = render(
      <UpdateBar update={null} installing={false} onInstall={() => {}} />,
    )
    expect(container).toBeEmptyDOMElement()
  })
})

import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { TmuxNotice } from './TmuxNotice'

describe('TmuxNotice', () => {
  it('offers to install tmux with Homebrew', async () => {
    const onInstall = vi.fn()
    render(
      <TmuxNotice
        status={{ installed: false, can_install: true }}
        installing={false}
        onInstall={onInstall}
      />,
    )
    expect(screen.getByText('tmux is not installed')).toBeInTheDocument()
    await userEvent.click(screen.getByRole('button', { name: 'Install tmux' }))
    expect(onInstall).toHaveBeenCalledOnce()
  })

  it('waits while Homebrew installs it', () => {
    render(
      <TmuxNotice
        status={{ installed: false, can_install: true }}
        installing
        onInstall={() => {}}
      />,
    )
    expect(screen.getByRole('button', { name: 'Installing tmux…' })).toBeDisabled()
  })

  it('points to Homebrew when it is missing too', () => {
    render(
      <TmuxNotice
        status={{ installed: false, can_install: false }}
        installing={false}
        onInstall={() => {}}
      />,
    )
    expect(screen.queryByRole('button')).not.toBeInTheDocument()
    expect(screen.getByText(/brew\.sh/)).toBeInTheDocument()
  })

  it('shows nothing once tmux is there', () => {
    const { container } = render(
      <TmuxNotice
        status={{ installed: true, can_install: true }}
        installing={false}
        onInstall={() => {}}
      />,
    )
    expect(container).toBeEmptyDOMElement()
  })
})

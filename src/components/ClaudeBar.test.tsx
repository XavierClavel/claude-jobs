import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { ClaudeBar } from './ClaudeBar'

describe('ClaudeBar', () => {
  it('says why jobs wait and offers to sign in', async () => {
    const onSignIn = vi.fn()
    render(
      <ClaudeBar
        status={{
          condition: 'claude',
          problem: "Claude Code's login expired: run claude, then /login.",
        }}
        onSignIn={onSignIn}
      />,
    )
    expect(screen.getByText(/login expired/)).toBeInTheDocument()
    await userEvent.click(screen.getByRole('button', { name: 'Sign in to Claude Code' }))
    expect(onSignIn).toHaveBeenCalledOnce()
  })

  it('shows nothing while Claude Code is signed in', () => {
    const { container } = render(
      <ClaudeBar status={{ condition: 'claude', problem: null }} onSignIn={() => {}} />,
    )
    expect(container).toBeEmptyDOMElement()
  })
})

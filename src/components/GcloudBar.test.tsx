import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { GcloudBar } from './GcloudBar'

describe('GcloudBar', () => {
  it('warns when the credentials have expired and offers to sign in', async () => {
    const onSignIn = vi.fn()
    render(
      <GcloudBar
        status={{ installed: true, signed_in: false, account: 'me@example.com' }}
        signingIn={false}
        onSignIn={onSignIn}
      />,
    )
    expect(screen.getByText('Google Cloud: signed out')).toBeInTheDocument()
    await userEvent.click(screen.getByRole('button', { name: 'Sign in to Google Cloud' }))
    expect(onSignIn).toHaveBeenCalledOnce()
  })

  it('shows the signed-in account', () => {
    render(
      <GcloudBar
        status={{ installed: true, signed_in: true, account: 'me@example.com' }}
        signingIn={false}
        onSignIn={() => {}}
      />,
    )
    expect(screen.getByText('me@example.com')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Sign in again' })).toBeInTheDocument()
  })

  it('waits for the browser while signing in', () => {
    render(
      <GcloudBar
        status={{ installed: true, signed_in: false, account: null }}
        signingIn
        onSignIn={() => {}}
      />,
    )
    expect(screen.getByRole('button', { name: 'Waiting for the browser…' })).toBeDisabled()
    expect(screen.getByText('Finish signing in in your browser.')).toBeInTheDocument()
  })

  it('shows nothing without gcloud', () => {
    const { container } = render(
      <GcloudBar
        status={{ installed: false, signed_in: false, account: null }}
        signingIn={false}
        onSignIn={() => {}}
      />,
    )
    expect(container).toBeEmptyDOMElement()
  })
})

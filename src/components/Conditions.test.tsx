import { render, screen, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { gcloudOut, job, signedIn } from '../test/fixtures'
import { Conditions } from './Conditions'

const noop = () => {}

describe('Conditions', () => {
  it('says which conditions are met and which jobs use them', () => {
    render(
      <Conditions
        jobs={[job({ id: 'a', name: 'BigQuery cost', conditions: ['gcloud'] }), job({ id: 'b' })]}
        checks={gcloudOut}
        actions={{ onSignIn: noop, onCheck: () => Promise.resolve() }}
        onSelect={noop}
      />,
    )
    const claude = within(screen.getByRole('region', { name: 'Claude Code signed in' }))
    expect(claude.getByText('Met')).toBeInTheDocument()
    expect(claude.getByText('all 2 jobs')).toBeInTheDocument()
    expect(claude.getByRole('button', { name: 'Sign in' })).toBeDisabled()
    const gcloud = within(screen.getByRole('region', { name: 'Google Cloud signed in' }))
    expect(gcloud.getByText('Not met')).toBeInTheDocument()
    expect(gcloud.getByRole('button', { name: 'BigQuery cost' })).toBeInTheDocument()
  })

  it('signs in, and checks again', async () => {
    const onSignIn = vi.fn()
    const onCheck = vi.fn(() => Promise.resolve())
    render(
      <Conditions
        jobs={[job()]}
        checks={gcloudOut}
        actions={{ onSignIn, onCheck }}
        onSelect={noop}
      />,
    )
    const gcloud = within(screen.getByRole('region', { name: 'Google Cloud signed in' }))
    await userEvent.click(gcloud.getByRole('button', { name: 'Sign in' }))
    expect(onSignIn).toHaveBeenCalledWith('gcloud')
    await userEvent.click(screen.getByRole('button', { name: 'Check all now' }))
    expect(onCheck).toHaveBeenCalledOnce()
  })

  it('shows the signed-in Google account', () => {
    render(
      <Conditions
        jobs={[job()]}
        checks={signedIn}
        actions={{ onSignIn: noop, onCheck: () => Promise.resolve() }}
        onSelect={noop}
      />,
    )
    expect(screen.getByText('me@example.com')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Sign in again' })).toBeEnabled()
  })
})

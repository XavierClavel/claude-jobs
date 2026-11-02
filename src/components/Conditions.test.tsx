import { render, screen, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { api } from '../api'
import { command, gcloudOut, job, signedIn } from '../test/fixtures'
import { Conditions } from './Conditions'

vi.mock('../api', () => ({
  api: { checkCondition: vi.fn(), deleteCondition: vi.fn(), saveCondition: vi.fn() },
  errorsOf: () => [],
}))

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

  it('shows a command condition: its command, what meets it, its check and its jobs', () => {
    const vpn = command({
      problem: 'Connect to the VPN: the command printed "Disconnected", not "Connected".',
    })
    render(
      <Conditions
        jobs={[job({ id: 'a', name: 'Release digest', conditions: ['vpn'] }), job({ id: 'b' })]}
        checks={{ ...signedIn, commands: [vpn] }}
        actions={{ onSignIn: noop, onCheck: () => Promise.resolve() }}
        onSelect={noop}
      />,
    )
    const card = within(screen.getByRole('region', { name: 'VPN connected' }))
    expect(card.getByText('Not met')).toBeInTheDocument()
    expect(card.getByText('Command · per job')).toBeInTheDocument()
    expect(card.getByText('scutil --nc status Work | head -1')).toBeInTheDocument()
    expect(card.getByText('prints “Connected”')).toBeInTheDocument()
    expect(card.getByText(/printed "Disconnected"/)).toBeInTheDocument()
    expect(card.getByRole('button', { name: 'Release digest' })).toBeInTheDocument()
    expect(card.queryByRole('button', { name: 'Sign in' })).not.toBeInTheDocument()
  })

  it('checks a command condition again, and deletes it on a second click', async () => {
    const onCheck = vi.fn(() => Promise.resolve())
    vi.mocked(api.checkCondition).mockResolvedValue({ problem: null, exit_code: 0, stdout: '' })
    vi.mocked(api.deleteCondition).mockResolvedValue()
    render(
      <Conditions
        jobs={[job()]}
        checks={{ ...signedIn, commands: [command()] }}
        actions={{ onSignIn: noop, onCheck }}
        onSelect={noop}
      />,
    )
    const card = within(screen.getByRole('region', { name: 'VPN connected' }))
    await userEvent.click(card.getByRole('button', { name: 'Check now' }))
    expect(api.checkCondition).toHaveBeenCalledWith(expect.objectContaining({ id: 'vpn' }))
    expect(onCheck).toHaveBeenCalledOnce()
    await userEvent.click(card.getByRole('button', { name: 'Delete' }))
    expect(api.deleteCondition).not.toHaveBeenCalled()
    await userEvent.click(card.getByRole('button', { name: 'Click again to delete' }))
    expect(api.deleteCondition).toHaveBeenCalledWith('vpn')
  })

  it('opens the form to add a condition, and to edit one', async () => {
    render(
      <Conditions
        jobs={[job()]}
        checks={{ ...signedIn, commands: [command()] }}
        actions={{ onSignIn: noop, onCheck: () => Promise.resolve() }}
        onSelect={noop}
      />,
    )
    await userEvent.click(screen.getByRole('button', { name: 'Add condition' }))
    expect(screen.getByRole('form', { name: 'New condition' })).toBeInTheDocument()
    await userEvent.click(screen.getByRole('button', { name: 'Cancel' }))
    await userEvent.click(screen.getByRole('button', { name: 'Edit' }))
    expect(screen.getByRole('form', { name: 'Edit VPN connected' })).toBeInTheDocument()
    expect(screen.queryByRole('region', { name: 'VPN connected' })).not.toBeInTheDocument()
  })
})

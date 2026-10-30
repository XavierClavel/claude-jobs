import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { UpdateCheck } from './UpdateCheck'

describe('UpdateCheck', () => {
  it('checks when clicked and says so when up to date', async () => {
    const onCheck = vi.fn().mockResolvedValue(true)
    render(<UpdateCheck onCheck={onCheck} />)
    await userEvent.click(screen.getByRole('button', { name: 'Check for updates' }))
    expect(onCheck).toHaveBeenCalledOnce()
    expect(await screen.findByText('Up to date')).toBeInTheDocument()
  })

  it('says nothing when the check found a version or failed', async () => {
    render(<UpdateCheck onCheck={() => Promise.resolve(false)} />)
    const button = screen.getByRole('button', { name: 'Check for updates' })
    await userEvent.click(button)
    await waitFor(() => expect(button).toBeEnabled())
    expect(screen.queryByText('Up to date')).not.toBeInTheDocument()
  })

  it('cannot be clicked again while it checks', async () => {
    render(<UpdateCheck onCheck={() => new Promise(() => {})} />)
    const button = screen.getByRole('button', { name: 'Check for updates' })
    await userEvent.click(button)
    expect(button).toBeDisabled()
  })
})

import { render, screen } from '@testing-library/react'
import { api } from '../api'
import { CronField } from './CronField'

vi.mock('../api', () => ({
  api: { cronPreview: vi.fn() },
  errorsOf: (e: { errors: string[] }) => e.errors,
}))

describe('CronField', () => {
  it('lists the next runs of an expression that reads', async () => {
    // Local times, so the test reads the same in any time zone.
    const at = (h: number, m: number) => new Date(2026, 10, 16, h, m).toISOString()
    vi.mocked(api.cronPreview).mockResolvedValue([at(9, 0), at(9, 15)])
    render(<CronField value="*/15 9-17 * * 1-5" onChange={() => {}} />)
    expect(await screen.findByLabelText('Next runs')).toHaveTextContent(/09:00 · .+09:15$/)
  })

  it('says what is wrong with one that does not', async () => {
    vi.mocked(api.cronPreview).mockRejectedValue({
      errors: ['Cron: the hour must be 0 to 23, not 25.'],
    })
    render(<CronField value="0 25 * * *" onChange={() => {}} />)
    expect(await screen.findByText('Cron: the hour must be 0 to 23, not 25.')).toBeInTheDocument()
    expect(screen.queryByLabelText('Next runs')).not.toBeInTheDocument()
  })

  it('asks nothing while it is empty', () => {
    vi.mocked(api.cronPreview).mockClear()
    render(<CronField value=" " onChange={() => {}} />)
    expect(api.cronPreview).not.toHaveBeenCalled()
  })
})

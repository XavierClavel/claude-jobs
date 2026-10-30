import { act, render, screen } from '@testing-library/react'
import { Elapsed } from './Elapsed'

describe('Elapsed', () => {
  afterEach(() => {
    vi.useRealTimers()
  })

  it('ticks on the second the run started at', () => {
    vi.useFakeTimers({ now: new Date('2026-10-30T09:12:34.400Z') })
    render(<Elapsed since="2026-10-30T09:00:00Z" />)
    expect(screen.getByText('12:34')).toBeInTheDocument()
    act(() => vi.advanceTimersByTime(599))
    expect(screen.getByText('12:34')).toBeInTheDocument()
    act(() => vi.advanceTimersByTime(1))
    expect(screen.getByText('12:35')).toBeInTheDocument()
    act(() => vi.advanceTimersByTime(48 * 60 * 1000))
    expect(screen.getByText('1:00:35')).toBeInTheDocument()
  })
})

import { act, renderHook, waitFor } from '@testing-library/react'
import { api } from './api'
import type { Layout } from './domain/Layout'
import { useLayout } from './useLayout'

vi.mock('./api', () => ({ api: { getLayout: vi.fn(), saveLayout: vi.fn() } }))

const saved: Layout = { ungrouped: ['a'], groups: [] }

beforeEach(() => {
  vi.mocked(api.getLayout).mockReset().mockResolvedValue(saved)
  vi.mocked(api.saveLayout).mockReset()
})

describe('useLayout', () => {
  it('keeps the layout when the jobs refresh with the same ids', async () => {
    const { result, rerender } = renderHook(({ ids }) => useLayout(ids, vi.fn()), {
      initialProps: { ids: ['a'] as string[] | null },
    })
    await waitFor(() => expect(result.current[0]).toEqual(saved))
    rerender({ ids: ['a'] })
    rerender({ ids: ['a'] })
    expect(api.getLayout).toHaveBeenCalledOnce()
  })

  it('loads the layout again when a job comes or goes', async () => {
    const { rerender } = renderHook(({ ids }) => useLayout(ids, vi.fn()), {
      initialProps: { ids: ['a'] as string[] | null },
    })
    await waitFor(() => expect(api.getLayout).toHaveBeenCalledOnce())
    rerender({ ids: ['a', 'b'] })
    await waitFor(() => expect(api.getLayout).toHaveBeenCalledTimes(2))
  })

  it('shows a change at once, then keeps what was saved', async () => {
    const next: Layout = {
      ungrouped: [],
      groups: [{ id: 'g', name: 'G', collapsed: false, jobs: ['a'] }],
    }
    let finish: (layout: Layout) => void = () => {}
    vi.mocked(api.saveLayout).mockReturnValue(new Promise((resolve) => (finish = resolve)))
    const { result } = renderHook(() => useLayout(['a'], vi.fn()))
    await waitFor(() => expect(result.current[0]).toEqual(saved))
    act(() => result.current[1](next))
    expect(result.current[0]).toEqual(next)
    await act(async () => finish(next))
    expect(api.saveLayout).toHaveBeenCalledWith(next)
  })

  it('says a save that fails, and loads the saved layout again', async () => {
    const onError = vi.fn()
    vi.mocked(api.saveLayout).mockRejectedValue({ errors: ['disk full'] })
    const { result } = renderHook(() => useLayout(['a'], onError))
    await waitFor(() => expect(result.current[0]).toEqual(saved))
    await act(async () => result.current[1]({ ungrouped: [], groups: [] }))
    await waitFor(() => expect(result.current[0]).toEqual(saved))
    expect(onError).toHaveBeenCalledWith({ errors: ['disk full'] })
    expect(api.getLayout).toHaveBeenCalledTimes(2)
  })

  it('says a layout that cannot be read, and leaves it to the jobs', async () => {
    const onError = vi.fn()
    vi.mocked(api.getLayout).mockRejectedValue({ errors: ['layout.json does not parse'] })
    const { result } = renderHook(() => useLayout(['a'], onError))
    await waitFor(() => expect(onError).toHaveBeenCalled())
    expect(result.current[0]).toBeNull()
  })
})

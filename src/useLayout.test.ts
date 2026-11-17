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

  it('keeps the latest change when an earlier save answers last', async () => {
    const first: Layout = {
      ungrouped: [],
      groups: [{ id: 'g', name: 'G', collapsed: true, jobs: ['a'] }],
    }
    const second: Layout = {
      ungrouped: [],
      groups: [{ id: 'g', name: 'G', collapsed: false, jobs: ['a'] }],
    }
    const answers: ((layout: Layout) => void)[] = []
    vi.mocked(api.saveLayout).mockImplementation(
      () => new Promise((resolve) => answers.push(resolve)),
    )
    const { result } = renderHook(() => useLayout(['a'], vi.fn()))
    await waitFor(() => expect(result.current[0]).toEqual(saved))
    act(() => result.current[1](first))
    act(() => result.current[1](second))
    await act(async () => answers[1]?.(second))
    await act(async () => answers[0]?.(first))
    expect(result.current[0]).toEqual(second)
  })

  it('saves one change at a time, in order', async () => {
    const first: Layout = {
      ungrouped: [],
      groups: [{ id: 'g', name: 'G', collapsed: true, jobs: ['a'] }],
    }
    const second: Layout = { ungrouped: ['a'], groups: [] }
    const answers: ((layout: Layout) => void)[] = []
    vi.mocked(api.saveLayout).mockImplementation(
      () => new Promise((resolve) => answers.push(resolve)),
    )
    const { result } = renderHook(() => useLayout(['a'], vi.fn()))
    await waitFor(() => expect(result.current[0]).toEqual(saved))
    act(() => result.current[1](first))
    act(() => result.current[1](second))
    expect(api.saveLayout).toHaveBeenCalledTimes(1)
    await act(async () => answers[0]?.(first))
    await waitFor(() => expect(api.saveLayout).toHaveBeenLastCalledWith(second))
  })

  it('does not let a load started before a change undo it', async () => {
    let answer: (layout: Layout) => void = () => {}
    const next: Layout = {
      ungrouped: [],
      groups: [{ id: 'g', name: 'G', collapsed: false, jobs: ['a', 'b'] }],
    }
    vi.mocked(api.saveLayout).mockResolvedValue(next)
    const { result, rerender } = renderHook(({ ids }) => useLayout(ids, vi.fn()), {
      initialProps: { ids: ['a'] },
    })
    await waitFor(() => expect(result.current[0]).toEqual(saved))
    vi.mocked(api.getLayout).mockReturnValue(new Promise((resolve) => (answer = resolve)))
    rerender({ ids: ['a', 'b'] })
    await act(async () => result.current[1](next))
    await act(async () => answer(saved))
    expect(result.current[0]).toEqual(next)
  })

  it('is not ready until the layout has loaded', async () => {
    let answer: (layout: Layout) => void = () => {}
    vi.mocked(api.getLayout).mockReturnValue(new Promise((resolve) => (answer = resolve)))
    const { result } = renderHook(() => useLayout(['a'], vi.fn()))
    expect(result.current[2]).toBe(false)
    await act(async () => answer(saved))
    expect(result.current[2]).toBe(true)
  })
})

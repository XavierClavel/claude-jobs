import { type RefObject, useEffect } from 'react'

// While a picker is open, a click outside `ref` or Escape closes it.
export function useDismiss(
  open: boolean,
  ref: RefObject<HTMLElement | null>,
  setOpen: (open: boolean) => void,
) {
  useEffect(() => {
    if (!open) return
    const away = (e: MouseEvent) => {
      if (!ref.current?.contains(e.target as Node)) setOpen(false)
    }
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') setOpen(false)
    }
    document.addEventListener('mousedown', away)
    document.addEventListener('keydown', onKey)
    return () => {
      document.removeEventListener('mousedown', away)
      document.removeEventListener('keydown', onKey)
    }
  }, [open, ref, setOpen])
}

import { type ReactNode, useEffect, useRef } from 'react'

// A native modal <dialog>, opened and closed from React state. Escape closes it through `onClose`.
export function Dialog({
  open,
  onClose,
  labelledBy,
  children,
}: {
  open: boolean
  onClose: () => void
  labelledBy: string
  children: ReactNode
}) {
  const ref = useRef<HTMLDialogElement>(null)
  useEffect(() => {
    const dialog = ref.current
    if (!dialog) return
    if (open && !dialog.open) dialog.showModal()
    if (!open && dialog.open) dialog.close()
  }, [open])
  return (
    <dialog ref={ref} aria-labelledby={labelledBy} onClose={onClose}>
      {open && children}
    </dialog>
  )
}

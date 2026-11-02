import type { ReactNode } from 'react'

// A checkbox drawn as a switch, with what it does under its label.
export function Switch({
  label,
  checked,
  onChange,
  children,
}: {
  label: string
  checked: boolean
  onChange: (on: boolean) => void
  children: ReactNode
}) {
  return (
    <label className="switch">
      <input
        type="checkbox"
        role="switch"
        aria-checked={checked}
        checked={checked}
        onChange={(e) => onChange(e.target.checked)}
      />
      <span className="knob" />
      <span>
        <span className="switch-label">{label}</span>
        <span className="switch-text">{children}</span>
      </span>
    </label>
  )
}

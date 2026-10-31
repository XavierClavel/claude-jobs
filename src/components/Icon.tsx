// Line icons on a 24-unit grid, drawn in the text colour.
const PATHS = {
  plus: 'M12 5v14M5 12h14',
  refresh: 'M21 12a9 9 0 1 1-2.64-6.36M21 3v6h-6',
  back: 'M15 6l-6 6 6 6',
  chevron: 'M9 6l6 6-6 6',
  close: 'M6 6l12 12M18 6L6 18',
  check: 'M5 12l5 5L20 7',
  clock: 'M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18zM12 7v5l3 2',
  lock: 'M7 11h10a2 2 0 0 1 2 2v6a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2v-6a2 2 0 0 1 2-2zM8 11V7a4 4 0 0 1 8 0v4',
} as const

export type IconName = keyof typeof PATHS | 'play'

export function Icon({ name, size = 14 }: { name: IconName; size?: number }) {
  return (
    <svg
      className="icon-svg"
      width={size}
      height={size}
      viewBox="0 0 24 24"
      aria-hidden="true"
      fill={name === 'play' ? 'currentColor' : 'none'}
      stroke={name === 'play' ? 'none' : 'currentColor'}
      strokeWidth="2.2"
      strokeLinecap="round"
      strokeLinejoin="round"
    >
      <path d={name === 'play' ? 'M7 4v16l13-8z' : PATHS[name]} />
    </svg>
  )
}

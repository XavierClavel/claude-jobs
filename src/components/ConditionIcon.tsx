import { useId } from 'react'

// The marks of the conditions: Claude's spark for Claude Code, a four-colour cloud for Google Cloud, and a terminal for a
// command condition, which runs a bash command. Decorative: the condition's name says the rest.

// The spark's rays: angle in degrees from the top, and length from the centre.
const RAYS: [number, number][] = Array.from({ length: 12 }, (_, i) => [i * 30 + 8, i % 2 ? 7 : 9.6])

function ClaudeMark() {
  return (
    <g stroke="#D97757" strokeWidth="2.3" strokeLinecap="round">
      {RAYS.map(([angle, length]) => {
        const rad = (angle * Math.PI) / 180
        return (
          <line
            key={angle}
            x1={12 + Math.sin(rad) * 1.6}
            y1={12 - Math.cos(rad) * 1.6}
            x2={12 + Math.sin(rad) * length}
            y2={12 - Math.cos(rad) * length}
          />
        )
      })}
    </g>
  )
}

// A cloud's outline, drawn once per colour, each time inside its own part of the square.
const CLOUD = 'M7 18.5h10.2a4.3 4.3 0 0 0 .6-8.56A6 6 0 0 0 6.4 9.2 4.7 4.7 0 0 0 7 18.5z'
const PARTS: [string, string, string][] = [
  ['#EA4335', 'top', 'M0 0h24v10.4H0z'],
  ['#4285F4', 'right', 'M16 10.4h8V24h-8z'],
  ['#34A853', 'bottom', 'M8 10.4h8V24H8z'],
  ['#FBBC04', 'left', 'M0 10.4h8V24H0z'],
]

function GcloudMark({ id }: { id: string }) {
  return (
    <>
      <defs>
        {PARTS.map(([, part, d]) => (
          <clipPath key={part} id={`${id}-${part}`}>
            <path d={d} />
          </clipPath>
        ))}
      </defs>
      {PARTS.map(([color, part]) => (
        <path
          key={part}
          d={CLOUD}
          fill="none"
          stroke={color}
          strokeWidth="2.4"
          strokeLinejoin="round"
          clipPath={`url(#${id}-${part})`}
        />
      ))}
    </>
  )
}

// A terminal's prompt, `>_`, in the text's colour.
function TerminalMark() {
  return (
    <g
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
    >
      <rect x="2.5" y="3.5" width="19" height="17" rx="3.5" />
      <path d="M7 9.5l3 2.5-3 2.5M12.5 15h4.5" />
    </g>
  )
}

export function ConditionIcon({ condition, size = 14 }: { condition: string; size?: number }) {
  // Clip paths are found by id in the whole document: each icon needs its own.
  const id = useId().replace(/:/g, '')
  return (
    <svg
      className="action-icon"
      width={size}
      height={size}
      viewBox="0 0 24 24"
      aria-hidden="true"
      data-condition={condition}
    >
      {condition === 'claude' ? (
        <ClaudeMark />
      ) : condition === 'gcloud' ? (
        <GcloudMark id={id} />
      ) : (
        <TerminalMark />
      )}
    </svg>
  )
}

import { useId } from 'react'

// The marks of the conditions: Anthropic's mark for Claude Code, a four-colour cloud for Google Cloud, and a terminal for a
// command condition, which runs a bash command. Decorative: the condition's name says the rest.

// Anthropic's mark, from its brand assets (as Simple Icons has it), in the text's colour: its own near-black would
// not show in dark mode.
const ANTHROPIC =
  'M17.3041 3.541h-3.6718l6.696 16.918H24Zm-10.6082 0L0 20.459h3.7442l1.3693-3.5527h7.0052l1.3693 3.5528h3.7442L10.5363 3.5409Zm-.3712 10.2232 2.2914-5.9456 2.2914 5.9456Z'

function AnthropicMark() {
  return <path fill="currentColor" d={ANTHROPIC} />
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
        <AnthropicMark />
      ) : condition === 'gcloud' ? (
        <GcloudMark id={id} />
      ) : (
        <TerminalMark />
      )}
    </svg>
  )
}

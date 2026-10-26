import type { ReactNode } from 'react'
import type { RunStatus } from '../domain/RunStatus'

export type Tone = 'ok' | 'bad' | 'running' | 'muted' | 'warn'

export function runTone(status: RunStatus): Tone {
  if (status === 'ok') return 'ok'
  if (status === 'running') return 'running'
  return 'bad'
}

export function Chip({ tone, children }: { tone: Tone; children: ReactNode }) {
  return <span className={`chip chip-${tone}`}>{children}</span>
}

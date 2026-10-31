import type { Tone } from './components/Chip'
import type { Condition } from './domain/Condition'
import type { ConditionStatus } from './domain/ConditionStatus'
import type { JobView } from './domain/JobView'
import type { RunStatus } from './domain/RunStatus'

export const CONDITION_NAME: Record<Condition, string> = {
  claude: 'Claude Code',
  gcloud: 'Google Cloud',
}

// Every job needs Claude Code; the others only the jobs that tick them.
export const required = (job: Pick<JobView, 'conditions'>): Condition[] => [
  'claude',
  ...job.conditions.filter((c) => c !== 'claude'),
]

export const failed = (status: RunStatus | undefined) =>
  status === 'failed' || status === 'timeout' || status === 'interrupted'

export const isRunning = (job: JobView) => job.last_run?.status === 'running'

// A paused job does not start, so what it would wait for only matters once a run waits.
export const unmetOf = (job: JobView): ConditionStatus[] =>
  job.waiting || job.enabled ? job.unmet_conditions : []

// What a job is doing, as one colour: its dot in the list, and the tone of what the detail says about it.
export function jobTone(job: JobView): Tone {
  if (isRunning(job)) return 'running'
  if (job.waiting) return 'warn'
  if (!job.enabled) return 'muted'
  if (!job.loaded || unmetOf(job).length) return 'warn'
  if (failed(job.last_run?.status)) return 'bad'
  return 'ok'
}

export const waitsFor = (unmet: ConditionStatus[]) =>
  unmet.map((c) => CONDITION_NAME[c.condition]).join(' and ')

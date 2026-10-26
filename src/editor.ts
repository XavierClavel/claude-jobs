import type { Defaults } from './domain/Defaults'
import type { Job } from './domain/Job'
import { clock } from './format'

// What the editor's fields hold: text, as typed. `toJob` turns it into the job the Rust side validates.
export type Draft = {
  name: string
  id: string
  prompt: string
  time: string
  timeout: string
  weekdays: number[]
  tools: string
  cwd: string
  model: string
  claude: string
  path: string
  enabled: boolean
}

export function toDraft(job: Job | null, defaults: Defaults | null): Draft {
  return {
    name: job?.name ?? '',
    id: job?.id ?? '',
    prompt: job?.prompt ?? '',
    time: job ? clock(job.hour, job.minute) : '09:00',
    timeout: String(job?.timeout_minutes ?? 30),
    weekdays: job?.weekdays ?? [],
    tools: job?.allowed_tools.join('\n') ?? '',
    cwd: job?.cwd ?? defaults?.cwd ?? '',
    model: job?.model ?? '',
    claude: job?.claude ?? defaults?.claude ?? '',
    path: job?.path ?? defaults?.path ?? '',
    enabled: job?.enabled ?? true,
  }
}

export function toJob(draft: Draft): Job {
  const [hour = 0, minute = 0] = draft.time.split(':').map(Number)
  return {
    id: draft.id.trim(),
    name: draft.name,
    prompt: draft.prompt,
    hour,
    minute,
    weekdays: [...draft.weekdays].sort((a, b) => a - b),
    cwd: draft.cwd,
    allowed_tools: draft.tools.split('\n'),
    model: draft.model,
    timeout_minutes: Number(draft.timeout) || 0,
    enabled: draft.enabled,
    claude: draft.claude,
    path: draft.path.trim(),
  }
}

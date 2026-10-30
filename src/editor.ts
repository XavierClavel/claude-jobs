import type { Condition } from './domain/Condition'
import type { Defaults } from './domain/Defaults'
import type { Job } from './domain/Job'
import { clock } from './format'

export type Repeat = 'daily' | 'weekly' | 'monthly'

// What the editor's fields hold: text, as typed. `toJob` turns it into the job the Rust side validates.
export type Draft = {
  name: string
  id: string
  prompt: string
  time: string
  timeout: string
  repeat: Repeat
  // Both kept while the other repeat is picked, so switching back restores them; `toJob` sends only one.
  weekdays: number[]
  monthDays: number[]
  lastDay: boolean
  tools: string
  cwd: string
  model: string
  claude: string
  path: string
  enabled: boolean
  interactive: boolean
  // What the job needs on top of Claude Code signed in, which every job needs.
  conditions: Condition[]
}

export function toDraft(job: Job | null, defaults: Defaults | null): Draft {
  return {
    name: job?.name ?? '',
    id: job?.id ?? '',
    prompt: job?.prompt ?? '',
    time: job ? clock(job.hour, job.minute) : '09:00',
    timeout: String(job?.timeout_minutes ?? 30),
    repeat: job?.month_days.length ? 'monthly' : job?.weekdays.length ? 'weekly' : 'daily',
    weekdays: job?.weekdays ?? [],
    monthDays: job?.month_days ?? [],
    lastDay: job?.last_day_fallback ?? false,
    tools: job?.allowed_tools.join('\n') ?? '',
    cwd: job?.cwd ?? defaults?.cwd ?? '',
    model: job?.model ?? '',
    claude: job?.claude ?? defaults?.claude ?? '',
    path: job?.path ?? defaults?.path ?? '',
    enabled: job?.enabled ?? true,
    interactive: job?.interactive ?? false,
    conditions: job?.conditions ?? [],
  }
}

// A week or a month with no day picked means nothing, so switching to one starts on its first day.
export function withRepeat(draft: Draft, repeat: Repeat): Draft {
  if (repeat === 'weekly' && !draft.weekdays.length) return { ...draft, repeat, weekdays: [1] }
  if (repeat === 'monthly' && !draft.monthDays.length) return { ...draft, repeat, monthDays: [1] }
  return { ...draft, repeat }
}

// The Rust side reads no day as every day, so a repeat left without a day has to be caught here.
export function draftErrors(draft: Draft): string[] {
  if (draft.repeat === 'weekly' && !draft.weekdays.length)
    return ['Pick at least one day of the week.']
  if (draft.repeat === 'monthly' && !draft.monthDays.length)
    return ['Pick at least one day of the month.']
  return []
}

const sorted = (days: number[]) => [...days].sort((a, b) => a - b)

// Every month has the 1st to the 28th; only a later day can be missing.
export const hasShortMonths = (monthDays: number[]) => monthDays.some((d) => d > 28)

export function toJob(draft: Draft): Job {
  const [hour = 0, minute = 0] = draft.time.split(':').map(Number)
  return {
    id: draft.id.trim(),
    name: draft.name,
    prompt: draft.prompt,
    hour,
    minute,
    weekdays: draft.repeat === 'weekly' ? sorted(draft.weekdays) : [],
    month_days: draft.repeat === 'monthly' ? sorted(draft.monthDays) : [],
    last_day_fallback:
      draft.repeat === 'monthly' && draft.lastDay && hasShortMonths(draft.monthDays),
    cwd: draft.cwd,
    allowed_tools: draft.tools.split('\n'),
    model: draft.model,
    timeout_minutes: Number(draft.timeout) || 0,
    enabled: draft.enabled,
    claude: draft.claude,
    path: draft.path.trim(),
    interactive: draft.interactive,
    conditions: draft.conditions,
  }
}

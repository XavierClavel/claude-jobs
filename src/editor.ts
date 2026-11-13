import type { ActionUse } from './domain/ActionUse'
import type { Defaults } from './domain/Defaults'
import type { EventKind } from './domain/EventKind'
import type { Job } from './domain/Job'
import { clock } from './format'

// `cron`: a cron expression, for times the others cannot say.
export type Repeat = 'daily' | 'weekly' | 'monthly' | 'cron'

// What starts the job: its schedule, or a trigger, such as a pull request opened in a repository it watches.
export type Starts = 'schedule' | 'trigger'

// What the editor's fields hold: text, as typed. `toJob` turns it into the job the Rust side validates.
export type Draft = {
  name: string
  id: string
  prompt: string
  starts: Starts
  // Which trigger, kept while the schedule is picked.
  trigger: EventKind
  time: string
  timeout: string
  repeat: Repeat
  // Both kept while the other repeat is picked, so switching back restores them; `toJob` sends only one.
  weekdays: number[]
  monthDays: number[]
  lastDay: boolean
  // The cron expression, as typed; kept, like the days, while another repeat is picked.
  cron: string
  // The repositories a pull request job watches, `owner/name`, and whether only your pull requests start it. Kept
  // while the schedule is picked, so switching back restores them.
  repos: string[]
  mine: boolean
  tools: string
  cwd: string
  model: string
  claude: string
  path: string
  enabled: boolean
  interactive: boolean
  chrome: boolean
  // What the job needs on top of Claude Code signed in, which every job needs.
  conditions: string[]
  // The actions Claude may send with during a run, and those taken when a run ends, each with who it sends to.
  duringRun: ActionUse[]
  onSuccess: ActionUse[]
  onFailure: ActionUse[]
  onComplete: ActionUse[]
}

export function toDraft(job: Job | null, defaults: Defaults | null): Draft {
  return {
    name: job?.name ?? '',
    id: job?.id ?? '',
    prompt: job?.prompt ?? '',
    starts: job?.event ? 'trigger' : 'schedule',
    trigger: job?.event?.kind ?? 'pull_request_opened',
    time: job ? clock(job.hour, job.minute) : '09:00',
    timeout: String(job?.timeout_minutes ?? 30),
    repeat: job?.cron
      ? 'cron'
      : job?.month_days.length
        ? 'monthly'
        : job?.weekdays.length
          ? 'weekly'
          : 'daily',
    weekdays: job?.weekdays ?? [],
    monthDays: job?.month_days ?? [],
    lastDay: job?.last_day_fallback ?? false,
    cron: job?.cron ?? '',
    repos: job?.event?.repos ?? [],
    mine: job?.event?.mine ?? true,
    tools: job?.allowed_tools.join('\n') ?? '',
    cwd: job?.cwd ?? defaults?.cwd ?? '',
    model: job?.model ?? '',
    claude: job?.claude ?? defaults?.claude ?? '',
    path: job?.path ?? defaults?.path ?? '',
    enabled: job?.enabled ?? true,
    interactive: job?.interactive ?? false,
    chrome: job?.chrome ?? false,
    conditions: job?.conditions ?? [],
    duringRun: job?.during_run ?? [],
    onSuccess: job?.on_success ?? [],
    onFailure: job?.on_failure ?? [],
    onComplete: job?.on_complete ?? [],
  }
}

// A week or a month with no day picked means nothing, so switching to one starts on its first day. A cron expression
// starts as the schedule picked so far, to change from there.
export function withRepeat(draft: Draft, repeat: Repeat): Draft {
  if (repeat === 'weekly' && !draft.weekdays.length) return { ...draft, repeat, weekdays: [1] }
  if (repeat === 'monthly' && !draft.monthDays.length) return { ...draft, repeat, monthDays: [1] }
  if (repeat === 'cron' && !draft.cron.trim()) return { ...draft, repeat, cron: cronOf(draft) }
  return { ...draft, repeat }
}

// The draft's time and days as a cron expression: "30 9 * * 1,5". Sunday is 0, which every cron reads.
export function cronOf(draft: Draft): string {
  const [hour = 0, minute = 0] = draft.time.split(':').map(Number)
  const list = (days: number[]) => (days.length ? sorted(days).join(',') : '*')
  const monthDays = draft.repeat === 'monthly' ? list(draft.monthDays) : '*'
  const weekdays = draft.repeat === 'weekly' ? list(draft.weekdays.map((d) => d % 7)) : '*'
  return `${minute} ${hour} ${monthDays} * ${weekdays}`
}

// The Rust side reads no day as every day, so a repeat left without a day has to be caught here.
export function draftErrors(draft: Draft): string[] {
  if (draft.starts === 'trigger') return draft.repos.length ? [] : ['Add a repository to watch.']
  if (draft.repeat === 'weekly' && !draft.weekdays.length)
    return ['Pick at least one day of the week.']
  if (draft.repeat === 'monthly' && !draft.monthDays.length)
    return ['Pick at least one day of the month.']
  if (draft.repeat === 'cron' && !draft.cron.trim()) return ['Enter a cron expression.']
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
    cron: draft.repeat === 'cron' ? draft.cron.trim() : null,
    cwd: draft.cwd,
    allowed_tools: draft.tools.split('\n'),
    model: draft.model,
    timeout_minutes: Number(draft.timeout) || 0,
    enabled: draft.enabled,
    claude: draft.claude,
    path: draft.path.trim(),
    interactive: draft.interactive,
    chrome: draft.chrome,
    conditions: draft.conditions,
    during_run: draft.duringRun,
    on_success: draft.onSuccess,
    on_failure: draft.onFailure,
    on_complete: draft.onComplete,
    event:
      draft.starts === 'trigger'
        ? { kind: draft.trigger, repos: draft.repos, mine: draft.mine }
        : null,
  }
}

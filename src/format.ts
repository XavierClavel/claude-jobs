import type { Job } from './domain/Job'
import type { RunStatus } from './domain/RunStatus'
import type { Trigger } from './domain/Trigger'

export const DAYS = ['Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat', 'Sun'] as const

export const MONTH_DAYS = Array.from({ length: 31 }, (_, i) => i + 1)

const SUFFIX: Record<number, string> = {
  1: 'st',
  2: 'nd',
  3: 'rd',
  21: 'st',
  22: 'nd',
  23: 'rd',
  31: 'st',
}

// For a day of the month, 1 to 31.
export function ordinal(day: number): string {
  return `${day}${SUFFIX[day] ?? 'th'}`
}

export const STATUS_LABEL: Record<RunStatus, string> = {
  running: 'Running',
  ok: 'OK',
  failed: 'Failed',
  timeout: 'Timed out',
  interrupted: 'Interrupted',
}

export const TRIGGER_LABEL: Record<Trigger, string> = {
  schedule: 'Schedule',
  manual: 'By hand',
  'catch-up': 'Catch-up',
}

const pad = (n: number) => String(n).padStart(2, '0')

export function clock(hour: number, minute: number): string {
  return `${pad(hour)}:${pad(minute)}`
}

export function scheduleText(
  job: Pick<Job, 'hour' | 'minute' | 'weekdays' | 'month_days' | 'last_day_fallback'>,
): string {
  const time = clock(job.hour, job.minute)
  if (job.month_days.length) {
    const shorter = job.last_day_fallback ? ', or the last day of shorter months' : ''
    return `Monthly on the ${job.month_days.map(ordinal).join(', ')} at ${time}${shorter}`
  }
  if (!job.weekdays.length) return `Every day at ${time}`
  return `${job.weekdays.map((d) => DAYS[d - 1] ?? '?').join(', ')} at ${time}`
}

export function dateTime(iso: string | null | undefined): string {
  if (!iso) return '—'
  return new Date(iso).toLocaleString(undefined, {
    weekday: 'short',
    day: 'numeric',
    month: 'short',
    hour: '2-digit',
    minute: '2-digit',
  })
}

export function ago(iso: string, now: number = Date.now()): string {
  const s = Math.max(0, Math.round((now - new Date(iso).getTime()) / 1000))
  if (s < 90) return `${s} s ago`
  if (s < 5400) return `${Math.round(s / 60)} min ago`
  if (s < 129600) return `${Math.round(s / 3600)} h ago`
  return `${Math.round(s / 86400)} days ago`
}

export function duration(seconds: number | null | undefined): string {
  if (seconds === null || seconds === undefined) return '—'
  if (seconds < 90) return `${seconds} s`
  return `${Math.floor(seconds / 60)} min ${seconds % 60} s`
}

// A run still going, as a stopwatch: 0:07, 12:34, then 1:02:05 past an hour.
export function stopwatch(seconds: number): string {
  const s = Math.max(0, Math.floor(seconds))
  const minutes = Math.floor(s / 60)
  if (minutes < 60) return `${minutes}:${pad(s % 60)}`
  return `${Math.floor(minutes / 60)}:${pad(minutes % 60)}:${pad(s % 60)}`
}

export function cost(usd: number | null | undefined): string {
  if (usd === null || usd === undefined) return '—'
  return `$${usd.toFixed(2)}`
}

export function slug(name: string): string {
  return name
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '')
    .slice(0, 40)
}

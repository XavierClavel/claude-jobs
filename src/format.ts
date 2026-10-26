import type { Job } from './domain/Job'
import type { RunStatus } from './domain/RunStatus'

export const DAYS = ['Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat', 'Sun'] as const

export const STATUS_LABEL: Record<RunStatus, string> = {
  running: 'Running',
  ok: 'OK',
  failed: 'Failed',
  timeout: 'Timed out',
  interrupted: 'Interrupted',
}

const pad = (n: number) => String(n).padStart(2, '0')

export function clock(hour: number, minute: number): string {
  return `${pad(hour)}:${pad(minute)}`
}

export function scheduleText(job: Pick<Job, 'hour' | 'minute' | 'weekdays'>): string {
  const time = clock(job.hour, job.minute)
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

import { invoke } from '@tauri-apps/api/core'
import type { ApiError } from './domain/ApiError'
import type { Defaults } from './domain/Defaults'
import type { Job } from './domain/Job'
import type { JobView } from './domain/JobView'
import type { RunDetail } from './domain/RunDetail'
import type { RunRecord } from './domain/RunRecord'

// One function per Rust command in `src-tauri/src/commands.rs`.
export const api = {
  listJobs: () => invoke<JobView[]>('list_jobs'),
  defaults: () => invoke<Defaults>('defaults'),
  saveJob: (job: Job, editing: string | null) => invoke<JobView>('save_job', { job, editing }),
  deleteJob: (id: string) => invoke<void>('delete_job', { id }),
  setEnabled: (id: string, enabled: boolean) => invoke<JobView>('set_enabled', { id, enabled }),
  runNow: (id: string) => invoke<void>('run_now', { id }),
  listRuns: (id: string) => invoke<RunRecord[]>('list_runs', { id }),
  getRun: (id: string, runId: string) => invoke<RunDetail>('get_run', { id, runId }),
}

export function errorsOf(error: unknown): string[] {
  if (error && typeof error === 'object' && Array.isArray((error as ApiError).errors)) {
    return (error as ApiError).errors
  }
  if (typeof error === 'string') return [error]
  if (error instanceof Error) return [error.message]
  return ['Something went wrong.']
}

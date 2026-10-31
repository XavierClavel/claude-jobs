import { useEffect, useState } from 'react'
import { api } from '../api'
import {
  type CheckActions,
  type Checks,
  conditionState,
  shownConditions,
  usedBy,
} from '../conditions'
import type { Condition } from '../domain/Condition'
import type { JobView } from '../domain/JobView'
import type { RunRecord } from '../domain/RunRecord'
import { clock, cost, duration, midSentence, STATUS_LABEL, tally, when } from '../format'
import { CONDITION_NAME, failed, isRunning, unmetOf, waitsFor } from '../jobs'
import { Chip } from './Chip'
import { Elapsed } from './Elapsed'

const WEEK = 7 * 86400000

// With no job selected: what is going on now, what comes next, and what needs a hand.
export function Overview({
  jobs,
  checks,
  actions,
  onSelect,
  onOpenRun,
}: {
  jobs: JobView[]
  checks: Checks
  actions: CheckActions
  onSelect: (id: string) => void
  onOpenRun: (job: string, run: string) => void
}) {
  const running = jobs.filter(isRunning)
  const waiting = jobs.filter((j) => j.waiting)
  const scheduled = jobs.filter((j) => j.enabled).length
  const paused = jobs.length - scheduled
  const week = useWeek(jobs)
  const upNext = jobs
    .filter((j) => j.enabled && j.next_run)
    .sort((a, b) => Date.parse(a.next_run ?? '') - Date.parse(b.next_run ?? ''))
  const attention = needsAttention(jobs, checks)

  return (
    <div className="view scroll-view">
      <header className="view-head">
        <div className="view-title">
          <h1>
            {new Date().toLocaleDateString('en-GB', {
              weekday: 'long',
              day: 'numeric',
              month: 'long',
            })}
          </h1>
          <span className="muted">
            {jobs.length === 0
              ? 'No jobs yet'
              : `${scheduled} ${scheduled === 1 ? 'job' : 'jobs'} scheduled${paused ? `, ${paused} paused` : ''} · select a job for its details`}
          </span>
        </div>
      </header>

      <div className="view-body">
        {jobs.length === 0 ? (
          <div className="empty">
            <p>No jobs yet.</p>
            <p className="muted">
              A job runs a Claude Code prompt on a schedule, like /photobook-monitor every morning.
            </p>
          </div>
        ) : (
          <>
            <div className="stats">
              {running[0]?.last_run ? (
                <button
                  type="button"
                  className="stat stat-running"
                  onClick={() => running[0] && onSelect(running[0].id)}
                >
                  <span className="stat-value">
                    {running.length} running <Elapsed since={running[0].last_run.started_at} />
                  </span>
                  <span className="stat-key">
                    {running.map((j) => j.name).join(', ')}, since{' '}
                    {timeOf(running[0].last_run.started_at)}
                  </span>
                </button>
              ) : (
                <div className="stat">
                  <span className="stat-value">Nothing running</span>
                  <span className="stat-key">
                    {upNext[0]?.next_run
                      ? `Next: ${upNext[0].name}, ${midSentence(when(upNext[0].next_run))}`
                      : 'Nothing scheduled'}
                  </span>
                </div>
              )}
              <div className="stat">
                <span className="stat-value">{waiting.length} waiting</span>
                <span className="stat-key">
                  {waiting.length
                    ? waiting
                        .map(
                          (j) => `${j.name} · ${waitsFor(j.unmet_conditions) || 'its conditions'}`,
                        )
                        .join(', ')
                    : 'No run is waiting for its conditions'}
                </span>
              </div>
              <div className="stat">
                <span className="stat-value">
                  {week
                    ? `${week.length} ${week.length === 1 ? 'run' : 'runs'} · ${cost(week.reduce((s, r) => s + (r.cost_usd ?? 0), 0))}`
                    : '…'}
                </span>
                <span className="stat-key">
                  {['Last 7 days', ...tally((week ?? []).map((r) => r.status))].join(' · ')}
                </span>
              </div>
            </div>

            <div className="grid-overview">
              <section className="card" aria-label="Up next">
                <div className="lbl">Up next</div>
                {upNext.length === 0 ? (
                  <p className="muted">Nothing scheduled.</p>
                ) : (
                  <div className="up-next">
                    {upNext.map((job) => {
                      const unmet = unmetOf(job)
                      return (
                        <button
                          type="button"
                          key={job.id}
                          className="up"
                          onClick={() => onSelect(job.id)}
                        >
                          <span className="up-when">{when(job.next_run)}</span>
                          <span className="up-name">{job.name}</span>
                          {unmet.length ? (
                            <Chip tone="warn">Waits for {waitsFor(unmet)}</Chip>
                          ) : (
                            <Chip tone="ok">Ready</Chip>
                          )}
                        </button>
                      )
                    })}
                  </div>
                )}
                <p className="muted small-text">
                  Times a Mac sleeps through run at wake; a login catches up what was missed in the
                  last day.
                </p>
              </section>

              <section aria-label="Needs attention" className="stack attention">
                <div className="lbl">Needs attention</div>
                {attention.length === 0 && <p className="muted">Nothing: every job is fine.</p>}
                {attention.map((item) =>
                  item.kind === 'condition' ? (
                    <div key={item.condition} className="att">
                      <span className="dot dot-warn" />
                      <div className="att-text">
                        <div className="att-name">{item.title}</div>
                        <div className="att-sub">{item.text}</div>
                      </div>
                      {item.canSignIn && (
                        <button
                          type="button"
                          className="primary"
                          disabled={item.condition === 'gcloud' && checks.signingIn}
                          onClick={() => actions.onSignIn(item.condition)}
                        >
                          {item.condition === 'gcloud' && checks.signingIn
                            ? 'Signing in…'
                            : 'Sign in'}
                        </button>
                      )}
                    </div>
                  ) : (
                    <div key={item.job.id} className="att">
                      <span className={`dot dot-${item.kind === 'run' ? 'bad' : 'warn'}`} />
                      <div className="att-text">
                        <div className="att-name">{item.title}</div>
                        <div className="att-sub">{item.text}</div>
                      </div>
                      {item.kind === 'run' && item.job.last_run ? (
                        <button
                          type="button"
                          onClick={() =>
                            item.job.last_run && onOpenRun(item.job.id, item.job.last_run.id)
                          }
                        >
                          Open run
                        </button>
                      ) : (
                        <button type="button" onClick={() => onSelect(item.job.id)}>
                          Open job
                        </button>
                      )}
                    </div>
                  ),
                )}
              </section>
            </div>
          </>
        )}
      </div>
    </div>
  )
}

const timeOf = (iso: string) => {
  const at = new Date(iso)
  return clock(at.getHours(), at.getMinutes())
}

type Attention =
  | { kind: 'condition'; condition: Condition; title: string; text: string; canSignIn: boolean }
  | { kind: 'run' | 'launchd'; job: JobView; title: string; text: string }

export function needsAttention(jobs: JobView[], checks: Checks): Attention[] {
  const items: Attention[] = []
  for (const condition of shownConditions(checks, jobs)) {
    const state = conditionState(condition, checks)
    const users = usedBy(condition, jobs).filter((j) => j.enabled)
    if (state.met !== false || !users.length) continue
    const held = users.filter((j) => j.waiting)
    items.push({
      kind: 'condition',
      condition,
      title: `${CONDITION_NAME[condition]} is ${state.word}`,
      text: held.length
        ? held
            .map((j) => `${j.name} has waited since ${midSentence(when(j.waiting?.since))}.`)
            .join(' ')
        : `${users.map((j) => j.name).join(', ')} will wait for it.`,
      canSignIn: !(condition === 'gcloud' && !checks.gcloud?.installed),
    })
  }
  for (const job of jobs) {
    if (job.enabled && !job.loaded)
      items.push({
        kind: 'launchd',
        job,
        title: `${job.name} is not loaded in launchd`,
        text: 'It will not start on its schedule. Saving it again reloads it.',
      })
    const last = job.last_run
    if (last && failed(last.status))
      items.push({
        kind: 'run',
        job,
        title: `${job.name} ${last.status === 'failed' ? 'failed' : STATUS_LABEL[last.status].toLowerCase()}`,
        text: [
          when(last.started_at),
          last.exit_code !== null
            ? `exit code ${last.exit_code} after ${duration(last.duration_s)}`
            : duration(last.duration_s),
        ].join(' · '),
      })
  }
  return items
}

// Every job's runs of the last 7 days, fetched again as a run starts or ends.
function useWeek(jobs: JobView[]): RunRecord[] | null {
  const [runs, setRuns] = useState<RunRecord[] | null>(null)
  const ids = jobs.map((j) => j.id)
  const key = jobs.map((j) => `${j.id}/${j.last_run?.id}/${j.last_run?.status}`).join(' ')
  // biome-ignore lint/correctness/useExhaustiveDependencies: `key` says when the runs changed
  useEffect(() => {
    let latest = true
    const since = Date.now() - WEEK
    Promise.all(ids.map((id) => api.listRuns(id).catch(() => [] as RunRecord[]))).then((all) => {
      if (latest) setRuns(all.flat().filter((r) => Date.parse(r.started_at) >= since))
    })
    return () => {
      latest = false
    }
  }, [key])
  return runs
}

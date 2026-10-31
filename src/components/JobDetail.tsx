import { useEffect, useState } from 'react'
import { api } from '../api'
import { type Checks, conditionState } from '../conditions'
import type { JobView } from '../domain/JobView'
import type { RunRecord } from '../domain/RunRecord'
import type { Trigger } from '../domain/Trigger'
import {
  clock,
  cost,
  duration,
  midSentence,
  STATUS_LABEL,
  scheduleText,
  TRIGGER_LABEL,
  when,
} from '../format'
import { CONDITION_NAME, isRunning, required, unmetOf } from '../jobs'
import { Chip, runTone } from './Chip'
import { Elapsed } from './Elapsed'
import { Icon } from './Icon'

const STARTED_BY: Record<Trigger, string> = {
  schedule: 'from the schedule',
  manual: 'by hand',
  'catch-up': 'to catch up at login',
}

const RECENT = 5

export function JobDetail({
  job,
  checks,
  onRun,
  onToggle,
  onEdit,
  onHistory,
  onWatch,
}: {
  job: JobView
  checks: Checks
  onRun: () => void
  onToggle: () => void
  onEdit: () => void
  // With a run, the history opens on it.
  onHistory: (run?: string) => void
  onWatch: () => void
}) {
  const last = job.last_run
  const running = isRunning(job)
  const waiting = job.waiting
  const unmet = unmetOf(job)
  const problems = unmet.map((c) => c.problem).join(' ')
  const runs = useRecentRuns(job)

  return (
    <div className="view scroll-view">
      <header className="view-head">
        <div className="view-title">
          <h1>{job.name}</h1>
          <div className="chips">
            {!job.enabled ? (
              <Chip tone="muted">Paused</Chip>
            ) : job.loaded ? (
              <Chip tone="ok">Scheduled</Chip>
            ) : (
              <Chip tone="warn">Not loaded in launchd</Chip>
            )}
            {waiting ? (
              <Chip tone="warn">Waiting to start</Chip>
            ) : (
              unmet.length > 0 && <Chip tone="warn">Conditions not met</Chip>
            )}
            {job.interactive && <Chip tone="muted">Interactive</Chip>}
            {job.chrome && <Chip tone="muted">Chrome</Chip>}
          </div>
        </div>
        <div className="actions">
          <button type="button" onClick={onRun} disabled={running || waiting !== null}>
            <Icon name="play" size={13} />
            {running ? 'Running…' : waiting ? 'Waiting…' : 'Run now'}
          </button>
          {running && job.interactive && (
            <button type="button" onClick={onWatch}>
              Watch
            </button>
          )}
          <button type="button" onClick={onToggle}>
            {job.enabled ? 'Pause' : 'Resume'}
          </button>
          <button type="button" onClick={onEdit}>
            Edit
          </button>
          <button type="button" onClick={() => onHistory()}>
            History
          </button>
        </div>
      </header>

      <div className="view-body">
        {last && running ? (
          <section aria-label="Current run" className="now now-running">
            <div className="now-text">
              <div className="now-state">
                <span className="dot dot-running" />
                Running
              </div>
              <div>
                Started {midSentence(when(last.started_at))} {STARTED_BY[last.trigger]} · times out
                at {timesOutAt(last.started_at, job)}
              </div>
              {job.interactive && (
                <div className="muted small-text">
                  Interactive session in tmux · Watch opens it in Terminal.
                </div>
              )}
            </div>
            <div className="now-timer">
              <Elapsed since={last.started_at} />
            </div>
          </section>
        ) : waiting ? (
          <section aria-label="Waiting run" className="now now-waiting">
            <div className="now-text">
              <div className="now-state">
                <span className="dot dot-warn" />
                Waiting to start
              </div>
              <div>
                Waiting since {midSentence(when(waiting.since))}
                {problems ? `: ${problems}` : ' for its conditions.'} It starts as soon as they are
                met.
              </div>
            </div>
          </section>
        ) : (
          unmet.length > 0 && (
            <p className="note note-warn" role="status">
              {problems} Its next run waits until then.
            </p>
          )
        )}

        <div className="grid-2">
          <section className="card" aria-label="Prompt">
            <div className="lbl">Prompt</div>
            <div className="mono prompt-text">{job.prompt}</div>
          </section>
          <section className="card" aria-label="Setup">
            <div className="lbl">Setup</div>
            <dl className="kv">
              <dt>Schedule</dt>
              <dd>
                {scheduleText(job)}
                {job.enabled && job.next_run ? ` · next ${midSentence(when(job.next_run))}` : ''}
              </dd>
              <dt>Timeout</dt>
              <dd>{job.timeout_minutes} minutes</dd>
              <dt>Allowed tools</dt>
              <dd className="mono">
                {job.allowed_tools.filter(Boolean).join(' · ') || (
                  <span className="muted">None</span>
                )}
              </dd>
              <dt>Folder</dt>
              <dd className="mono">{job.cwd}</dd>
              {job.model && (
                <>
                  <dt>Model</dt>
                  <dd className="mono">{job.model}</dd>
                </>
              )}
              <dt>Starts when</dt>
              <dd className="stack">
                {required(job).map((condition) => {
                  // The job's own check first: it is what its runs wait on.
                  const met = job.unmet_conditions.some((c) => c.condition === condition)
                    ? false
                    : conditionState(condition, checks).met
                  return (
                    <span key={condition} className="inline">
                      <span className={`dot dot-${met === null ? 'muted' : met ? 'ok' : 'warn'}`} />
                      {CONDITION_NAME[condition]} signed in
                    </span>
                  )
                })}
              </dd>
            </dl>
          </section>
        </div>

        <section className="card" aria-label="Recent runs">
          <div className="card-head">
            <div className="lbl">Recent runs</div>
            {runs && runs.length > 0 && (
              <button type="button" className="link" onClick={() => onHistory()}>
                {runs.length === 1 ? 'All 1 run' : `All ${runs.length} runs`}
              </button>
            )}
          </div>
          {runs === null ? (
            <p className="muted">Loading runs…</p>
          ) : runs.length === 0 ? (
            <p className="muted">Never run</p>
          ) : (
            <RunsTable runs={runs.slice(0, RECENT)} onOpen={(run) => onHistory(run.id)} />
          )}
        </section>
      </div>
    </div>
  )
}

function timesOutAt(startedAt: string, job: JobView): string {
  const end = new Date(new Date(startedAt).getTime() + job.timeout_minutes * 60000)
  return clock(end.getHours(), end.getMinutes())
}

// The job's runs, newest first, fetched again whenever its last run starts or ends.
function useRecentRuns(job: JobView): RunRecord[] | null {
  const [runs, setRuns] = useState<RunRecord[] | null>(null)
  const key = `${job.id}/${job.last_run?.id}/${job.last_run?.status}`
  // biome-ignore lint/correctness/useExhaustiveDependencies: `key` says when the runs changed
  useEffect(() => {
    let latest = true
    api.listRuns(job.id).then(
      (found) => latest && setRuns(found),
      () => latest && setRuns([]),
    )
    return () => {
      latest = false
    }
  }, [key])
  return runs
}

function RunsTable({ runs, onOpen }: { runs: RunRecord[]; onOpen: (run: RunRecord) => void }) {
  return (
    <table>
      <thead>
        <tr>
          <th>Started</th>
          <th>Trigger</th>
          <th>Status</th>
          <th className="num">Duration</th>
          <th className="num">Turns</th>
          <th className="num">Cost</th>
        </tr>
      </thead>
      <tbody>
        {runs.map((r) => (
          <tr
            key={r.id}
            onClick={() => onOpen(r)}
            onKeyDown={(e) => {
              if (e.key === 'Enter') onOpen(r)
            }}
            tabIndex={0}
          >
            <td>{when(r.started_at)}</td>
            <td className="muted">{TRIGGER_LABEL[r.trigger]}</td>
            <td>
              <Chip tone={runTone(r.status)}>{STATUS_LABEL[r.status]}</Chip>
            </td>
            <td className="num">
              {r.status === 'running' ? <Elapsed since={r.started_at} /> : duration(r.duration_s)}
            </td>
            <td className="num">{r.num_turns ?? '—'}</td>
            <td className="num">{cost(r.cost_usd)}</td>
          </tr>
        ))}
      </tbody>
    </table>
  )
}

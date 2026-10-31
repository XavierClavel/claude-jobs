import { useCallback, useEffect, useState } from 'react'
import { api, errorsOf } from '../api'
import type { JobView } from '../domain/JobView'
import type { RunDetail } from '../domain/RunDetail'
import type { RunRecord } from '../domain/RunRecord'
import type { Trigger } from '../domain/Trigger'
import {
  cost,
  duration,
  STATUS_LABEL,
  shortDate,
  stopwatch,
  TRIGGER_LABEL,
  tally,
  when,
} from '../format'
import { failed } from '../jobs'
import { Chip, runTone } from './Chip'
import { Elapsed } from './Elapsed'
import { Icon } from './Icon'

const FROM: Record<Trigger, string> = {
  schedule: 'from the schedule',
  manual: 'started by hand',
  'catch-up': 'caught up at login',
}

// The job's runs, newest first, beside the one picked: the run asked for, or else the newest.
export function History({
  job,
  run,
  onBack,
}: {
  job: JobView
  run: string | null
  onBack: () => void
}) {
  const [runs, setRuns] = useState<RunRecord[] | null>(null)
  const [picked, setPicked] = useState<string | null>(run)
  const [detail, setDetail] = useState<RunDetail | null>(null)
  const [errors, setErrors] = useState<string[]>([])

  const load = useCallback(async () => {
    try {
      setRuns(await api.listRuns(job.id))
    } catch (e) {
      setErrors(errorsOf(e))
    }
  }, [job.id])

  useEffect(() => {
    load()
  }, [load])

  const running = runs?.some((r) => r.status === 'running') ?? false
  useEffect(() => {
    if (!running) return
    const timer = setInterval(load, 4000)
    return () => clearInterval(timer)
  }, [running, load])

  const shown = picked ?? runs?.[0]?.id ?? null
  // The record in the list, which changes as a run ends: its detail is fetched again then.
  const record = runs?.find((r) => r.id === shown)
  useEffect(() => {
    if (!shown || !record) return
    let latest = true
    api.getRun(job.id, shown).then(
      (found) => latest && setDetail(found),
      (e) => latest && setErrors(errorsOf(e)),
    )
    return () => {
      latest = false
    }
  }, [job.id, shown, record])

  return (
    <div className="view">
      <header className="view-head bordered">
        <div className="view-title row-title">
          <button
            type="button"
            className="square"
            aria-label={`Back to ${job.name}`}
            title={`Back to ${job.name}`}
            onClick={onBack}
          >
            <Icon name="back" />
          </button>
          <div className="view-title">
            <h1 id="history-title">{job.name} · history</h1>
            {runs && runs.length > 0 && <span className="muted">{summary(runs)}</span>}
          </div>
        </div>
      </header>
      {errors.length > 0 && (
        <p className="errors pane-errors" role="alert">
          {errors.join(' ')}
        </p>
      )}
      {runs && runs.length === 0 && <p className="muted view-empty">No runs yet.</p>}
      {runs && runs.length > 0 && (
        <div className="history">
          <nav className="history-runs" aria-label="Runs">
            {runs.map((r) => (
              <button
                type="button"
                key={r.id}
                className={`run-row${r.id === shown ? ' selected' : ''}`}
                aria-current={r.id === shown ? 'true' : undefined}
                onClick={() => setPicked(r.id)}
              >
                <span className="run-row-when">
                  <span className={`dot dot-${runTone(r.status)}`} />
                  {when(r.started_at)}
                </span>
                <span className="run-row-side">
                  {r.status === 'running' ? (
                    <Elapsed since={r.started_at} />
                  ) : (
                    duration(r.duration_s)
                  )}
                </span>
                <span className="run-row-meta">
                  {TRIGGER_LABEL[r.trigger]} ·{' '}
                  {r.status === 'ok' ? 'OK' : STATUS_LABEL[r.status].toLowerCase()}
                  {r.num_turns !== null &&
                    ` · ${r.num_turns} ${r.num_turns === 1 ? 'turn' : 'turns'}`}
                </span>
                <span className="run-row-side">{cost(r.cost_usd)}</span>
              </button>
            ))}
          </nav>
          {detail && detail.id === shown ? (
            <Run detail={detail} timeout={job.timeout_minutes} />
          ) : (
            <div />
          )}
        </div>
      )}
    </div>
  )
}

// "42 runs since 12 Sep · 39 OK · 2 failed · 1 timed out · $31.20 in all"
export function summary(runs: RunRecord[]): string {
  const oldest = runs[runs.length - 1]
  const since = oldest ? shortDate(oldest.started_at) : ''
  const parts = [
    `${runs.length} ${runs.length === 1 ? 'run' : 'runs'} since ${since}`,
    ...tally(runs.map((r) => r.status)),
  ]
  const spent = runs.reduce((sum, r) => sum + (r.cost_usd ?? 0), 0)
  if (spent) parts.push(`${cost(spent)} in all`)
  return parts.join(' · ')
}

function Run({ detail, timeout }: { detail: RunDetail; timeout: number }) {
  const [copied, setCopied] = useState(false)
  const resume = detail.session_id ? `claude --resume ${detail.session_id}` : null
  const errors = [detail.error, detail.err].filter(Boolean).join('\n\n').trim()
  const running = detail.status === 'running'
  return (
    <article className="run" aria-label={`Run of ${when(detail.started_at)}`}>
      <div className="run-head">
        <div className="inline">
          <h2>{when(detail.started_at)}</h2>
          <Chip tone={runTone(detail.status)}>{STATUS_LABEL[detail.status]}</Chip>
          {detail.interactive && <Chip tone="muted">Interactive</Chip>}
        </div>
        <span className="muted small-text">
          {detail.exit_code !== null ? `exit code ${detail.exit_code} · ` : ''}
          {FROM[detail.trigger]}
        </span>
      </div>
      <div className="stats">
        <div className="stat">
          <span className="stat-value">
            {running ? (
              <Elapsed since={detail.started_at} />
            ) : detail.duration_s !== null ? (
              stopwatch(detail.duration_s)
            ) : (
              '—'
            )}
          </span>
          <span className="stat-key">
            {detail.status === 'timeout' ? `Duration · hit the ${timeout} min timeout` : 'Duration'}
          </span>
        </div>
        <div className="stat">
          <span className="stat-value">{detail.num_turns ?? '—'}</span>
          <span className="stat-key">Turns</span>
        </div>
        <div className="stat">
          <span className="stat-value">{cost(detail.cost_usd)}</span>
          <span className="stat-key">Cost</span>
        </div>
      </div>
      {detail.interactive && (
        <p className="muted small-text">
          Interactive session: the result and cost come from its transcript.
        </p>
      )}
      {resume && (
        <div className="resume">
          <span className="muted">Full conversation</span>
          <code className="mono">{resume}</code>
          <button
            type="button"
            className="small"
            onClick={() =>
              navigator.clipboard.writeText(resume).then(
                () => setCopied(true),
                () => setCopied(false),
              )
            }
          >
            {copied ? 'Copied' : 'Copy'}
          </button>
        </div>
      )}
      <section className="stack">
        <div className="lbl">Result</div>
        <pre>{detail.result || (running ? 'Still running…' : '(empty)')}</pre>
      </section>
      {errors && (
        <section className="stack">
          <div className={`lbl${failed(detail.status) ? ' bad' : ''}`}>Errors</div>
          <pre className={failed(detail.status) ? 'pre-bad' : undefined}>{errors}</pre>
        </section>
      )}
    </article>
  )
}

import type { JobView } from '../domain/JobView'
import type { Trigger } from '../domain/Trigger'
import { ago, cost, dateTime, duration, STATUS_LABEL, scheduleText } from '../format'
import { Chip, runTone } from './Chip'
import { Elapsed } from './Elapsed'

const TRIGGER_NOTE: Record<Trigger, string> = {
  schedule: '',
  manual: ' · started by hand',
  'catch-up': ' · caught up at login',
}

export function JobCard({
  job,
  onRun,
  onToggle,
  onEdit,
  onHistory,
  onWatch,
}: {
  job: JobView
  onRun: () => void
  onToggle: () => void
  onEdit: () => void
  onHistory: () => void
  onWatch: () => void
}) {
  const last = job.last_run
  const running = last?.status === 'running'
  const waiting = job.waiting
  // A paused job does not start, so what it would wait for only matters once a run waits.
  const unmet = waiting || job.enabled ? job.unmet_conditions : []
  const problems = unmet.map((c) => c.problem).join(' ')
  const prompt = job.prompt.length > 140 ? `${job.prompt.slice(0, 140)}…` : job.prompt
  return (
    <section className="job" aria-label={job.name}>
      <div className="job-main">
        <div className="job-head">
          <h2 className="job-name">{job.name}</h2>
          {!job.enabled ? (
            <Chip tone="muted">Paused</Chip>
          ) : job.loaded ? (
            <Chip tone="ok">Scheduled</Chip>
          ) : (
            <Chip tone="warn">Not loaded in launchd</Chip>
          )}
          {last && <Chip tone={runTone(last.status)}>Last run: {STATUS_LABEL[last.status]}</Chip>}
          {waiting ? (
            <Chip tone="warn">Waiting to start</Chip>
          ) : (
            unmet.length > 0 && <Chip tone="warn">Conditions not met</Chip>
          )}
          {job.interactive && <Chip tone="muted">Interactive</Chip>}
          {job.chrome && <Chip tone="muted">Chrome</Chip>}
        </div>
        <div className="lines">
          <div>
            {scheduleText(job)}
            {job.enabled && job.next_run ? ` · next ${dateTime(job.next_run)}` : ''}
          </div>
          <div className="muted mono">{prompt}</div>
          {waiting ? (
            <div className="problem">
              Waiting since {dateTime(waiting.since)}
              {problems ? `: ${problems}` : ' for its conditions.'} It starts as soon as they are
              met.
            </div>
          ) : (
            unmet.length > 0 && (
              <div className="problem">{problems} Its next run waits until then.</div>
            )
          )}
          <div className="muted">
            {!last
              ? 'Never run'
              : running
                ? `Running since ${dateTime(last.started_at)}${TRIGGER_NOTE[last.trigger]}`
                : `Last run ${ago(last.started_at)} · ${duration(last.duration_s)} · ${cost(last.cost_usd)}${
                    TRIGGER_NOTE[last.trigger]
                  }`}
          </div>
        </div>
        <div className="actions">
          <button type="button" onClick={onRun} disabled={running || waiting !== null}>
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
          <button type="button" onClick={onHistory}>
            History
          </button>
        </div>
      </div>
      {running && (
        <div className="job-timer">
          <Elapsed since={last.started_at} />
        </div>
      )}
    </section>
  )
}

import type { JobView } from '../domain/JobView'
import { ago, cost, dateTime, duration, STATUS_LABEL, scheduleText } from '../format'
import { Chip, runTone } from './Chip'

export function JobCard({
  job,
  onRun,
  onToggle,
  onEdit,
  onHistory,
}: {
  job: JobView
  onRun: () => void
  onToggle: () => void
  onEdit: () => void
  onHistory: () => void
}) {
  const last = job.last_run
  const running = last?.status === 'running'
  const prompt = job.prompt.length > 140 ? `${job.prompt.slice(0, 140)}…` : job.prompt
  return (
    <section className="job" aria-label={job.name}>
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
      </div>
      <div className="lines">
        <div>
          {scheduleText(job)}
          {job.enabled && job.next_run ? ` · next ${dateTime(job.next_run)}` : ''}
        </div>
        <div className="muted mono">{prompt}</div>
        <div className="muted">
          {!last
            ? 'Never run'
            : running
              ? `Running since ${dateTime(last.started_at)}`
              : `Last run ${ago(last.started_at)} · ${duration(last.duration_s)} · ${cost(last.cost_usd)}${
                  last.trigger === 'manual' ? ' · started by hand' : ''
                }`}
        </div>
      </div>
      <div className="actions">
        <button type="button" onClick={onRun} disabled={running}>
          {running ? 'Running…' : 'Run now'}
        </button>
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
    </section>
  )
}

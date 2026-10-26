import { useCallback, useEffect, useState } from 'react'
import { api, errorsOf } from '../api'
import type { JobView } from '../domain/JobView'
import type { RunDetail } from '../domain/RunDetail'
import type { RunRecord } from '../domain/RunRecord'
import { cost, dateTime, duration, STATUS_LABEL } from '../format'
import { Chip, runTone } from './Chip'

export function History({ job, onClose }: { job: JobView; onClose: () => void }) {
  const [runs, setRuns] = useState<RunRecord[] | null>(null)
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

  async function open(run: RunRecord) {
    try {
      setDetail(await api.getRun(job.id, run.id))
    } catch (e) {
      setErrors(errorsOf(e))
    }
  }

  return (
    <div className="dlg">
      <div className="foot">
        <h2 id="history-title">{job.name}: history</h2>
        <button type="button" onClick={onClose}>
          Close
        </button>
      </div>
      {errors.length > 0 && (
        <p className="errors" role="alert">
          {errors.join(' ')}
        </p>
      )}
      {runs && runs.length === 0 && <p className="muted">No runs yet.</p>}
      {runs && runs.length > 0 && (
        <div className="scroll">
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
                  className={detail?.id === r.id ? 'selected' : undefined}
                  onClick={() => open(r)}
                  onKeyDown={(e) => {
                    if (e.key === 'Enter') open(r)
                  }}
                  tabIndex={0}
                >
                  <td>{dateTime(r.started_at)}</td>
                  <td className="muted">{r.trigger === 'manual' ? 'By hand' : 'Schedule'}</td>
                  <td>
                    <Chip tone={runTone(r.status)}>{STATUS_LABEL[r.status]}</Chip>
                  </td>
                  <td className="num">{duration(r.duration_s)}</td>
                  <td className="num">{r.num_turns ?? '—'}</td>
                  <td className="num">{cost(r.cost_usd)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
      {detail && <Run detail={detail} />}
    </div>
  )
}

function Run({ detail }: { detail: RunDetail }) {
  const [copied, setCopied] = useState(false)
  const resume = detail.session_id ? `claude --resume ${detail.session_id}` : null
  const errors = [detail.error, detail.err].filter(Boolean).join('\n\n').trim()
  return (
    <div className="dlg nested">
      <h3>Run of {dateTime(detail.started_at)}</h3>
      <div className="lines muted">
        <div>
          Status: {STATUS_LABEL[detail.status]}
          {detail.exit_code !== null ? ` (exit code ${detail.exit_code})` : ''}
        </div>
        {detail.interactive && (
          <div>Interactive session: the result and cost come from its transcript.</div>
        )}
        <div>
          Duration: {duration(detail.duration_s)} · turns: {detail.num_turns ?? '—'} · cost:{' '}
          {cost(detail.cost_usd)}
        </div>
        {resume && (
          <div>
            Full conversation: <span className="mono">{resume}</span>{' '}
            <button
              type="button"
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
      </div>
      <div>
        <div className="muted">Result</div>
        <pre>{detail.result || (detail.status === 'running' ? 'Still running…' : '(empty)')}</pre>
      </div>
      {errors && (
        <div>
          <div className="muted">Errors</div>
          <pre>{errors}</pre>
        </div>
      )}
    </div>
  )
}

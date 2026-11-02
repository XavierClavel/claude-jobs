import {
  type CheckActions,
  type Checks,
  canSignIn,
  conditionState,
  shortName,
  shownConditions,
  waitsFor,
} from '../conditions'
import type { JobView } from '../domain/JobView'
import { day, midSentence, STATUS_LABEL, scheduleShort } from '../format'
import { failed, isRunning, jobTone, unmetOf } from '../jobs'
import { Elapsed } from './Elapsed'
import { Icon } from './Icon'
import { UpdateCheck } from './UpdateCheck'

// What a job is doing, in the line under its name.
export function JobLine({ job, checks }: { job: JobView; checks: Checks }) {
  const last = job.last_run
  if (last && isRunning(job))
    return (
      <>
        Running · <Elapsed since={last.started_at} /> so far
      </>
    )
  const unmet = unmetOf(job)
  if (job.waiting) return <>Waiting for {waitsFor(unmet, checks) || 'its conditions'}</>
  if (!job.enabled) return <>Paused</>
  if (!job.loaded) return <>Not loaded in launchd</>
  const schedule = scheduleShort(job)
  if (unmet.length) return <>{`${schedule} · waits for ${waitsFor(unmet, checks)}`}</>
  if (last && failed(last.status))
    return <>{`${schedule} · last run ${STATUS_LABEL[last.status].toLowerCase()}`}</>
  if (!job.next_run) return <>{schedule}</>
  return <>{`${schedule} · next ${midSentence(day(job.next_run))}`}</>
}

export function Sidebar({
  version,
  jobs,
  selected,
  conditionsOpen,
  editing,
  checks,
  actions,
  onCheckUpdate,
  onNew,
  onSelect,
  onConditions,
  onOverview,
}: {
  version: string | null
  jobs: JobView[] | null
  selected: string | null
  conditionsOpen: boolean
  // The editor has the pane: the rest waits for Save or Cancel.
  editing: boolean
  checks: Checks
  actions: CheckActions
  onCheckUpdate: () => Promise<boolean>
  onNew: () => void
  onSelect: (id: string) => void
  onConditions: () => void
  onOverview: () => void
}) {
  return (
    <aside className="side">
      <div className="side-head">
        <div className="side-title">
          <button type="button" className="app-name" title="Overview" onClick={onOverview}>
            Claude Jobs
          </button>
          {version && <span className="version mono">{version}</span>}
        </div>
        <UpdateCheck onCheck={onCheckUpdate} />
      </div>
      <div className="side-new">
        <button type="button" className="primary wide" disabled={editing} onClick={onNew}>
          <Icon name="plus" />
          New job
        </button>
      </div>
      <div className="lbl side-count">Jobs{jobs ? ` · ${jobs.length}` : ''}</div>
      <nav aria-label="Jobs" className="side-jobs" inert={editing}>
        {jobs?.map((job) => (
          <button
            type="button"
            key={job.id}
            className={`side-row${job.id === selected ? ' selected' : ''}${job.enabled ? '' : ' paused'}`}
            aria-current={job.id === selected ? 'page' : undefined}
            onClick={() => onSelect(job.id)}
          >
            <span className={`dot dot-${jobTone(job)}`} />
            <span className="side-row-text">
              <span className="side-row-name">{job.name}</span>
              <span className="side-row-sub">
                <JobLine job={job} checks={checks} />
              </span>
            </span>
          </button>
        ))}
      </nav>
      <section
        aria-label="Conditions"
        className={`side-conditions${conditionsOpen ? ' selected' : ''}`}
        inert={editing}
      >
        <div className="side-conditions-head">
          <span className="lbl">Conditions</span>
          {conditionsOpen ? (
            <span className="link">Managing</span>
          ) : (
            <button type="button" className="link" onClick={onConditions}>
              Manage
            </button>
          )}
        </div>
        {shownConditions(checks, jobs ?? []).map((condition) => {
          const state = conditionState(condition, checks)
          const signIn = state.met === false && canSignIn(condition, checks)
          return (
            <div key={condition} className="side-condition">
              <span
                className={`dot dot-${state.met === null ? 'muted' : state.met ? 'ok' : 'warn'}`}
              />
              <span className="side-condition-name">{shortName(condition, checks)}</span>
              {signIn && !conditionsOpen ? (
                <button
                  type="button"
                  className="small"
                  disabled={condition === 'gcloud' && checks.signingIn}
                  onClick={() => actions.onSignIn(condition)}
                >
                  {condition === 'gcloud' && checks.signingIn ? 'Signing in…' : 'Sign in'}
                </button>
              ) : (
                <span className="muted small-text">{state.word}</span>
              )}
            </div>
          )
        })}
      </section>
    </aside>
  )
}

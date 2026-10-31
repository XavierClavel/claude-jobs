import { type ReactNode, useState } from 'react'
import {
  type CheckActions,
  type Checks,
  conditionState,
  shownConditions,
  usedBy,
} from '../conditions'
import type { Condition } from '../domain/Condition'
import type { JobView } from '../domain/JobView'
import { midSentence, when } from '../format'
import { CONDITION_NAME } from '../jobs'
import { Chip } from './Chip'

const ABOUT: Record<Condition, { tag: string; what: ReactNode; fix: ReactNode }> = {
  claude: {
    tag: 'Built in · every job',
    what: (
      <>
        Checks <span className="mono">claude auth status</span>, and the claude.ai login's refresh
        token in the keychain for an expiry date. A run the API turns away with a 401 goes back to
        waiting until the credentials change. Checked every minute.
      </>
    ),
    fix: (
      <>
        Sign in opens Terminal on <span className="mono">claude auth login</span>. Jobs wait, then
        start as soon as you have signed in.
      </>
    ),
  },
  gcloud: {
    tag: 'Optional · per job',
    what: (
      <>
        Checks that gcloud has an active account, for the jobs that read BigQuery or Cloud Logging
        and tick it in the editor. Checked every 5 minutes.
      </>
    ),
    fix: (
      <>
        Sign in opens the browser on Google's sign-in, as{' '}
        <span className="mono">gcloud auth login</span> does.
      </>
    ),
  },
}

// Every condition a job can wait for: whether it is met, which jobs use it, and how to meet it.
export function Conditions({
  jobs,
  checks,
  actions,
  onSelect,
}: {
  jobs: JobView[]
  checks: Checks
  actions: CheckActions
  onSelect: (id: string) => void
}) {
  const [checking, setChecking] = useState(false)
  async function checkNow() {
    setChecking(true)
    await actions.onCheck()
    setChecking(false)
  }
  return (
    <div className="view scroll-view">
      <header className="view-head">
        <div className="view-title">
          <h1>Conditions</h1>
          <span className="muted lead">
            A job starts only when every condition it uses is met. Until then its run waits, checks
            again every 30 seconds, and starts the moment they all are.
          </span>
        </div>
        <div className="actions">
          <button type="button" disabled={checking} onClick={checkNow}>
            {checking ? 'Checking…' : 'Check all now'}
          </button>
        </div>
      </header>
      <div className="view-body">
        {shownConditions(checks, jobs).map((condition) => {
          const state = conditionState(condition, checks)
          const users = usedBy(condition, jobs)
          const waiting = users.filter((j) => j.waiting)
          const installed = condition !== 'gcloud' || checks.gcloud?.installed
          const signingIn = condition === 'gcloud' && checks.signingIn
          return (
            <section
              key={condition}
              aria-label={`${CONDITION_NAME[condition]} signed in`}
              className={`cond${state.met === false ? ' cond-unmet' : ''}`}
            >
              <div className="cond-main">
                <div className="inline wrap">
                  <h2>{CONDITION_NAME[condition]} signed in</h2>
                  {state.met === null ? (
                    <Chip tone="muted">Checking…</Chip>
                  ) : state.met ? (
                    <Chip tone="ok">Met</Chip>
                  ) : (
                    <Chip tone="warn">Not met</Chip>
                  )}
                  <span className="tag">{ABOUT[condition].tag}</span>
                </div>
                {state.problem ? (
                  <p className="cond-problem">
                    {state.problem}
                    {waiting.map(
                      (j) => ` ${j.name} has waited since ${midSentence(when(j.waiting?.since))}.`,
                    )}
                  </p>
                ) : (
                  <p className="cond-about">{ABOUT[condition].what}</p>
                )}
                <dl className="kv">
                  {condition === 'gcloud' && installed && (
                    <>
                      <dt>Account</dt>
                      <dd>{checks.gcloud?.account ?? '—'}</dd>
                    </>
                  )}
                  <dt>Used by</dt>
                  <dd className="tags">
                    {condition === 'claude' && users.length > 0 && (
                      <span className="tag">
                        {users.length === 1 ? 'the 1 job' : `all ${users.length} jobs`}
                      </span>
                    )}
                    {condition !== 'claude' &&
                      users.map((j) => (
                        <button
                          type="button"
                          key={j.id}
                          className="tag"
                          onClick={() => onSelect(j.id)}
                        >
                          {j.name}
                        </button>
                      ))}
                    {users.length === 0 && <span className="muted">No job</span>}
                  </dd>
                </dl>
              </div>
              <div className="cond-side">
                <span className="lbl">When not met</span>
                <span className="muted small-text">{ABOUT[condition].fix}</span>
                <div className="actions cond-actions">
                  <button type="button" disabled={checking} onClick={checkNow}>
                    Check now
                  </button>
                  <button
                    type="button"
                    className={state.met === false && installed ? 'primary' : undefined}
                    // Signing in again is for Google Cloud, whose account can be switched.
                    disabled={!installed || signingIn || (condition === 'claude' && !!state.met)}
                    onClick={() => actions.onSignIn(condition)}
                  >
                    {signingIn
                      ? 'Waiting for the browser…'
                      : condition === 'gcloud' && state.met
                        ? 'Sign in again'
                        : 'Sign in'}
                  </button>
                </div>
              </div>
            </section>
          )
        })}
      </div>
    </div>
  )
}

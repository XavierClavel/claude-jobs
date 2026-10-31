import { type ReactNode, useState } from 'react'
import { api, errorsOf } from '../api'
import {
  type CheckActions,
  type Checks,
  conditionState,
  fullName,
  shownConditions,
  usedBy,
} from '../conditions'
import type { CommandConditionView } from '../domain/CommandConditionView'
import type { Expect } from '../domain/Expect'
import type { JobView } from '../domain/JobView'
import { midSentence, when } from '../format'
import { Chip } from './Chip'
import { CommandConditionEditor } from './CommandConditionEditor'
import { Icon } from './Icon'

const ABOUT: Record<string, { tag: string; what: ReactNode; fix: ReactNode }> = {
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
    tag: 'Built in · per job',
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

// "exits with 0", "prints “Connected”", "prints something with “ready”"
export function expectText(expect: Expect): string {
  if (expect.kind === 'exit_code') return `exits with ${expect.code}`
  return expect.contains ? `prints something with “${expect.text}”` : `prints “${expect.text}”`
}

// Which condition the form is open on: a new one, or one by its id.
type Editing = { id: string | null } | null

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
  const [editing, setEditing] = useState<Editing>(null)
  const [errors, setErrors] = useState<string[]>([])

  // The command conditions keep a check for a minute: checking now runs them again.
  async function checkNow(commands: CommandConditionView[] = checks.commands) {
    setChecking(true)
    try {
      await Promise.all(commands.map((c) => api.checkCondition(c)))
      setErrors([])
    } catch (e) {
      setErrors(errorsOf(e))
    }
    await actions.onCheck()
    setChecking(false)
  }

  async function remove(id: string) {
    try {
      await api.deleteCondition(id)
      setErrors([])
    } catch (e) {
      setErrors(errorsOf(e))
    }
    await actions.onCheck()
  }

  const shown = shownConditions(checks, jobs)

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
          <button type="button" disabled={checking} onClick={() => checkNow()}>
            {checking ? 'Checking…' : 'Check all now'}
          </button>
          <button
            type="button"
            className="primary"
            disabled={editing !== null}
            onClick={() => setEditing({ id: null })}
          >
            <Icon name="plus" />
            Add condition
          </button>
        </div>
      </header>
      <div className="view-body conditions">
        {errors.length > 0 && (
          <ul className="errors" role="alert">
            {errors.map((m) => (
              <li key={m}>{m}</li>
            ))}
          </ul>
        )}
        {editing?.id === null && (
          <CommandConditionEditor
            condition={null}
            onCancel={() => setEditing(null)}
            onSaved={() => {
              setEditing(null)
              actions.onCheck()
            }}
          />
        )}
        {shown.map((condition) => {
          const command = checks.commands.find((c) => c.id === condition)
          if (command && editing?.id === condition)
            return (
              <CommandConditionEditor
                key={condition}
                condition={command}
                onCancel={() => setEditing(null)}
                onSaved={() => {
                  setEditing(null)
                  actions.onCheck()
                }}
              />
            )
          return command ? (
            <CommandCard
              key={condition}
              command={command}
              jobs={jobs}
              checks={checks}
              checking={checking}
              onSelect={onSelect}
              onCheck={() => checkNow([command])}
              onEdit={() => setEditing({ id: condition })}
              onDelete={() => remove(condition)}
            />
          ) : (
            <BuiltInCard
              key={condition}
              condition={condition}
              jobs={jobs}
              checks={checks}
              actions={actions}
              checking={checking}
              onSelect={onSelect}
              onCheck={() => checkNow([])}
            />
          )
        })}
        {editing === null && (
          <button type="button" className="add-condition" onClick={() => setEditing({ id: null })}>
            <Icon name="plus" size={16} />
            Add a condition
            <span className="normal">
              · a command every job, or only the jobs that tick it, waits for
            </span>
          </button>
        )}
      </div>
    </div>
  )
}

function UsedBy({
  users,
  everyJob,
  onSelect,
}: {
  users: JobView[]
  everyJob: boolean
  onSelect: (id: string) => void
}) {
  return (
    <>
      <dt>Used by</dt>
      <dd className="tags">
        {users.length === 0 ? (
          <span className="muted">No job</span>
        ) : everyJob ? (
          <span className="tag">
            {users.length === 1 ? 'the 1 job' : `all ${users.length} jobs`}
          </span>
        ) : (
          users.map((j) => (
            <button type="button" key={j.id} className="tag" onClick={() => onSelect(j.id)}>
              {j.name}
            </button>
          ))
        )}
      </dd>
    </>
  )
}

function StateChip({ met }: { met: boolean | null }) {
  if (met === null) return <Chip tone="muted">Checking…</Chip>
  return met ? <Chip tone="ok">Met</Chip> : <Chip tone="warn">Not met</Chip>
}

function BuiltInCard({
  condition,
  jobs,
  checks,
  actions,
  checking,
  onSelect,
  onCheck,
}: {
  condition: string
  jobs: JobView[]
  checks: Checks
  actions: CheckActions
  checking: boolean
  onSelect: (id: string) => void
  onCheck: () => void
}) {
  const state = conditionState(condition, checks)
  const users = usedBy(condition, jobs, checks)
  const waiting = users.filter((j) => j.waiting)
  const installed = condition !== 'gcloud' || checks.gcloud?.installed
  const signingIn = condition === 'gcloud' && checks.signingIn
  const about = ABOUT[condition]
  return (
    <section
      aria-label={fullName(condition, checks)}
      className={`cond${state.met === false ? ' cond-unmet' : ''}`}
    >
      <div className="cond-main">
        <div className="inline wrap">
          <h2>{fullName(condition, checks)}</h2>
          <StateChip met={state.met} />
          <span className="tag">{about?.tag}</span>
        </div>
        {state.problem ? (
          <p className="cond-problem">
            {state.problem}
            {waiting.map(
              (j) => ` ${j.name} has waited since ${midSentence(when(j.waiting?.since))}.`,
            )}
          </p>
        ) : (
          <p className="cond-about">{about?.what}</p>
        )}
        <dl className="kv">
          {condition === 'gcloud' && installed && (
            <>
              <dt>Account</dt>
              <dd>{checks.gcloud?.account ?? '—'}</dd>
            </>
          )}
          <UsedBy users={users} everyJob={condition === 'claude'} onSelect={onSelect} />
        </dl>
      </div>
      <div className="cond-side">
        <span className="lbl">When not met</span>
        <span className="muted small-text">{about?.fix}</span>
        <div className="actions cond-actions">
          <button type="button" disabled={checking} onClick={onCheck}>
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
}

function CommandCard({
  command,
  jobs,
  checks,
  checking,
  onSelect,
  onCheck,
  onEdit,
  onDelete,
}: {
  command: CommandConditionView
  jobs: JobView[]
  checks: Checks
  checking: boolean
  onSelect: (id: string) => void
  onCheck: () => void
  onEdit: () => void
  onDelete: () => void
}) {
  const [confirmDelete, setConfirmDelete] = useState(false)
  const state = conditionState(command.id, checks)
  const users = usedBy(command.id, jobs, checks)
  const waiting = users.filter((j) => j.waiting)
  return (
    <section
      aria-label={command.name}
      className={`cond${state.met === false ? ' cond-unmet' : ''}`}
    >
      <div className="cond-main">
        <div className="inline wrap">
          <h2>{command.name}</h2>
          <StateChip met={state.met} />
          <span className="tag">Command · {command.every_job ? 'every job' : 'per job'}</span>
        </div>
        <code className="mono command-line">{command.command}</code>
        {state.problem && (
          <p className="cond-problem">
            {state.problem}
            {waiting.map(
              (j) => ` ${j.name} has waited since ${midSentence(when(j.waiting?.since))}.`,
            )}
          </p>
        )}
        <dl className="kv">
          <dt>Met when it</dt>
          <dd>{expectText(command.expect)}</dd>
          <dt>Last check</dt>
          <dd>{midSentence(when(command.checked_at))} · checked every minute</dd>
          <UsedBy users={users} everyJob={command.every_job} onSelect={onSelect} />
        </dl>
      </div>
      <div className="cond-side">
        <span className="lbl">When not met</span>
        <span className="muted small-text">
          {command.hint || 'Jobs wait, then start as soon as the command passes.'}
        </span>
        <div className="actions cond-actions">
          <button type="button" disabled={checking} onClick={onCheck}>
            Check now
          </button>
          <button type="button" onClick={onEdit}>
            Edit
          </button>
          <button
            type="button"
            className="danger"
            onClick={() => (confirmDelete ? onDelete() : setConfirmDelete(true))}
            onBlur={() => setConfirmDelete(false)}
          >
            {confirmDelete ? 'Click again to delete' : 'Delete'}
          </button>
        </div>
      </div>
    </section>
  )
}

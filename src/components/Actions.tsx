import { Fragment, useState } from 'react'
import { destinationText, maskedUrl, OUTCOMES, usersOf } from '../actions'
import { api, errorsOf } from '../api'
import type { Action } from '../domain/Action'
import type { JobView } from '../domain/JobView'
import { ActionEditor, type Sent, SentResult, sendTest } from './ActionEditor'
import { ActionIcon } from './ActionIcon'
import { Icon } from './Icon'

// Which action the form is open on: a new one, or one by its id.
type Editing = { id: string | null } | null

// What jobs can do when a run ends, and which jobs do it.
export function Actions({
  actions,
  jobs,
  onChanged,
  onSelect,
}: {
  actions: Action[]
  jobs: JobView[]
  onChanged: () => Promise<unknown>
  onSelect: (id: string) => void
}) {
  const [editing, setEditing] = useState<Editing>(null)
  const [errors, setErrors] = useState<string[]>([])

  async function remove(id: string) {
    try {
      await api.deleteAction(id)
      setErrors([])
    } catch (e) {
      setErrors(errorsOf(e))
    }
    await onChanged()
  }

  const saved = () => {
    setEditing(null)
    onChanged()
  }

  return (
    <div className="view scroll-view">
      <header className="view-head">
        <div className="view-title">
          <h1>Actions</h1>
          <span className="muted lead">
            What a job does when a run ends. Each job picks its own in its editor: on success, on
            failure, or both.
          </span>
        </div>
        <div className="actions">
          <button
            type="button"
            className="primary"
            disabled={editing !== null}
            onClick={() => setEditing({ id: null })}
          >
            <Icon name="plus" />
            Add action
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
          <ActionEditor action={null} onCancel={() => setEditing(null)} onSaved={saved} />
        )}
        {actions.map((action) =>
          editing?.id === action.id ? (
            <ActionEditor
              key={action.id}
              action={action}
              onCancel={() => setEditing(null)}
              onSaved={saved}
            />
          ) : (
            <ActionCard
              key={action.id}
              action={action}
              jobs={jobs}
              onSelect={onSelect}
              onEdit={() => setEditing({ id: action.id })}
              onDelete={() => remove(action.id)}
            />
          ),
        )}
        {editing === null && (
          <button type="button" className="add-condition" onClick={() => setEditing({ id: null })}>
            <Icon name="plus" size={16} />
            Add an action
            <span className="normal">
              · a Slack workflow that messages a channel or a user when a run ends
            </span>
          </button>
        )}
      </div>
    </div>
  )
}

function ActionCard({
  action,
  jobs,
  onSelect,
  onEdit,
  onDelete,
}: {
  action: Action
  jobs: JobView[]
  onSelect: (id: string) => void
  onEdit: () => void
  onDelete: () => void
}) {
  const [confirmDelete, setConfirmDelete] = useState(false)
  const [testing, setTesting] = useState(false)
  const [sent, setSent] = useState<Sent | null>(null)

  async function test() {
    setTesting(true)
    setSent(await sendTest(action))
    setTesting(false)
  }

  return (
    <section aria-label={action.name} className="cond">
      <div className="cond-main">
        <div className="inline wrap">
          <h2>{action.name}</h2>
          <span className="tag with-icon">
            <ActionIcon action={action} size={12} />
            Slack workflow
          </span>
        </div>
        <p className="cond-about">{destinationText(action)}</p>
        <dl className="kv">
          <dt>Webhook</dt>
          <dd className="mono">{maskedUrl(action.target.webhook_url)}</dd>
          {OUTCOMES.map(([outcome, label]) => {
            const users = usersOf(action.id, outcome, jobs)
            return (
              <Fragment key={outcome}>
                <dt>{label}</dt>
                <dd className="tags">
                  {users.length === 0 ? (
                    <span className="muted">No job</span>
                  ) : (
                    users.map((j) => (
                      <button
                        type="button"
                        key={j.id}
                        className="tag"
                        onClick={() => onSelect(j.id)}
                      >
                        {j.name}
                      </button>
                    ))
                  )}
                </dd>
              </Fragment>
            )
          })}
        </dl>
      </div>
      <div className="cond-side">
        {sent && <SentResult sent={sent} />}
        <div className="actions cond-actions">
          <button type="button" disabled={testing} onClick={test}>
            {testing ? 'Sending…' : 'Send test'}
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

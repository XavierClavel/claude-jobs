import { Fragment, useState } from 'react'
import { ACTION_LISTS, maskedUrl, recipientKind, usersOf } from '../actions'
import { api, errorsOf } from '../api'
import type { Action } from '../domain/Action'
import type { JobView } from '../domain/JobView'
import { ActionEditor, type Sent, SentResult, sendTest, TestButton } from './ActionEditor'
import { ActionIcon } from './ActionIcon'
import { Icon } from './Icon'

// Which action the form is open on: a new one, or one by its id.
type Editing = { id: string | null } | null

// The Slack workflows jobs send with, who each sends to, and which jobs do.
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
  const [sent, setSent] = useState<Sent | null>(null)

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
        <dl className="kv">
          <dt>Webhook</dt>
          <dd className="mono">{maskedUrl(action.target.webhook_url)}</dd>
          <dt>Recipients</dt>
          <dd className="tags">
            {action.recipients.length === 0 ? (
              <span className="muted">None</span>
            ) : (
              action.recipients.map((r) => (
                <span key={r.id} className="tag" title={recipientKind(r)}>
                  {r.name}
                </span>
              ))
            )}
          </dd>
          {ACTION_LISTS.map(([list, label]) => {
            const users = usersOf((u) => u.action === action.id, list, jobs)
            return (
              <Fragment key={list}>
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
          <TestButton
            recipients={action.recipients}
            onSend={async (to) => setSent(await sendTest(action, to))}
          />
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

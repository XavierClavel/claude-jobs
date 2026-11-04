import { useRef, useState } from 'react'
import { ANYONE, freeId } from '../actions'
import { api, errorsOf } from '../api'
import type { Action } from '../domain/Action'
import type { ActionUse } from '../domain/ActionUse'
import type { Recipient } from '../domain/Recipient'
import { ActionIcon } from './ActionIcon'
import { Icon } from './Icon'
import { useDismiss } from './useDismiss'

export type Picked = { onSuccess: ActionUse[]; onFailure: ActionUse[]; onComplete: ActionUse[] }

const ROWS: { key: keyof Picked; label: string; when: string }[] = [
  { key: 'onSuccess', label: 'On success', when: 'the run worked' },
  { key: 'onFailure', label: 'On failure', when: 'it failed or timed out' },
  { key: 'onComplete', label: 'On complete', when: 'it ended, however it went' },
]

// The value of the recipient menu's last option, which opens the form for a new one.
const NEW = '\u0000new'

type Shared = {
  actions: Action[]
  // An action given a recipient from the picker, saved: the list it picks from gets it.
  onActionSaved: (saved: Action) => void
}

// What the job does when a run ends, picked from the actions that exist, each to one of its recipients.
export function AfterRunCard({
  picked,
  onChange,
  ...shared
}: Shared & {
  picked: Picked
  onChange: (key: keyof Picked, uses: ActionUse[]) => void
}) {
  return (
    <section className="card" aria-label="When it ends">
      <div className="lbl">When it ends</div>
      {ROWS.map((row) => (
        <ActionsRow
          key={row.key}
          label={row.label}
          note={`when ${row.when}`}
          where={row.label.toLowerCase()}
          uses={picked[row.key]}
          onChange={(uses) => onChange(row.key, uses)}
          {...shared}
        />
      ))}
      {shared.actions.length === 0 && <p className="muted small-text">No action yet</p>}
    </section>
  )
}

// Who Claude may send to during a run, and with which action.
export function DuringRunCard({
  uses,
  onChange,
  ...shared
}: Shared & {
  uses: ActionUse[]
  onChange: (uses: ActionUse[]) => void
}) {
  return (
    <section className="card" aria-label="During the run">
      <div className="lbl">During the run</div>
      <ActionsRow
        label="Claude can send"
        note="when the prompt asks"
        where="during the run"
        uses={uses}
        onChange={onChange}
        anyone
        {...shared}
      />
      {shared.actions.length === 0 && <p className="muted small-text">No action yet</p>}
    </section>
  )
}

// One list of a job's actions, each with its recipient, and a menu to add an action. `where` ends the labels of its
// buttons: "on success".
function ActionsRow({
  label,
  note,
  where,
  uses,
  actions,
  onChange,
  onActionSaved,
  anyone = false,
}: Shared & {
  label: string
  note: string
  where: string
  uses: ActionUse[]
  onChange: (uses: ActionUse[]) => void
  // Whether a use can send to anyone, whose Slack ID Claude gives: during the run only.
  anyone?: boolean
}) {
  const [open, setOpen] = useState(false)
  // The use whose recipient is being made, by its place in the list.
  const [adding, setAdding] = useState<number | null>(null)
  const ref = useRef<HTMLFieldSetElement>(null)
  useDismiss(open, ref, setOpen)

  const update = (at: number, recipient: string) =>
    onChange(uses.map((u, i) => (i === at ? { ...u, recipient } : u)))

  function add(action: Action) {
    // The action's one recipient it does not send to yet in this list, when there is only one left.
    const free = action.recipients.filter(
      (r) => !uses.some((u) => u.action === action.id && u.recipient === r.id),
    )
    const only = free.length === 1 ? free[0] : undefined
    onChange([...uses, { action: action.id, recipient: only?.id ?? '' }])
    setOpen(false)
  }

  return (
    <fieldset className="outcome picker-anchor" aria-label={label} ref={ref}>
      <div className="card-head">
        <span className="outcome-title">
          {label} <span className="muted normal">· {note}</span>
        </span>
        <button
          type="button"
          className="small"
          aria-label={`Add action ${where}`}
          aria-expanded={open}
          aria-haspopup="menu"
          disabled={actions.length === 0}
          onClick={() => setOpen((o) => !o)}
        >
          <Icon name="plus" size={12} />
          Add action
        </button>
      </div>
      {uses.length === 0 ? (
        <p className="muted small-text">Nothing</p>
      ) : (
        <div className="stack-6">
          {uses.map((used, at) => {
            const action = actions.find((a) => a.id === used.action)
            const name = action?.name ?? used.action
            const recipients = action?.recipients ?? []
            const known =
              recipients.some((r) => r.id === used.recipient) ||
              (anyone && used.recipient === ANYONE)
            return (
              // biome-ignore lint/suspicious/noArrayIndexKey: the same action can be in the list twice, to two recipients.
              <div key={at} className="stack-6">
                <div className="condition-item">
                  {action && <ActionIcon action={action} size={16} />}
                  <span className="condition-text">
                    <span className="condition-name">{name}</span>
                    {!action && <span className="condition-sub">No longer exists</span>}
                  </span>
                  <span className="muted" aria-hidden="true">
                    →
                  </span>
                  <select
                    className="recipient-select"
                    aria-label={`Who ${name} sends to ${where}`}
                    value={adding === at ? NEW : known ? used.recipient : ''}
                    onChange={(e) => {
                      if (e.target.value === NEW) {
                        setAdding(at)
                      } else {
                        setAdding(null)
                        update(at, e.target.value)
                      }
                    }}
                  >
                    <option value="">No recipient</option>
                    {anyone && <option value={ANYONE}>Anyone</option>}
                    {recipients.map((r) => (
                      <option key={r.id} value={r.id}>
                        {r.name}
                      </option>
                    ))}
                    {action && <option value={NEW}>New recipient…</option>}
                  </select>
                  <button
                    type="button"
                    className="square ghost"
                    aria-label={`Remove ${name} ${where}`}
                    onClick={() => {
                      setAdding(null)
                      onChange(uses.filter((_, i) => i !== at))
                    }}
                  >
                    <Icon name="close" size={13} />
                  </button>
                </div>
                {adding === at && action && (
                  <NewRecipient
                    action={action}
                    onCancel={() => setAdding(null)}
                    onAdded={(saved, added) => {
                      onActionSaved(saved)
                      update(at, added)
                      setAdding(null)
                    }}
                  />
                )}
              </div>
            )
          })}
        </div>
      )}
      {open && (
        <div role="menu" aria-label={`Actions ${where}`} className="picker">
          <div className="lbl picker-title">Actions</div>
          {actions.map((action) => (
            <button
              type="button"
              key={action.id}
              role="menuitem"
              className="pick"
              onClick={() => add(action)}
            >
              <ActionIcon action={action} size={16} />
              <span className="condition-text">
                <span className="condition-name">{action.name}</span>
              </span>
            </button>
          ))}
        </div>
      )}
    </fieldset>
  )
}

// A recipient added to an action from a job's picker: a name and a Slack ID, saved in the action at once, so other
// jobs can pick it too. `onAdded` gets the action saved, and the new recipient's id.
export function NewRecipient({
  action,
  onCancel,
  onAdded,
}: {
  action: Action
  onCancel: () => void
  onAdded: (saved: Action, added: string) => void
}) {
  const [name, setName] = useState('')
  const [slackId, setSlackId] = useState('')
  const [errors, setErrors] = useState<string[]>([])
  const [saving, setSaving] = useState(false)

  async function save() {
    setSaving(true)
    try {
      const recipient: Recipient = { id: freeId(name, action.recipients), name, slack_id: slackId }
      const saved = await api.saveAction(
        { ...action, recipients: [...action.recipients, recipient] },
        action.id,
      )
      onAdded(saved, recipient.id)
    } catch (e) {
      setErrors(errorsOf(e))
    } finally {
      setSaving(false)
    }
  }

  // Not a form: it sits inside the job's own.
  return (
    <fieldset className="new-recipient" aria-label={`New recipient of ${action.name}`}>
      <div className="grid-2 tight">
        <label className="field">
          Name
          <input
            value={name}
            placeholder="Ops channel"
            // biome-ignore lint/a11y/noAutofocus: it opens because the user asked for it.
            autoFocus
            onChange={(e) => setName(e.target.value)}
          />
        </label>
        <label className="field">
          Slack ID
          <input
            className="mono"
            value={slackId}
            placeholder="U012AB3CD or C012AB3CD"
            spellCheck={false}
            onChange={(e) => setSlackId(e.target.value)}
          />
        </label>
      </div>
      {errors.length > 0 && (
        <ul className="errors" role="alert">
          {errors.map((m) => (
            <li key={m}>{m}</li>
          ))}
        </ul>
      )}
      <div className="actions">
        <button type="button" className="small" onClick={onCancel}>
          Cancel
        </button>
        <button
          type="button"
          className="small primary"
          disabled={saving || !name.trim() || !slackId.trim()}
          onClick={save}
        >
          {saving ? 'Adding…' : 'Add recipient'}
        </button>
      </div>
    </fieldset>
  )
}

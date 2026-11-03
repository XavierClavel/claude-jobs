import { useRef, useState } from 'react'
import { destinationText } from '../actions'
import type { Action } from '../domain/Action'
import { ActionIcon } from './ActionIcon'
import { Icon } from './Icon'
import { useDismiss } from './useDismiss'

export type Picked = { onSuccess: string[]; onFailure: string[]; onComplete: string[] }

const ROWS: { key: keyof Picked; label: string; when: string }[] = [
  { key: 'onSuccess', label: 'On success', when: 'the run worked' },
  { key: 'onFailure', label: 'On failure', when: 'it failed or timed out' },
  { key: 'onComplete', label: 'On complete', when: 'it ended, however it went' },
]

// What the job does when a run ends, picked from the actions that exist.
export function AfterRunCard({
  picked,
  actions,
  onChange,
}: {
  picked: Picked
  actions: Action[]
  onChange: (key: keyof Picked, ids: string[]) => void
}) {
  return (
    <section className="card" aria-label="When it ends">
      <div className="lbl">When it ends</div>
      {ROWS.map((row) => (
        <OutcomeRow
          key={row.key}
          label={row.label}
          when={row.when}
          ids={picked[row.key]}
          actions={actions}
          onChange={(ids) => onChange(row.key, ids)}
        />
      ))}
      <p className="hint">
        {actions.length
          ? 'Make and test actions on the Actions page.'
          : 'No action yet: make one, such as a Slack message, on the Actions page.'}
      </p>
    </section>
  )
}

function OutcomeRow({
  label,
  when,
  ids,
  actions,
  onChange,
}: {
  label: string
  when: string
  ids: string[]
  actions: Action[]
  onChange: (ids: string[]) => void
}) {
  const [open, setOpen] = useState(false)
  const ref = useRef<HTMLFieldSetElement>(null)
  useDismiss(open, ref, setOpen)
  const where = label.toLowerCase()

  return (
    <fieldset className="outcome picker-anchor" aria-label={label} ref={ref}>
      <div className="card-head">
        <span className="outcome-title">
          {label} <span className="muted normal">· when {when}</span>
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
      {ids.length === 0 ? (
        <p className="muted small-text">Nothing</p>
      ) : (
        <div className="stack-6">
          {ids.map((id) => {
            const action = actions.find((a) => a.id === id)
            const name = action?.name ?? id
            return (
              <div key={id} className="condition-item">
                {action && <ActionIcon action={action} size={16} />}
                <span className="condition-text">
                  <span className="condition-name">{name}</span>
                  <span className="condition-sub">
                    {action ? `Slack · ${destinationText(action)}` : 'No longer exists'}
                  </span>
                </span>
                <button
                  type="button"
                  className="square ghost"
                  aria-label={`Remove ${name} ${where}`}
                  onClick={() => onChange(ids.filter((i) => i !== id))}
                >
                  <Icon name="close" size={13} />
                </button>
              </div>
            )
          })}
        </div>
      )}
      {open && (
        <div role="menu" aria-label={`Actions ${where}`} className="picker">
          <div className="lbl picker-title">Actions</div>
          {actions.map((action) => {
            const on = ids.includes(action.id)
            return (
              <button
                type="button"
                key={action.id}
                role="menuitemcheckbox"
                aria-checked={on}
                className={`pick${on ? ' selected' : ''}`}
                onClick={() => {
                  onChange(on ? ids.filter((i) => i !== action.id) : [...ids, action.id])
                  setOpen(false)
                }}
              >
                <span className={on ? undefined : 'invisible'}>
                  <Icon name="check" />
                </span>
                <ActionIcon action={action} size={16} />
                <span className="condition-text">
                  <span className="condition-name">{action.name}</span>
                  <span className="condition-sub">
                    {on ? 'Added' : `Slack · ${destinationText(action)}`}
                  </span>
                </span>
              </button>
            )
          })}
        </div>
      )}
    </fieldset>
  )
}

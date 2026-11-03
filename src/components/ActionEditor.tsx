import { type FormEvent, useState } from 'react'
import { destination, RECIPIENT, VARIABLES } from '../actions'
import { api, errorsOf } from '../api'
import type { Action } from '../domain/Action'
import type { SlackTo } from '../domain/SlackTo'
import { slug } from '../format'

// What the form's fields hold, as typed; `toAction` turns it into the action the Rust side checks.
type Draft = {
  id: string
  name: string
  webhookUrl: string
  to: SlackTo
  recipient: string
}

function toDraft(action: Action | null): Draft {
  return {
    id: action?.id ?? '',
    name: action?.name ?? '',
    webhookUrl: action?.target.webhook_url ?? '',
    to: action?.target.to ?? 'channel',
    recipient: action?.target.recipient ?? '',
  }
}

export function toAction(draft: Draft): Action {
  return {
    id: draft.id.trim(),
    name: draft.name,
    target: {
      kind: 'slack',
      webhook_url: draft.webhookUrl,
      to: draft.to,
      recipient: draft.recipient,
    },
  }
}

const TO: [SlackTo, string][] = [
  ['channel', 'Channel'],
  ['user', 'User'],
]

// What a test message did: sent, or why not.
export type Sent = { to: string; errors: string[] }

export async function sendTest(action: Action): Promise<Sent> {
  const to = destination(action)
  try {
    await api.testAction(action)
    return { to, errors: [] }
  } catch (e) {
    return { to, errors: errorsOf(e) }
  }
}

export function SentResult({ sent }: { sent: Sent }) {
  const failed = sent.errors.length > 0
  return (
    <div className={`test-result${failed ? ' test-unmet' : ''}`} role="status">
      <div className="inline">
        <span className={`dot dot-${failed ? 'warn' : 'ok'}`} />
        <strong>{failed ? 'Not sent' : `Sent to ${sent.to}`}</strong>
      </div>
      {failed && <p className="small-text">{sent.errors.join(' ')}</p>}
    </div>
  )
}

// An action, new or changed, with a test message that it can send before it is saved.
export function ActionEditor({
  action,
  onCancel,
  onSaved,
}: {
  action: Action | null
  onCancel: () => void
  onSaved: (saved: Action) => void
}) {
  const [draft, setDraft] = useState<Draft>(() => toDraft(action))
  const [idTouched, setIdTouched] = useState(false)
  const [errors, setErrors] = useState<string[]>([])
  const [saving, setSaving] = useState(false)
  const [testing, setTesting] = useState(false)
  const [sent, setSent] = useState<Sent | null>(null)
  const set = <K extends keyof Draft>(key: K, value: Draft[K]) => {
    setDraft((d) => ({ ...d, [key]: value }))
    // What the test did was for the webhook as it was.
    if (key === 'webhookUrl' || key === 'to' || key === 'recipient') setSent(null)
  }

  async function test() {
    setTesting(true)
    setSent(await sendTest(toAction(draft)))
    setTesting(false)
  }

  async function save(event: FormEvent) {
    event.preventDefault()
    setSaving(true)
    try {
      onSaved(await api.saveAction(toAction(draft), action?.id ?? null))
    } catch (e) {
      setErrors(errorsOf(e))
    } finally {
      setSaving(false)
    }
  }

  const channel = draft.to === 'channel'

  return (
    <form
      className="card condition-editor"
      onSubmit={save}
      aria-label={action ? `Edit ${action.name}` : 'New action'}
    >
      <div className="card-head">
        <h2>{action ? `Edit ${action.name}` : 'New action'}</h2>
        <span className="tag">Slack workflow</span>
      </div>
      <div className="grid-2 tight">
        <label className="field">
          Name
          <input
            value={draft.name}
            required
            placeholder="Ops channel"
            onChange={(e) => {
              const name = e.target.value
              setDraft((d) => ({ ...d, name, id: action || idTouched ? d.id : slug(name) }))
            }}
          />
        </label>
        <label className="field">
          Id
          <input
            className="mono"
            value={draft.id}
            required
            disabled={!!action}
            pattern="[a-z0-9][a-z0-9\-]{0,39}"
            onChange={(e) => {
              setIdTouched(true)
              set('id', e.target.value)
            }}
          />
        </label>
      </div>
      <div className="stack">
        <span className="field">Sends to</span>
        <div className="inline wrap gap-16">
          <div className="segmented" role="radiogroup" aria-label="Sends to">
            {TO.map(([value, name]) => (
              <label key={value}>
                <input
                  type="radio"
                  name="to"
                  checked={draft.to === value}
                  onChange={() => set('to', value)}
                />
                {name}
              </label>
            ))}
          </div>
          <input
            className="mono grow"
            aria-label={channel ? 'Channel ID' : 'User ID'}
            placeholder={channel ? 'C012AB3CD' : 'U012AB3CD'}
            spellCheck={false}
            value={draft.recipient}
            onChange={(e) => set('recipient', e.target.value)}
          />
        </div>
        <p className="hint">
          {channel
            ? "In Slack, open the channel's details: its ID is at the bottom of About."
            : "In Slack, open the user's profile, click ⋮, then Copy member ID."}
        </p>
      </div>
      <WorkflowSetup to={draft.to} />
      <label className="field">
        Webhook URL
        <input
          className="mono"
          value={draft.webhookUrl}
          required
          spellCheck={false}
          placeholder="https://hooks.slack.com/triggers/…"
          onChange={(e) => set('webhookUrl', e.target.value)}
        />
      </label>
      <p className="hint">
        Anyone with it can start the workflow, so it is kept in a file only you can read.
      </p>
      {sent && <SentResult sent={sent} />}
      {errors.length > 0 && (
        <ul className="errors" role="alert">
          {errors.map((m) => (
            <li key={m}>{m}</li>
          ))}
        </ul>
      )}
      <div className="foot">
        <button type="button" disabled={testing || !draft.webhookUrl.trim()} onClick={test}>
          {testing ? 'Sending…' : 'Send test'}
        </button>
        <span className="actions">
          <button type="button" onClick={onCancel}>
            Cancel
          </button>
          <button type="submit" className="primary" disabled={saving}>
            {saving ? 'Saving…' : 'Save'}
          </button>
        </span>
      </div>
    </form>
  )
}

// How to make the workflow the action starts, and the variables it gets.
function WorkflowSetup({ to }: { to: SlackTo }) {
  const recipient = RECIPIENT[to]
  return (
    <section className="workflow-setup" aria-label="Workflow setup">
      <span className="field">In Slack's Workflow Builder</span>
      <ol className="steps">
        <li>
          Start a workflow <strong>From a webhook</strong>, with the variables its message uses:{' '}
          <span className="mono">{to}</span> and <span className="mono">text</span> are enough.
        </li>
        <li>
          Add the step <strong>{recipient.step}</strong>, to <span className="mono">{to}</span>,
          with <span className="mono">text</span> as its message.
        </li>
        <li>Publish it, and paste its web request URL below.</li>
      </ol>
      <table className="variables">
        <thead>
          <tr>
            <th>Key</th>
            <th>Data type</th>
            <th>What it holds</th>
          </tr>
        </thead>
        <tbody>
          <tr>
            <td className="mono">{to}</td>
            <td>{recipient.type}</td>
            <td>The {to} above</td>
          </tr>
          {VARIABLES.map(([key, holds]) => (
            <tr key={key}>
              <td className="mono">{key}</td>
              <td>Text</td>
              <td>{holds}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </section>
  )
}

import { type FormEvent, useState } from 'react'
import { freeId, RECIPIENT, recipientKind, slackKind, VARIABLES } from '../actions'
import { api, errorsOf } from '../api'
import type { Action } from '../domain/Action'
import type { Recipient } from '../domain/Recipient'
import { slug } from '../format'
import { ActionIcon } from './ActionIcon'
import { Icon } from './Icon'

// A recipient as typed in the form: `key` tells its row apart while it is edited, and a new one has no id yet.
type Row = Recipient & { key: number }

// What the form's fields hold, as typed; `toAction` turns it into the action the Rust side checks.
type Draft = {
  id: string
  name: string
  webhookUrl: string
  recipients: Row[]
}

let rows = 0
const row = (recipient: Recipient): Row => ({ ...recipient, key: rows++ })

function toDraft(action: Action | null): Draft {
  return {
    id: action?.id ?? '',
    name: action?.name ?? '',
    webhookUrl: action?.target.webhook_url ?? '',
    recipients: (action?.recipients ?? []).map(row),
  }
}

// The action the form describes. A new recipient gets an id from its name, which stays when it is renamed: the jobs
// that send to it keep it.
export function toAction(draft: Draft): Action {
  const recipients: Recipient[] = []
  for (const { key: _, ...recipient } of draft.recipients) {
    const taken = [...recipients, ...draft.recipients.filter((r) => r.id)]
    recipients.push({ ...recipient, id: recipient.id || freeId(recipient.name, taken) })
  }
  return {
    id: draft.id.trim(),
    name: draft.name,
    target: { kind: 'slack', webhook_url: draft.webhookUrl },
    recipients,
  }
}

// What a test message did: sent, or why not.
export type Sent = { to: string; errors: string[] }

export async function sendTest(action: Action, recipient: Recipient | null): Promise<Sent> {
  const to = recipient?.name ?? ''
  try {
    await api.testAction(action, recipient)
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
        <strong>{failed ? 'Not sent' : sent.to ? `Sent to ${sent.to}` : 'Sent'}</strong>
      </div>
      {failed && <p className="small-text">{sent.errors.join(' ')}</p>}
    </div>
  )
}

// "Send test" to a recipient picked beside it, or to none: the workflow sends where it was built to.
export function TestButton({
  recipients,
  disabled,
  onSend,
}: {
  recipients: Recipient[]
  disabled?: boolean
  onSend: (recipient: Recipient | null) => Promise<void>
}) {
  const [to, setTo] = useState(recipients[0]?.id ?? '')
  const [testing, setTesting] = useState(false)
  const recipient = recipients.find((r) => r.id === to) ?? null
  return (
    <span className="inline test-to">
      <button
        type="button"
        disabled={disabled || testing}
        onClick={async () => {
          setTesting(true)
          await onSend(recipient)
          setTesting(false)
        }}
      >
        {testing ? 'Sending…' : 'Send test to'}
      </button>
      <select
        aria-label="Send the test to"
        className="recipient-select"
        value={recipient?.id ?? ''}
        onChange={(e) => setTo(e.target.value)}
      >
        <option value="">No recipient</option>
        {recipients.map((r) => (
          <option key={r.id} value={r.id}>
            {r.name}
          </option>
        ))}
      </select>
    </span>
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
  const [sent, setSent] = useState<Sent | null>(null)
  const set = <K extends keyof Draft>(key: K, value: Draft[K]) => {
    setDraft((d) => ({ ...d, [key]: value }))
    // What the test did was for the webhook as it was.
    if (key === 'webhookUrl') setSent(null)
  }
  const setRecipient = (key: number, change: Partial<Recipient>) =>
    set(
      'recipients',
      draft.recipients.map((r) => (r.key === key ? { ...r, ...change } : r)),
    )
  // Those a test can go to: named, with a Slack ID that is one.
  const testable = toAction(draft).recipients.filter((r) => r.name.trim() && slackKind(r.slack_id))

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

  return (
    <form
      className="card condition-editor"
      onSubmit={save}
      aria-label={action ? `Edit ${action.name}` : 'New action'}
    >
      <div className="card-head">
        <h2>{action ? `Edit ${action.name}` : 'New action'}</h2>
        <span className="tag with-icon">
          <ActionIcon action={toAction(draft)} size={12} />
          Slack workflow
        </span>
      </div>
      <div className="grid-2 tight">
        <label className="field">
          Name
          <input
            value={draft.name}
            required
            placeholder="Claude Jobs message"
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
      <fieldset className="stack recipients" aria-label="Recipients">
        <span className="field">Recipients</span>
        {draft.recipients.map((r, at) => {
          const kind = slackKind(r.slack_id)
          return (
            <div key={r.key} className="recipient-row">
              <input
                aria-label={`Name of recipient ${at + 1}`}
                value={r.name}
                required
                placeholder="Ops channel"
                onChange={(e) => setRecipient(r.key, { name: e.target.value })}
              />
              <input
                className="mono"
                aria-label={`Slack ID of recipient ${at + 1}`}
                value={r.slack_id}
                required
                spellCheck={false}
                placeholder="U012AB3CD or C012AB3CD"
                onChange={(e) => setRecipient(r.key, { slack_id: e.target.value })}
              />
              <span className="muted small-text recipient-kind">
                {kind ? recipientKind({ ...r, slack_id: r.slack_id.trim() }).split(' ')[0] : ''}
              </span>
              <button
                type="button"
                className="square ghost"
                aria-label={`Remove recipient ${at + 1}`}
                onClick={() =>
                  set(
                    'recipients',
                    draft.recipients.filter((other) => other.key !== r.key),
                  )
                }
              >
                <Icon name="close" size={13} />
              </button>
            </div>
          )
        })}
        <div>
          <button
            type="button"
            className="small"
            onClick={() =>
              set('recipients', [...draft.recipients, row({ id: '', name: '', slack_id: '' })])
            }
          >
            <Icon name="plus" size={12} />
            Add recipient
          </button>
        </div>
      </fieldset>
      <WorkflowSetup />
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
      {sent && <SentResult sent={sent} />}
      {errors.length > 0 && (
        <ul className="errors" role="alert">
          {errors.map((m) => (
            <li key={m}>{m}</li>
          ))}
        </ul>
      )}
      <div className="foot">
        <TestButton
          recipients={testable}
          disabled={!draft.webhookUrl.trim()}
          onSend={async (to) => setSent(await sendTest(toAction(draft), to))}
        />
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

// How to make the workflow the action starts, and the variables it gets. Who it sends to comes with each message,
// from the recipient a job picks: `user` for a person, `channel` for a channel.
function WorkflowSetup() {
  return (
    <section className="workflow-setup" aria-label="Workflow setup">
      <span className="field">In Slack's Workflow Builder</span>
      <ol className="steps">
        <li>
          Start a workflow <strong>From a webhook</strong>, with <span className="mono">text</span>{' '}
          and the recipient's variable: <span className="mono">user</span> to message people,{' '}
          <span className="mono">channel</span> to post in channels.
        </li>
        <li>
          Add the step <strong>{RECIPIENT.user.step}</strong> to <span className="mono">user</span>,
          or <strong>{RECIPIENT.channel.step}</strong> to <span className="mono">channel</span>,
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
            <td className="mono">user</td>
            <td>{RECIPIENT.user.type}</td>
            <td>The recipient, when it is a person</td>
          </tr>
          <tr>
            <td className="mono">channel</td>
            <td>{RECIPIENT.channel.type}</td>
            <td>The recipient, when it is a channel</td>
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

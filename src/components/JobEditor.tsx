import { type FormEvent, useState } from 'react'
import { api, errorsOf } from '../api'
import type { Defaults } from '../domain/Defaults'
import type { JobView } from '../domain/JobView'
import { type Draft, toDraft, toJob } from '../editor'
import { DAYS, slug } from '../format'

export function JobEditor({
  job,
  defaults,
  onClose,
  onSaved,
}: {
  job: JobView | null
  defaults: Defaults | null
  onClose: () => void
  onSaved: () => void
}) {
  const [draft, setDraft] = useState<Draft>(() => toDraft(job, defaults))
  const [idTouched, setIdTouched] = useState(false)
  const [errors, setErrors] = useState<string[]>([])
  const [confirmDelete, setConfirmDelete] = useState(false)
  const [saving, setSaving] = useState(false)
  const set = <K extends keyof Draft>(key: K, value: Draft[K]) =>
    setDraft((d) => ({ ...d, [key]: value }))

  async function save(event: FormEvent) {
    event.preventDefault()
    setSaving(true)
    try {
      await api.saveJob(toJob(draft), job?.id ?? null)
      onSaved()
    } catch (e) {
      setErrors(errorsOf(e))
    } finally {
      setSaving(false)
    }
  }

  async function remove() {
    if (!job) return
    if (!confirmDelete) {
      setConfirmDelete(true)
      return
    }
    try {
      await api.deleteJob(job.id)
      onSaved()
    } catch (e) {
      setErrors(errorsOf(e))
    }
  }

  function toggleDay(day: number) {
    set(
      'weekdays',
      draft.weekdays.includes(day)
        ? draft.weekdays.filter((d) => d !== day)
        : [...draft.weekdays, day],
    )
  }

  return (
    <form className="dlg" onSubmit={save}>
      <h2 id="editor-title">{job ? `Edit ${job.name}` : 'New job'}</h2>
      <div className="row">
        <label>
          Name
          <input
            value={draft.name}
            required
            onChange={(e) => {
              const name = e.target.value
              setDraft((d) => ({ ...d, name, id: job || idTouched ? d.id : slug(name) }))
            }}
          />
        </label>
        <label>
          Id
          <input
            className="mono"
            value={draft.id}
            required
            disabled={!!job}
            pattern="[a-z0-9][a-z0-9-]{0,39}"
            onChange={(e) => {
              setIdTouched(true)
              set('id', e.target.value)
            }}
          />
        </label>
      </div>
      <label>
        Prompt
        <textarea
          value={draft.prompt}
          required
          placeholder="/photobook-monitor"
          onChange={(e) => set('prompt', e.target.value)}
        />
      </label>
      <div className="row">
        <label>
          Time
          <input
            type="time"
            value={draft.time}
            required
            onChange={(e) => set('time', e.target.value)}
          />
        </label>
        <label>
          Timeout (minutes)
          <input
            type="number"
            min={1}
            max={240}
            value={draft.timeout}
            onChange={(e) => set('timeout', e.target.value)}
          />
        </label>
      </div>
      <fieldset className="days">
        <legend className="muted">Days (none checked = every day)</legend>
        {DAYS.map((name, i) => (
          <label key={name} className="day">
            <input
              type="checkbox"
              checked={draft.weekdays.includes(i + 1)}
              onChange={() => toggleDay(i + 1)}
            />
            {name}
          </label>
        ))}
      </fieldset>
      <label>
        Allowed tools, one per line
        <textarea
          value={draft.tools}
          placeholder={'Bash(python3:*)\nRead'}
          onChange={(e) => set('tools', e.target.value)}
        />
      </label>
      <details>
        <summary>Advanced</summary>
        <div className="dlg nested">
          <label>
            Working folder
            <input
              className="mono"
              value={draft.cwd}
              onChange={(e) => set('cwd', e.target.value)}
            />
          </label>
          <label>
            Model (empty = default)
            <input
              className="mono"
              value={draft.model}
              onChange={(e) => set('model', e.target.value)}
            />
          </label>
          <label>
            Claude binary
            <input
              className="mono"
              value={draft.claude}
              onChange={(e) => set('claude', e.target.value)}
            />
          </label>
          <label>
            PATH for runs
            <textarea value={draft.path} onChange={(e) => set('path', e.target.value)} />
          </label>
        </div>
      </details>
      <label className="check">
        <input
          type="checkbox"
          checked={draft.interactive}
          onChange={(e) => set('interactive', e.target.checked)}
        />
        Interactive session
      </label>
      <p className="muted hint">
        Runs a real Claude Code session in tmux instead of <span className="mono">claude -p</span>.
        Needed for tools a headless run does not get, such as the Artifact tools; you can watch it
        live while it runs.
      </p>
      <label className="check">
        <input
          type="checkbox"
          checked={draft.enabled}
          onChange={(e) => set('enabled', e.target.checked)}
        />
        Enabled
      </label>
      {errors.length > 0 && (
        <ul className="errors" role="alert">
          {errors.map((m) => (
            <li key={m}>{m}</li>
          ))}
        </ul>
      )}
      <div className="foot">
        {job ? (
          <button type="button" className="danger" onClick={remove}>
            {confirmDelete ? 'Click again to delete' : 'Delete job'}
          </button>
        ) : (
          <span />
        )}
        <span className="actions">
          <button type="button" onClick={onClose}>
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

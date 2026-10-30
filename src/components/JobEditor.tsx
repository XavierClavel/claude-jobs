import { type FormEvent, useEffect, useState } from 'react'
import { api, errorsOf } from '../api'
import type { Condition } from '../domain/Condition'
import type { Defaults } from '../domain/Defaults'
import type { JobView } from '../domain/JobView'
import type { Skill } from '../domain/Skill'
import type { TmuxStatus } from '../domain/TmuxStatus'
import {
  type Draft,
  draftErrors,
  hasShortMonths,
  type Repeat,
  toDraft,
  toJob,
  withRepeat,
} from '../editor'
import { DAYS, MONTH_DAYS, scheduleText, slug } from '../format'
import { PromptField } from './PromptField'
import { TmuxNotice } from './TmuxNotice'

const REPEATS: [Repeat, string][] = [
  ['daily', 'Daily'],
  ['weekly', 'Weekly'],
  ['monthly', 'Monthly'],
]

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
  const [tmux, setTmux] = useState<TmuxStatus | null>(null)
  const [installing, setInstalling] = useState(false)
  const [skills, setSkills] = useState<Skill[]>([])
  const set = <K extends keyof Draft>(key: K, value: Draft[K]) =>
    setDraft((d) => ({ ...d, [key]: value }))

  // An interactive job runs in tmux: checked where its runs will look, the job's PATH.
  useEffect(() => {
    if (!draft.interactive) return
    api.tmuxStatus(draft.path.trim()).then(setTmux, () => setTmux(null))
  }, [draft.interactive, draft.path])

  // The skills the prompt suggests: those the job's runs find, which depend on its folder, Claude binary and PATH.
  // Each check starts Claude Code, so it waits for a pause in typing.
  useEffect(() => {
    let latest = true
    const timer = setTimeout(() => {
      api.listSkills(draft.claude.trim(), draft.path.trim(), draft.cwd.trim()).then(
        (found) => {
          if (latest) setSkills(found)
        },
        () => {
          if (latest) setSkills([])
        },
      )
    }, 300)
    return () => {
      latest = false
      clearTimeout(timer)
    }
  }, [draft.claude, draft.path, draft.cwd])

  async function installTmux() {
    setInstalling(true)
    try {
      setTmux(await api.installTmux(draft.path.trim()))
      setErrors([])
    } catch (e) {
      setErrors(errorsOf(e))
    } finally {
      setInstalling(false)
    }
  }

  async function save(event: FormEvent) {
    event.preventDefault()
    const missing = draftErrors(draft)
    if (missing.length) {
      setErrors(missing)
      return
    }
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

  function toggleCondition(condition: Condition, on: boolean) {
    const rest = draft.conditions.filter((c) => c !== condition)
    set('conditions', on ? [...rest, condition] : rest)
  }

  function toggleDay(key: 'weekdays' | 'monthDays', day: number) {
    const days = draft[key]
    set(key, days.includes(day) ? days.filter((d) => d !== day) : [...days, day])
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
      <PromptField value={draft.prompt} skills={skills} onChange={(v) => set('prompt', v)} />
      <fieldset className="schedule">
        <legend>Schedule</legend>
        <div className="segmented" role="radiogroup" aria-label="Repeat">
          {REPEATS.map(([value, name]) => (
            <label key={value}>
              <input
                type="radio"
                name="repeat"
                checked={draft.repeat === value}
                onChange={() => setDraft((d) => withRepeat(d, value))}
              />
              {name}
            </label>
          ))}
        </div>
        {draft.repeat === 'weekly' && (
          <fieldset className="toggles" aria-label="Days of the week">
            {DAYS.map((name, i) => (
              <label key={name} className="toggle">
                <input
                  type="checkbox"
                  checked={draft.weekdays.includes(i + 1)}
                  onChange={() => toggleDay('weekdays', i + 1)}
                />
                {name}
              </label>
            ))}
          </fieldset>
        )}
        {draft.repeat === 'monthly' && (
          <fieldset className="toggles month-days" aria-label="Days of the month">
            {MONTH_DAYS.map((day) => (
              <label key={day} className="toggle">
                <input
                  type="checkbox"
                  checked={draft.monthDays.includes(day)}
                  onChange={() => toggleDay('monthDays', day)}
                />
                {day}
              </label>
            ))}
          </fieldset>
        )}
        <div className="schedule-time">
          <label>
            Time
            <input
              type="time"
              value={draft.time}
              required
              onChange={(e) => set('time', e.target.value)}
            />
          </label>
          <p className="muted">{draftErrors(draft)[0] ?? scheduleText(toJob(draft))}</p>
        </div>
        {draft.repeat === 'monthly' && hasShortMonths(draft.monthDays) && (
          <>
            <label className="check">
              <input
                type="checkbox"
                checked={draft.lastDay}
                onChange={(e) => set('lastDay', e.target.checked)}
              />
              Run on the last day of shorter months
            </label>
            <p className="muted hint">
              {draft.lastDay
                ? 'A job on the 31st runs on 30 April, and on 28 February (29 in a leap year).'
                : 'Otherwise a month without one of these days skips it: a job on the 31st does not run in April.'}
            </p>
          </>
        )}
      </fieldset>
      <label className="narrow">
        Timeout (minutes)
        <input
          type="number"
          min={1}
          max={240}
          value={draft.timeout}
          onChange={(e) => set('timeout', e.target.value)}
        />
      </label>
      <label>
        Allowed tools, one per line
        <textarea
          value={draft.tools}
          placeholder={'Bash(python3:*)\nRead'}
          onChange={(e) => set('tools', e.target.value)}
        />
      </label>
      <fieldset className="box">
        <legend>Start only when</legend>
        <label className="check">
          <input type="checkbox" checked disabled />
          Claude Code is signed in
        </label>
        <label className="check">
          <input
            type="checkbox"
            checked={draft.conditions.includes('gcloud')}
            onChange={(e) => toggleCondition('gcloud', e.target.checked)}
          />
          Google Cloud is signed in
        </label>
        <p className="muted hint">
          Every job needs Claude Code; tick Google Cloud for a job that reads BigQuery or Cloud
          Logging. Until they are met, a run waits, checks again every 30 seconds, and starts as
          soon as they are.
        </p>
      </fieldset>
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
        live while it runs. Needs tmux, which the editor can install for you. Saving tells Claude
        Code to trust the working folder, as accepting its trust question would. Not your home
        folder: a session lists every file in its folder as it starts, and from there macOS would
        ask to let Claude Jobs into Documents, Music and Pictures after every update.
      </p>
      {draft.interactive && (
        <TmuxNotice status={tmux} installing={installing} onInstall={installTmux} />
      )}
      <label className="check">
        <input
          type="checkbox"
          checked={draft.chrome}
          onChange={(e) => set('chrome', e.target.checked)}
        />
        Use Chrome
      </label>
      <p className="muted hint">
        Starts Claude with <span className="mono">--chrome</span>, for the Claude in Chrome tools: a
        headless run does not load them otherwise. Claude does not open Chrome itself, so a prompt
        that may find it closed has to open it. Allow the browser tools above.
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

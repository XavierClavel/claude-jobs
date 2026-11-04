import { type FormEvent, useEffect, useRef, useState } from 'react'
import { api, errorsOf } from '../api'
import {
  type Checks,
  commandOf,
  conditionState,
  fullName,
  isBuiltIn,
  optionalConditions,
  required,
} from '../conditions'
import type { Action } from '../domain/Action'
import type { Defaults } from '../domain/Defaults'
import type { EventKind } from '../domain/EventKind'
import type { JobView } from '../domain/JobView'
import type { Skill } from '../domain/Skill'
import type { TmuxStatus } from '../domain/TmuxStatus'
import {
  type Draft,
  draftErrors,
  hasShortMonths,
  type Repeat,
  type Starts,
  toDraft,
  toJob,
  withRepeat,
} from '../editor'
import { DAYS, MONTH_DAYS, slug } from '../format'
import { AfterRunCard, DuringRunCard } from './AfterRunCard'
import { ConditionIcon } from './ConditionIcon'
import { Icon } from './Icon'
import { PromptField } from './PromptField'
import { RepoPicker } from './RepoPicker'
import { Switch } from './Switch'
import { TmuxNotice } from './TmuxNotice'
import { useDismiss } from './useDismiss'

const REPEATS: [Repeat, string][] = [
  ['daily', 'Daily'],
  ['weekly', 'Weekly'],
  ['monthly', 'Monthly'],
]

const STARTS: [Starts, string][] = [
  ['schedule', 'On a schedule'],
  ['trigger', 'On trigger'],
]

const TRIGGERS: [EventKind, string][] = [
  ['pull_request_opened', 'Pull request opened'],
  ['pull_request_merged', 'Pull request merged'],
]

export function JobEditor({
  job,
  defaults,
  checks,
  actions,
  onActionSaved,
  onClose,
  onSaved,
  onDeleted,
}: {
  job: JobView | null
  defaults: Defaults | null
  checks: Checks
  // What a run can send with, during it or when it ends, each with who it can send to.
  actions: Action[]
  // An action given a new recipient from the job's picker, saved.
  onActionSaved: (saved: Action) => void
  onClose: () => void
  onSaved: (saved: JobView) => void
  onDeleted: () => void
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
      onSaved(await api.saveJob(toJob(draft), job?.id ?? null))
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
      onDeleted()
    } catch (e) {
      setErrors(errorsOf(e))
    }
  }

  function toggleCondition(condition: string, on: boolean) {
    const rest = draft.conditions.filter((c) => c !== condition)
    set('conditions', on ? [...rest, condition] : rest)
  }

  function toggleDay(key: 'weekdays' | 'monthDays', day: number) {
    const days = draft[key]
    set(key, days.includes(day) ? days.filter((d) => d !== day) : [...days, day])
  }

  const scheduleError = draftErrors(draft)[0]

  return (
    <form className="view" onSubmit={save} aria-labelledby="editor-title">
      <header className="view-head bordered">
        <div className="view-title">
          <h1 id="editor-title">{job ? `Edit ${job.name}` : 'New job'}</h1>
        </div>
        <div className="actions">
          <button type="button" onClick={onClose}>
            Cancel
          </button>
          <button type="submit" className="primary" disabled={saving}>
            {saving ? 'Saving…' : 'Save'}
          </button>
        </div>
      </header>
      {errors.length > 0 && (
        <ul className="errors pane-errors" role="alert">
          {errors.map((m) => (
            <li key={m}>{m}</li>
          ))}
        </ul>
      )}

      <div className="editor">
        <div className="stack-16">
          <section className="card">
            <div className="grid-2 tight">
              <label className="field">
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
              <label className="field">
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
          </section>

          <fieldset className="card">
            <legend className="lbl">Starts</legend>
            <div className="segmented wide" role="radiogroup" aria-label="Starts">
              {STARTS.map(([value, name]) => (
                <label key={value}>
                  <input
                    type="radio"
                    name="starts"
                    checked={draft.starts === value}
                    onChange={() => set('starts', value)}
                  />
                  {name}
                </label>
              ))}
            </div>
            {draft.starts === 'trigger' ? (
              <PullRequestFields
                kind={draft.trigger}
                onKind={(v) => set('trigger', v)}
                repos={draft.repos}
                path={draft.path.trim()}
                mine={draft.mine}
                onRepos={(v) => set('repos', v)}
                onMine={(v) => set('mine', v)}
              />
            ) : (
              <>
                <div className="inline wrap gap-16">
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
                  <label className="field inline">
                    at
                    <input
                      type="time"
                      className="time"
                      aria-label="Time"
                      value={draft.time}
                      required
                      onChange={(e) => set('time', e.target.value)}
                    />
                  </label>
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
                {scheduleError && <p className="hint warn-text">{scheduleError}</p>}
                {draft.repeat === 'monthly' && hasShortMonths(draft.monthDays) && (
                  <label className="check">
                    <input
                      type="checkbox"
                      checked={draft.lastDay}
                      onChange={(e) => set('lastDay', e.target.checked)}
                    />
                    Run on the last day of shorter months
                  </label>
                )}
              </>
            )}
          </fieldset>

          <section className="card">
            <label className="field">
              Allowed tools
              <textarea
                rows={3}
                value={draft.tools}
                placeholder={'Bash(python3:*)\nRead'}
                onChange={(e) => set('tools', e.target.value)}
              />
            </label>
          </section>
        </div>

        <div className="stack-16">
          <ConditionsCard
            conditions={draft.conditions}
            checks={checks}
            onToggle={toggleCondition}
          />

          <DuringRunCard
            uses={draft.duringRun}
            actions={actions}
            onActionSaved={onActionSaved}
            onChange={(uses) => set('duringRun', uses)}
          />

          <AfterRunCard
            picked={{
              onSuccess: draft.onSuccess,
              onFailure: draft.onFailure,
              onComplete: draft.onComplete,
            }}
            actions={actions}
            onActionSaved={onActionSaved}
            onChange={set}
          />

          <section className="card switches" aria-label="Run as">
            <div className="lbl">Run as</div>
            <Switch label="Enabled" checked={draft.enabled} onChange={(on) => set('enabled', on)} />
            <Switch
              label="Interactive session"
              checked={draft.interactive}
              onChange={(on) => set('interactive', on)}
            />
            {draft.interactive && (
              <TmuxNotice status={tmux} installing={installing} onInstall={installTmux} />
            )}
            <Switch
              label="Use Chrome"
              checked={draft.chrome}
              onChange={(on) => set('chrome', on)}
            />
          </section>

          <section className="card">
            <label className="field inline spread">
              Timeout
              <span className="inline normal">
                <input
                  type="number"
                  className="minutes"
                  min={1}
                  max={240}
                  value={draft.timeout}
                  onChange={(e) => set('timeout', e.target.value)}
                />
                minutes
              </span>
            </label>
          </section>

          <details className="advanced">
            <summary>
              <Icon name="chevron" size={12} />
              Advanced
            </summary>
            <div className="stack">
              <label className="field">
                Working folder
                <input
                  className="mono"
                  value={draft.cwd}
                  onChange={(e) => set('cwd', e.target.value)}
                />
              </label>
              <label className="field">
                Model (empty = default)
                <input
                  className="mono"
                  value={draft.model}
                  onChange={(e) => set('model', e.target.value)}
                />
              </label>
              <label className="field">
                Claude binary
                <input
                  className="mono"
                  value={draft.claude}
                  onChange={(e) => set('claude', e.target.value)}
                />
              </label>
              <label className="field">
                PATH for runs
                <textarea value={draft.path} onChange={(e) => set('path', e.target.value)} />
              </label>
            </div>
          </details>

          {job && (
            <button type="button" className="danger start" onClick={remove}>
              {confirmDelete ? 'Click again to delete' : 'Delete job…'}
            </button>
          )}
        </div>
      </div>
    </form>
  )
}

// Which pull requests start the job: opened or merged, in which repositories, and whose.
function PullRequestFields({
  kind,
  onKind,
  repos,
  path,
  mine,
  onRepos,
  onMine,
}: {
  kind: EventKind
  onKind: (kind: EventKind) => void
  repos: string[]
  // The job's PATH, where the GitHub CLI is found.
  path: string
  mine: boolean
  onRepos: (repos: string[]) => void
  onMine: (mine: boolean) => void
}) {
  return (
    <>
      <div className="segmented wide" role="radiogroup" aria-label="Trigger">
        {TRIGGERS.map(([value, name]) => (
          <label key={value}>
            <input
              type="radio"
              name="trigger"
              checked={kind === value}
              onChange={() => onKind(value)}
            />
            {name}
          </label>
        ))}
      </div>
      <RepoPicker picked={repos} path={path} onChange={onRepos} />
      <label className="check">
        <input type="checkbox" checked={mine} onChange={(e) => onMine(e.target.checked)} />
        Only my pull requests
      </label>
    </>
  )
}

// What the job waits for: those on every job, and those picked from the conditions that exist.
function ConditionsCard({
  conditions,
  checks,
  onToggle,
}: {
  conditions: string[]
  checks: Checks
  onToggle: (condition: string, on: boolean) => void
}) {
  const [open, setOpen] = useState(false)
  const ref = useRef<HTMLElement>(null)
  useDismiss(open, ref, setOpen)

  const status = (condition: string) => {
    const state = conditionState(condition, checks)
    const tone = state.met === null ? 'muted' : state.met ? 'ok' : 'warn'
    const text =
      state.met === null
        ? 'Not checked yet'
        : state.met
          ? 'Met'
          : 'Not met right now · its next run would wait'
    return { tone, text }
  }

  const everyJob = required({ conditions: [] }, checks)
  const optional = optionalConditions(checks)
  // A condition the job ticked that has since been made one for every job shows once, locked.
  const picked = optional.filter((c) => conditions.includes(c))
  const about = (condition: string) =>
    condition === 'gcloud'
      ? 'For BigQuery or Cloud Logging'
      : (commandOf(condition, checks)?.command ?? '')

  return (
    <section className="card picker-anchor" aria-label="Conditions" ref={ref}>
      <div className="card-head">
        <div className="lbl">Starts only when</div>
        <button
          type="button"
          className="small"
          aria-expanded={open}
          aria-haspopup="menu"
          onClick={() => setOpen((o) => !o)}
        >
          <Icon name="plus" size={12} />
          Add condition
        </button>
      </div>
      <div className="stack-6">
        {everyJob.map((condition) => (
          <div key={condition} className="condition-item">
            <span className={`dot dot-${status(condition).tone}`} />
            <ConditionIcon condition={condition} size={16} />
            <span className="condition-text">
              <span className="condition-name">{fullName(condition, checks)}</span>
              <span className="condition-sub">
                Required by every job · {status(condition).text}
              </span>
            </span>
            <span className="muted" role="img" aria-label="Always on" title="Every job needs it">
              <Icon name="lock" />
            </span>
          </div>
        ))}
        {picked.map((condition) => (
          <div key={condition} className="condition-item">
            <span className={`dot dot-${status(condition).tone}`} />
            <ConditionIcon condition={condition} size={16} />
            <span className="condition-text">
              <span className="condition-name">{fullName(condition, checks)}</span>
              <span className="condition-sub">{status(condition).text}</span>
            </span>
            <button
              type="button"
              className="square ghost"
              aria-label={`Remove ${fullName(condition, checks)}`}
              onClick={() => onToggle(condition, false)}
            >
              <Icon name="close" size={13} />
            </button>
          </div>
        ))}
      </div>
      {open && (
        <div role="menu" aria-label="Conditions" className="picker">
          <div className="lbl picker-title">Conditions</div>
          {everyJob.map((condition) => (
            <div
              key={condition}
              role="menuitemcheckbox"
              aria-checked="true"
              aria-disabled="true"
              tabIndex={-1}
              className="pick"
            >
              <Icon name="check" />
              <ConditionIcon condition={condition} size={16} />
              <span className="condition-text">
                <span className="condition-name">{fullName(condition, checks)}</span>
                <span className="condition-sub">On every job</span>
              </span>
            </div>
          ))}
          {optional.map((condition) => {
            const on = conditions.includes(condition)
            return (
              <button
                type="button"
                key={condition}
                role="menuitemcheckbox"
                aria-checked={on}
                className={`pick${on ? ' selected' : ''}`}
                onClick={() => {
                  onToggle(condition, !on)
                  setOpen(false)
                }}
              >
                <span className={on ? undefined : 'invisible'}>
                  <Icon name="check" />
                </span>
                <ConditionIcon condition={condition} size={16} />
                <span className="condition-text">
                  <span className="condition-name">{fullName(condition, checks)}</span>
                  <span className={`condition-sub${isBuiltIn(condition) ? '' : ' mono'}`}>
                    {on ? 'Added' : about(condition)}
                  </span>
                </span>
              </button>
            )
          })}
        </div>
      )}
    </section>
  )
}

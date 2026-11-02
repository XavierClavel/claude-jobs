import { type FormEvent, useState } from 'react'
import { api, errorsOf } from '../api'
import type { CommandCheck } from '../domain/CommandCheck'
import type { CommandCondition } from '../domain/CommandCondition'
import { slug } from '../format'
import { Switch } from './Switch'

type Match = 'exit_code' | 'equals' | 'contains'

// What the form's fields hold, as typed; `toCondition` turns it into the condition the Rust side checks.
type Draft = {
  id: string
  name: string
  command: string
  match: Match
  code: string
  text: string
  everyJob: boolean
  hint: string
}

function toDraft(condition: CommandCondition | null): Draft {
  const expect = condition?.expect
  return {
    id: condition?.id ?? '',
    name: condition?.name ?? '',
    command: condition?.command ?? '',
    match: expect?.kind === 'stdout' ? (expect.contains ? 'contains' : 'equals') : 'exit_code',
    code: expect?.kind === 'exit_code' ? String(expect.code) : '0',
    text: expect?.kind === 'stdout' ? expect.text : '',
    everyJob: condition?.every_job ?? false,
    hint: condition?.hint ?? '',
  }
}

export function toCondition(draft: Draft): CommandCondition {
  return {
    id: draft.id.trim(),
    name: draft.name,
    command: draft.command,
    expect:
      draft.match === 'exit_code'
        ? { kind: 'exit_code', code: Number.parseInt(draft.code, 10) || 0 }
        : { kind: 'stdout', text: draft.text, contains: draft.match === 'contains' },
    every_job: draft.everyJob,
    hint: draft.hint,
  }
}

const MATCHES: [Match, string][] = [
  ['exit_code', 'Exit code'],
  ['equals', 'Output is'],
  ['contains', 'Output contains'],
]

// A command condition, new or changed, with a Test that runs its command as a job would before it is saved.
export function CommandConditionEditor({
  condition,
  onCancel,
  onSaved,
}: {
  condition: CommandCondition | null
  onCancel: () => void
  onSaved: (saved: CommandCondition) => void
}) {
  const [draft, setDraft] = useState<Draft>(() => toDraft(condition))
  const [idTouched, setIdTouched] = useState(false)
  const [errors, setErrors] = useState<string[]>([])
  const [saving, setSaving] = useState(false)
  const [testing, setTesting] = useState(false)
  const [tested, setTested] = useState<CommandCheck | null>(null)
  const set = <K extends keyof Draft>(key: K, value: Draft[K]) => {
    setDraft((d) => ({ ...d, [key]: value }))
    // What the test found was for the command and check as they were.
    if (key === 'command' || key === 'match' || key === 'code' || key === 'text') setTested(null)
  }

  async function test() {
    setTesting(true)
    try {
      setTested(await api.checkCondition(toCondition(draft)))
      setErrors([])
    } catch (e) {
      setErrors(errorsOf(e))
    } finally {
      setTesting(false)
    }
  }

  async function save(event: FormEvent) {
    event.preventDefault()
    setSaving(true)
    try {
      onSaved(await api.saveCondition(toCondition(draft), condition?.id ?? null))
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
      aria-label={condition ? `Edit ${condition.name}` : 'New condition'}
    >
      <div className="card-head">
        <h2>{condition ? `Edit ${condition.name}` : 'New condition'}</h2>
      </div>
      <div className="grid-2 tight">
        <label className="field">
          Name
          <input
            value={draft.name}
            required
            placeholder="VPN connected"
            onChange={(e) => {
              const name = e.target.value
              setDraft((d) => ({ ...d, name, id: condition || idTouched ? d.id : slug(name) }))
            }}
          />
        </label>
        <label className="field">
          Id
          <input
            className="mono"
            value={draft.id}
            required
            disabled={!!condition}
            pattern="[a-z0-9][a-z0-9\-]{0,39}"
            onChange={(e) => {
              setIdTouched(true)
              set('id', e.target.value)
            }}
          />
        </label>
      </div>
      <label className="field">
        Command
        <textarea
          rows={2}
          value={draft.command}
          required
          placeholder="scutil --nc status Work | head -1"
          onChange={(e) => set('command', e.target.value)}
        />
      </label>
      <p className="hint">
        Run with <span className="mono">/bin/bash -c</span>, with the PATH of the job it is checked
        for. It has 20 seconds to finish.
      </p>
      <div className="stack">
        <span className="field">Met when</span>
        <div className="inline wrap gap-16">
          <div className="segmented" role="radiogroup" aria-label="Met when">
            {MATCHES.map(([value, name]) => (
              <label key={value}>
                <input
                  type="radio"
                  name="match"
                  checked={draft.match === value}
                  onChange={() => set('match', value)}
                />
                {name}
              </label>
            ))}
          </div>
          {draft.match === 'exit_code' ? (
            <label className="field inline normal">
              is
              <input
                type="number"
                className="minutes"
                aria-label="Expected exit code"
                value={draft.code}
                onChange={(e) => set('code', e.target.value)}
              />
            </label>
          ) : (
            <input
              className="mono grow"
              aria-label={draft.match === 'equals' ? 'Output' : 'Text in the output'}
              placeholder="Connected"
              value={draft.text}
              onChange={(e) => set('text', e.target.value)}
            />
          )}
        </div>
        <p className="hint">
          {draft.match === 'exit_code'
            ? 'Its exit code, whatever it prints: 0 means success.'
            : 'What it prints on stdout, whatever its exit code, without the spaces and line breaks around it.'}
        </p>
      </div>
      <label className="field">
        When it is not met, say
        <input
          value={draft.hint}
          placeholder="Connect to the VPN"
          onChange={(e) => set('hint', e.target.value)}
        />
      </label>
      <div className="switches">
        <Switch label="Every job" checked={draft.everyJob} onChange={(on) => set('everyJob', on)}>
          Every job waits for it, as for Claude Code. Off: only the jobs that tick it in their
          editor.
        </Switch>
      </div>
      {tested && <TestResult tested={tested} />}
      {errors.length > 0 && (
        <ul className="errors" role="alert">
          {errors.map((m) => (
            <li key={m}>{m}</li>
          ))}
        </ul>
      )}
      <div className="foot">
        <button type="button" disabled={testing || !draft.command.trim()} onClick={test}>
          {testing ? 'Testing…' : 'Test'}
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

function TestResult({ tested }: { tested: CommandCheck }) {
  const output = tested.stdout.trim()
  return (
    <div className={`test-result${tested.problem ? ' test-unmet' : ''}`} role="status">
      <div className="inline">
        <span className={`dot dot-${tested.problem ? 'warn' : 'ok'}`} />
        <strong>{tested.problem ? 'Not met' : 'Met'}</strong>
        <span className="muted small-text">
          {tested.exit_code === null ? 'no exit code' : `exit code ${tested.exit_code}`}
        </span>
      </div>
      {tested.problem && <p className="small-text">{tested.problem}</p>}
      <pre>{output || '(no output)'}</pre>
    </div>
  )
}

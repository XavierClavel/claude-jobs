import { useEffect, useState } from 'react'
import { api, errorsOf } from '../api'
import { when } from '../format'

type Preview = { runs: string[]; error: string | null }

const NONE: Preview = { runs: [], error: null }

// A cron expression, with the next times it runs, or what is wrong with it, as the Rust side reads it. Each check
// waits for a pause in typing.
export function CronField({
  value,
  onChange,
}: {
  value: string
  onChange: (value: string) => void
}) {
  const [preview, setPreview] = useState<Preview>(NONE)

  useEffect(() => {
    if (!value.trim()) {
      setPreview(NONE)
      return
    }
    let latest = true
    const timer = setTimeout(() => {
      api.cronPreview(value).then(
        (runs) => {
          if (latest) setPreview({ runs, error: null })
        },
        (e) => {
          if (latest) setPreview({ runs: [], error: errorsOf(e)[0] ?? null })
        },
      )
    }, 200)
    return () => {
      latest = false
      clearTimeout(timer)
    }
  }, [value])

  return (
    <div className="stack-6">
      <label className="field">
        Cron expression
        <input
          className="mono"
          value={value}
          placeholder="*/15 9-17 * * 1-5"
          spellCheck={false}
          autoCapitalize="off"
          autoCorrect="off"
          onChange={(e) => onChange(e.target.value)}
        />
      </label>
      <p className="hint">minute · hour · day of the month · month · day of the week</p>
      {preview.error && <p className="hint warn-text">{preview.error}</p>}
      {preview.runs.length > 0 && (
        <output className="hint" aria-label="Next runs">
          Next: {preview.runs.map((run) => when(run)).join(' · ')}
        </output>
      )}
    </div>
  )
}

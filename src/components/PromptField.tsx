import { type KeyboardEvent, useEffect, useId, useLayoutEffect, useRef, useState } from 'react'
import type { Skill } from '../domain/Skill'
import { complete, matching, slashAt } from '../skills'

// The prompt, suggesting skills while a `/name` is typed: the arrows pick one, Enter or Tab puts it in, Escape hides
// the list until the next key typed.
export function PromptField({
  value,
  skills,
  onChange,
}: {
  value: string
  skills: Skill[]
  onChange: (value: string) => void
}) {
  const ref = useRef<HTMLTextAreaElement>(null)
  const listId = useId()
  // Where the caret is; null while the field does not have the focus, which hides the list.
  const [caret, setCaret] = useState<number | null>(null)
  const [active, setActive] = useState(0)
  const [hidden, setHidden] = useState(false)
  // Where the caret goes once React has put in the picked skill.
  const placeCaret = useRef<number | null>(null)

  const token = caret === null || hidden ? null : slashAt(value, caret)
  const options = token ? matching(skills, token.query) : []
  const open = options.length > 0
  const current = Math.min(active, options.length - 1)
  const optionId = (i: number) => `${listId}-${i}`

  useLayoutEffect(() => {
    if (placeCaret.current === null) return
    ref.current?.setSelectionRange(placeCaret.current, placeCaret.current)
    placeCaret.current = null
  })

  useEffect(() => {
    if (open) document.getElementById(optionId(current))?.scrollIntoView?.({ block: 'nearest' })
  })

  function moveCaret(at: number) {
    if (at === caret) return
    setCaret(at)
    setActive(0)
  }

  function pick(skill: Skill) {
    if (!token) return
    const next = complete(value, token, skill.name)
    placeCaret.current = next.caret
    setCaret(next.caret)
    onChange(next.text)
  }

  function onKeyDown(e: KeyboardEvent<HTMLTextAreaElement>) {
    if (!open || e.nativeEvent.isComposing) return
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      e.preventDefault()
      const step = e.key === 'ArrowDown' ? 1 : -1
      setActive((current + step + options.length) % options.length)
    } else if (e.key === 'Enter' || e.key === 'Tab') {
      const skill = options[current]
      if (!skill) return
      e.preventDefault()
      pick(skill)
    } else if (e.key === 'Escape') {
      // Otherwise the dialog closes too.
      e.preventDefault()
      setHidden(true)
    }
  }

  return (
    <div className="prompt">
      <label>
        Prompt
        <textarea
          ref={ref}
          value={value}
          required
          placeholder="/photobook-monitor"
          aria-autocomplete="list"
          aria-controls={open ? listId : undefined}
          aria-activedescendant={open ? optionId(current) : undefined}
          onChange={(e) => {
            setHidden(false)
            moveCaret(e.target.selectionStart)
            onChange(e.target.value)
          }}
          onSelect={(e) => moveCaret(e.currentTarget.selectionStart)}
          onBlur={() => setCaret(null)}
          onKeyDown={onKeyDown}
        />
      </label>
      {open && (
        <div id={listId} role="listbox" aria-label="Skills" className="suggestions">
          {options.map((skill, i) => (
            <div
              key={skill.name}
              id={optionId(i)}
              role="option"
              tabIndex={-1}
              aria-selected={i === current}
              title={skill.description}
              // Keeps the focus in the prompt, which would otherwise hide the list before the click lands.
              onMouseDown={(e) => {
                e.preventDefault()
                pick(skill)
              }}
            >
              <span className="mono">
                /{skill.name}
                {skill.argument_hint && <span className="muted"> {skill.argument_hint}</span>}
              </span>
              {skill.description && <span className="muted">{skill.description}</span>}
            </div>
          ))}
        </div>
      )}
    </div>
  )
}

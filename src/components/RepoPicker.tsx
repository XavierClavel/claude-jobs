import { type KeyboardEvent, useEffect, useRef, useState } from 'react'
import { api, errorsOf } from '../api'
import type { Repo } from '../domain/Repo'
import { ago } from '../format'
import { Icon } from './Icon'
import { useDismiss } from './useDismiss'

// Matches shown at once: typing narrows the rest down.
const SHOWN = 50

// The repositories your GitHub account can see that `query` matches: every word of it, anywhere in `owner/name`.
export function matching(repos: Repo[], query: string): Repo[] {
  const words = query.toLowerCase().split(/\s+/).filter(Boolean)
  return repos.filter((r) => words.every((w) => r.name.toLowerCase().includes(w)))
}

// The repositories a job watches, picked from those your GitHub account can see, newest push first.
export function RepoPicker({
  picked,
  path,
  onChange,
}: {
  picked: string[]
  // The job's PATH, where the GitHub CLI is found.
  path: string
  onChange: (repos: string[]) => void
}) {
  const [repos, setRepos] = useState<Repo[] | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [open, setOpen] = useState(false)
  const [query, setQuery] = useState('')
  const [active, setActive] = useState(0)
  const ref = useRef<HTMLDivElement>(null)
  useDismiss(open, ref, setOpen)

  // Fetched as soon as the job watches pull requests: the list takes a few seconds the first time.
  useEffect(() => {
    let latest = true
    setError(null)
    api.listRepos(path).then(
      (found) => latest && setRepos(found),
      (e) => {
        if (!latest) return
        setRepos([])
        setError(errorsOf(e).join(' '))
      },
    )
    return () => {
      latest = false
    }
  }, [path])

  const found = matching(repos ?? [], query).filter((r) => !picked.includes(r.name))
  const shown = found.slice(0, SHOWN)

  function pick(name: string) {
    onChange([...picked, name])
    setOpen(false)
    setQuery('')
  }

  function onKey(e: KeyboardEvent<HTMLInputElement>) {
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      e.preventDefault()
      const step = e.key === 'ArrowDown' ? 1 : -1
      setActive((i) => Math.min(Math.max(i + step, 0), Math.max(shown.length - 1, 0)))
    } else if (e.key === 'Enter') {
      // Never the form's Save.
      e.preventDefault()
      const repo = shown[active]
      if (repo) pick(repo.name)
    }
  }

  const about = (name: string) => repos?.find((r) => r.name === name)

  return (
    <div className="repo-picker picker-anchor" ref={ref}>
      <div className="card-head">
        <span className="outcome-title">Repositories</span>
        <button
          type="button"
          className="small"
          aria-expanded={open}
          aria-haspopup="listbox"
          onClick={() => {
            setOpen((o) => !o)
            setQuery('')
            setActive(0)
          }}
        >
          <Icon name="plus" size={12} />
          Add repository
        </button>
      </div>
      {picked.length === 0 ? (
        <p className="muted small-text">None yet</p>
      ) : (
        <div className="stack-6">
          {picked.map((name) => {
            const repo = about(name)
            return (
              <div key={name} className="condition-item">
                <OwnerAvatar repo={repo} name={name} />
                <span className="condition-text">
                  <span className="condition-name mono">{name}</span>
                  {repo && <span className="condition-sub">{repoSub(repo)}</span>}
                </span>
                <button
                  type="button"
                  className="square ghost"
                  aria-label={`Remove ${name}`}
                  onClick={() => onChange(picked.filter((p) => p !== name))}
                >
                  <Icon name="close" size={13} />
                </button>
              </div>
            )
          })}
        </div>
      )}
      {open && (
        <div className="picker">
          <input
            // biome-ignore lint/a11y/noAutofocus: the picker opens to be typed in
            autoFocus
            className="picker-search"
            role="combobox"
            aria-label="Search repositories"
            aria-expanded="true"
            aria-controls="repo-options"
            aria-activedescendant={shown[active] ? `repo-${active}` : undefined}
            placeholder="Search your repositories"
            value={query}
            onChange={(e) => {
              setQuery(e.target.value)
              setActive(0)
            }}
            onKeyDown={onKey}
          />
          <div id="repo-options" role="listbox" aria-label="Repositories" className="picker-list">
            {repos === null ? (
              <p className="muted small-text picker-note">Loading…</p>
            ) : error ? (
              <p className="small-text picker-note warn-text">{error}</p>
            ) : shown.length === 0 ? (
              <p className="muted small-text picker-note">
                {query ? `No repository matches “${query}”.` : 'Every repository is added.'}
              </p>
            ) : (
              shown.map((repo, i) => (
                <div
                  key={repo.name}
                  id={`repo-${i}`}
                  role="option"
                  tabIndex={-1}
                  aria-selected={i === active}
                  className={`pick${i === active ? ' selected' : ''}`}
                  onMouseEnter={() => setActive(i)}
                  // Before the input loses focus.
                  onMouseDown={(e) => e.preventDefault()}
                  onClick={() => pick(repo.name)}
                  onKeyDown={() => {}}
                >
                  <OwnerAvatar repo={repo} name={repo.name} />
                  <span className="condition-text">
                    <span className="condition-name mono">{repo.name}</span>
                    <span className="condition-sub">{repoSub(repo)}</span>
                  </span>
                </div>
              ))
            )}
            {found.length > SHOWN && (
              <p className="muted small-text picker-note">{found.length - SHOWN} more</p>
            )}
          </div>
        </div>
      )}
    </div>
  )
}

// The picture of the repository's owner, small: GitHub sizes it with `s`. Its initial until the list has loaded, or
// when there is none.
// The picture of a repository's owner, from the list when it is in it, else from GitHub by the owner's login.
export function OwnerAvatar({ repo, name }: { repo: Repo | undefined; name: string }) {
  const [broken, setBroken] = useState(false)
  const owner = name.split('/')[0]
  const url =
    repo?.owner_avatar ?? (owner ? `https://avatars.githubusercontent.com/${owner}` : null)
  if (!url || broken)
    return (
      <span className="owner-avatar owner-initial" aria-hidden="true">
        {name.charAt(0).toUpperCase()}
      </span>
    )
  const sized = `${url}${url.includes('?') ? '&' : '?'}s=40`
  return (
    <img
      className="owner-avatar"
      src={sized}
      alt=""
      loading="lazy"
      onError={() => setBroken(true)}
    />
  )
}

// "Private · pushed 2 h ago"
function repoSub(repo: Repo): string {
  const pushed = repo.pushed_at ? ` · pushed ${ago(repo.pushed_at)}` : ''
  return `${repo.private ? 'Private' : 'Public'}${pushed}`
}

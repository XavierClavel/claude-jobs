import { useCallback, useEffect, useState } from 'react'
import { api, errorsOf } from './api'
import { Conditions } from './components/Conditions'
import { History } from './components/History'
import { JobDetail } from './components/JobDetail'
import { JobEditor } from './components/JobEditor'
import { Overview } from './components/Overview'
import { Sidebar } from './components/Sidebar'
import { UpdateBar } from './components/UpdateBar'
import type { CheckActions, Checks } from './conditions'
import type { AppUpdate } from './domain/AppUpdate'
import type { CommandConditionView } from './domain/CommandConditionView'
import type { ConditionStatus } from './domain/ConditionStatus'
import type { Defaults } from './domain/Defaults'
import type { GcloudStatus } from './domain/GcloudStatus'
import type { JobView } from './domain/JobView'

// What the pane shows beside the list of jobs.
type View =
  | { kind: 'overview' }
  | { kind: 'job'; id: string }
  | { kind: 'edit'; id: string | null }
  | { kind: 'history'; id: string; run: string | null }
  | { kind: 'conditions' }

export function App() {
  const [jobs, setJobs] = useState<JobView[] | null>(null)
  const [defaults, setDefaults] = useState<Defaults | null>(null)
  const [version, setVersion] = useState<string | null>(null)
  const [view, setView] = useState<View>({ kind: 'overview' })
  const [banner, setBanner] = useState<string[]>([])
  const [gcloud, setGcloud] = useState<GcloudStatus | null>(null)
  const [claude, setClaude] = useState<ConditionStatus | null>(null)
  const [commands, setCommands] = useState<CommandConditionView[]>([])
  const [signingIn, setSigningIn] = useState(false)
  const [update, setUpdate] = useState<AppUpdate | null>(null)
  const [installing, setInstalling] = useState(false)

  const refresh = useCallback(async () => {
    try {
      setJobs(await api.listJobs())
    } catch (e) {
      setBanner(errorsOf(e))
    }
  }, [])

  useEffect(() => {
    refresh()
    api.defaults().then(setDefaults, () => setDefaults(null))
    api.appVersion().then(setVersion, () => setVersion(null))
  }, [refresh])

  // Faster while a run is going or waiting to start, so its end or start shows up without reloading.
  const busy = jobs?.some((j) => j.last_run?.status === 'running' || j.waiting !== null) ?? false
  useEffect(() => {
    const timer = setInterval(refresh, busy ? 4000 : 30000)
    return () => clearInterval(timer)
  }, [refresh, busy])

  // The jobs follow the conditions: what they show comes from these checks.
  const checkConditions = useCallback(
    () =>
      Promise.all([
        api.gcloudStatus().then(setGcloud, () => setGcloud(null)),
        api.claudeStatus().then(setClaude, () => setClaude(null)),
        // A file that does not parse is said, not taken for no conditions.
        api.listConditions().then(setCommands, (e) => setBanner(errorsOf(e))),
      ]).then(refresh),
    [refresh],
  )

  // Checked again when the window comes back, which is when a sign-in in Terminal has just finished.
  useEffect(() => {
    checkConditions()
    const timer = setInterval(checkConditions, 300000)
    window.addEventListener('focus', checkConditions)
    return () => {
      clearInterval(timer)
      window.removeEventListener('focus', checkConditions)
    }
  }, [checkConditions])

  // The window may stay open for days. A failed check (offline, no release yet) keeps what the last one found.
  useEffect(() => {
    const check = () => api.checkUpdate().then(setUpdate, () => {})
    check()
    const timer = setInterval(check, 6 * 3600 * 1000)
    return () => clearInterval(timer)
  }, [])

  useEffect(() => {
    if (!banner.length) return
    const timer = setTimeout(() => setBanner([]), 10000)
    return () => clearTimeout(timer)
  }, [banner])

  // Asked for from the header: unlike the 6-hourly check, it says when it fails.
  async function checkUpdateNow() {
    try {
      const found = await api.checkUpdate()
      setUpdate(found)
      return found === null
    } catch (e) {
      setBanner(errorsOf(e))
      return false
    }
  }

  async function act(action: () => Promise<unknown>, delay = 0) {
    try {
      await action()
    } catch (e) {
      setBanner(errorsOf(e))
    }
    setTimeout(refresh, delay)
  }

  const checks: Checks = { claude, gcloud, signingIn, commands }
  const actions: CheckActions = {
    onCheck: checkConditions,
    onSignIn: (condition) => {
      if (condition === 'claude') {
        act(api.claudeSignIn)
        return
      }
      setSigningIn(true)
      api
        .gcloudSignIn()
        .then(setGcloud, (e) => setBanner(errorsOf(e)))
        .finally(() => {
          setSigningIn(false)
          refresh()
        })
    },
  }

  // A job deleted from elsewhere (the command line) leaves the pane on the overview.
  const job = 'id' in view && view.id !== null ? jobs?.find((j) => j.id === view.id) : undefined
  const missing = jobs !== null && 'id' in view && view.id !== null && !job
  useEffect(() => {
    if (missing) setView({ kind: 'overview' })
  }, [missing])

  const select = (id: string) => setView({ kind: 'job', id })

  return (
    <div className="shell">
      {/* The window has no title bar: this strip moves it, over the traffic lights' row. */}
      <div className="drag" data-tauri-drag-region />
      <Sidebar
        version={version}
        jobs={jobs}
        selected={job?.id ?? null}
        conditionsOpen={view.kind === 'conditions'}
        editing={view.kind === 'edit'}
        checks={checks}
        actions={actions}
        onCheckUpdate={checkUpdateNow}
        onNew={() => setView({ kind: 'edit', id: null })}
        onSelect={(id) =>
          setView(
            view.kind === 'job' && view.id === id ? { kind: 'overview' } : { kind: 'job', id },
          )
        }
        onConditions={() => setView({ kind: 'conditions' })}
        onOverview={() => setView({ kind: 'overview' })}
      />
      <main className="pane">
        <UpdateBar
          update={update}
          installing={installing}
          onInstall={() => {
            setInstalling(true)
            api
              .installUpdate()
              .catch((e) => setBanner(errorsOf(e)))
              .finally(() => setInstalling(false))
          }}
        />
        {banner.length > 0 && (
          <p className="errors banner" role="alert">
            {banner.join('\n')}
          </p>
        )}
        {jobs === null ? (
          <p className="muted view-empty">Loading jobs…</p>
        ) : view.kind === 'edit' && (view.id === null || job) ? (
          <JobEditor
            key={view.id ?? 'new'}
            job={job ?? null}
            defaults={defaults}
            checks={checks}
            onClose={() => setView(job ? { kind: 'job', id: job.id } : { kind: 'overview' })}
            onSaved={(saved) => {
              setView(saved ? { kind: 'job', id: saved.id } : { kind: 'overview' })
              refresh()
            }}
            onDeleted={() => {
              setView({ kind: 'overview' })
              refresh()
            }}
          />
        ) : view.kind === 'history' && job ? (
          <History
            key={`${job.id}/${view.run}`}
            job={job}
            run={view.run}
            onBack={() => select(job.id)}
          />
        ) : view.kind === 'job' && job ? (
          <JobDetail
            job={job}
            checks={checks}
            onRun={() => act(() => api.runNow(job.id), 1500)}
            onToggle={() => act(() => api.setEnabled(job.id, !job.enabled))}
            onEdit={() => setView({ kind: 'edit', id: job.id })}
            onHistory={(run) => setView({ kind: 'history', id: job.id, run: run ?? null })}
            onWatch={() => act(() => api.watchRun(job.id))}
          />
        ) : view.kind === 'conditions' ? (
          <Conditions jobs={jobs} checks={checks} actions={actions} onSelect={select} />
        ) : (
          <Overview
            jobs={jobs}
            checks={checks}
            actions={actions}
            onSelect={select}
            onOpenRun={(id, run) => setView({ kind: 'history', id, run })}
          />
        )}
      </main>
    </div>
  )
}

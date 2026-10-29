import { useCallback, useEffect, useState } from 'react'
import { api, errorsOf } from './api'
import { Dialog } from './components/Dialog'
import { GcloudBar } from './components/GcloudBar'
import { History } from './components/History'
import { JobCard } from './components/JobCard'
import { JobEditor } from './components/JobEditor'
import { UpdateBar } from './components/UpdateBar'
import type { AppUpdate } from './domain/AppUpdate'
import type { Defaults } from './domain/Defaults'
import type { GcloudStatus } from './domain/GcloudStatus'
import type { JobView } from './domain/JobView'

type Editing = { job: JobView | null } | null

export function App() {
  const [jobs, setJobs] = useState<JobView[] | null>(null)
  const [defaults, setDefaults] = useState<Defaults | null>(null)
  const [version, setVersion] = useState<string | null>(null)
  const [editing, setEditing] = useState<Editing>(null)
  const [history, setHistory] = useState<JobView | null>(null)
  const [banner, setBanner] = useState<string[]>([])
  const [gcloud, setGcloud] = useState<GcloudStatus | null>(null)
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

  // Faster while a run is going, so its end shows up without reloading.
  const running = jobs?.some((j) => j.last_run?.status === 'running') ?? false
  useEffect(() => {
    const timer = setInterval(refresh, running ? 4000 : 30000)
    return () => clearInterval(timer)
  }, [refresh, running])

  // Checked again when the window comes back, which is when a sign-in in Terminal has just finished.
  useEffect(() => {
    const check = () => api.gcloudStatus().then(setGcloud, () => setGcloud(null))
    check()
    const timer = setInterval(check, 300000)
    window.addEventListener('focus', check)
    return () => {
      clearInterval(timer)
      window.removeEventListener('focus', check)
    }
  }, [])

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

  async function act(action: () => Promise<unknown>, delay = 0) {
    try {
      await action()
    } catch (e) {
      setBanner(errorsOf(e))
    }
    setTimeout(refresh, delay)
  }

  return (
    <>
      <header className="top">
        <h1>Claude Jobs {version && <span className="version muted">{version}</span>}</h1>
        <button type="button" className="primary" onClick={() => setEditing({ job: null })}>
          New job
        </button>
      </header>
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
      <GcloudBar
        status={gcloud}
        signingIn={signingIn}
        onSignIn={() => {
          setSigningIn(true)
          api
            .gcloudSignIn()
            .then(setGcloud, (e) => setBanner(errorsOf(e)))
            .finally(() => setSigningIn(false))
        }}
      />
      {banner.length > 0 && (
        <p className="errors banner" role="alert">
          {banner.join('\n')}
        </p>
      )}
      <main className="jobs">
        {jobs === null ? (
          <p className="muted">Loading jobs…</p>
        ) : jobs.length === 0 ? (
          <div className="empty">
            <p>No jobs yet.</p>
            <p className="muted">
              A job runs a Claude Code prompt on a schedule, like /photobook-monitor every morning.
            </p>
          </div>
        ) : (
          jobs.map((job) => (
            <JobCard
              key={job.id}
              job={job}
              onRun={() => act(() => api.runNow(job.id), 1500)}
              onToggle={() => act(() => api.setEnabled(job.id, !job.enabled))}
              onEdit={() => setEditing({ job })}
              onHistory={() => setHistory(job)}
              onWatch={() => act(() => api.watchRun(job.id))}
            />
          ))
        )}
      </main>
      <Dialog open={editing !== null} onClose={() => setEditing(null)} labelledBy="editor-title">
        {editing && (
          <JobEditor
            job={editing.job}
            defaults={defaults}
            onClose={() => setEditing(null)}
            onSaved={() => {
              setEditing(null)
              refresh()
            }}
          />
        )}
      </Dialog>
      <Dialog open={history !== null} onClose={() => setHistory(null)} labelledBy="history-title">
        {history && <History job={history} onClose={() => setHistory(null)} />}
      </Dialog>
    </>
  )
}

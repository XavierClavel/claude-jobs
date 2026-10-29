import type { AppUpdate } from '../domain/AppUpdate'
import { Chip } from './Chip'

// A newer release on GitHub. Installing it restarts the window; runs in progress carry on.
export function UpdateBar({
  update,
  installing,
  onInstall,
}: {
  update: AppUpdate | null
  installing: boolean
  onInstall: () => void
}) {
  if (!update) return null
  return (
    <div className="update">
      <Chip tone="running">Update available</Chip>
      <span className="muted">
        {installing
          ? `Downloading ${update.version}. The window reopens on it; runs in progress carry on.`
          : `Claude Jobs ${update.version} is out (you have ${update.current_version}).`}
      </span>
      <button type="button" className="primary" disabled={installing} onClick={onInstall}>
        {installing ? 'Installing…' : 'Install and restart'}
      </button>
    </div>
  )
}

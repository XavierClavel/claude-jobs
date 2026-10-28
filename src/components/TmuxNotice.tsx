import type { TmuxStatus } from '../domain/TmuxStatus'
import { Chip } from './Chip'

// Interactive jobs run in tmux; the editor installs it with Homebrew when it is missing.
export function TmuxNotice({
  status,
  installing,
  onInstall,
}: {
  status: TmuxStatus | null
  installing: boolean
  onInstall: () => void
}) {
  if (!status || status.installed) return null
  return (
    <div className="tmux" role="status">
      <Chip tone="warn">tmux is not installed</Chip>
      {status.can_install ? (
        <>
          <span className="muted">
            {installing
              ? 'Homebrew is installing tmux, which can take a minute or two.'
              : 'Interactive sessions run in tmux.'}
          </span>
          <button type="button" className="primary" disabled={installing} onClick={onInstall}>
            {installing ? 'Installing tmux…' : 'Install tmux'}
          </button>
        </>
      ) : (
        <span className="muted">
          Interactive sessions run in tmux. Install Homebrew from brew.sh to install it from here,
          or install tmux another way.
        </span>
      )}
    </div>
  )
}

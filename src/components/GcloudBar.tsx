import type { GcloudStatus } from '../domain/GcloudStatus'
import { Chip } from './Chip'

// Jobs that read BigQuery or Cloud Logging fail once gcloud's credentials expire.
export function GcloudBar({
  status,
  onSignIn,
}: {
  status: GcloudStatus | null
  onSignIn: () => void
}) {
  if (!status?.installed) return null
  return (
    <div className={`gcloud${status.signed_in ? '' : ' gcloud-out'}`}>
      {status.signed_in ? (
        <Chip tone="ok">Google Cloud: signed in</Chip>
      ) : (
        <Chip tone="warn">Google Cloud: signed out</Chip>
      )}
      <span className="muted">
        {status.signed_in
          ? status.account
          : 'Runs that use BigQuery or Cloud Logging will fail until you sign in again.'}
      </span>
      <button type="button" className={status.signed_in ? undefined : 'primary'} onClick={onSignIn}>
        {status.signed_in ? 'Sign in again' : 'Sign in to Google Cloud'}
      </button>
    </div>
  )
}

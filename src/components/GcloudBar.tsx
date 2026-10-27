import type { GcloudStatus } from '../domain/GcloudStatus'
import { Chip } from './Chip'

// Jobs that read BigQuery or Cloud Logging fail once gcloud's credentials expire.
export function GcloudBar({
  status,
  signingIn,
  onSignIn,
}: {
  status: GcloudStatus | null
  signingIn: boolean
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
        {signingIn
          ? 'Finish signing in in your browser.'
          : status.signed_in
            ? status.account
            : 'Runs that use BigQuery or Cloud Logging will fail until you sign in again.'}
      </span>
      <button
        type="button"
        className={status.signed_in ? undefined : 'primary'}
        disabled={signingIn}
        onClick={onSignIn}
      >
        {signingIn
          ? 'Waiting for the browser…'
          : status.signed_in
            ? 'Sign in again'
            : 'Sign in to Google Cloud'}
      </button>
    </div>
  )
}

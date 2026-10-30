import type { ConditionStatus } from '../domain/ConditionStatus'
import { Chip } from './Chip'

// Every job needs Claude Code signed in: while it is not, runs wait for it, and this bar says why.
export function ClaudeBar({
  status,
  onSignIn,
}: {
  status: ConditionStatus | null
  onSignIn: () => void
}) {
  if (!status?.problem) return null
  return (
    <div className="signin signin-out">
      <Chip tone="warn">Claude Code: not signed in</Chip>
      <span className="muted">
        {status.problem} Jobs wait for it, and start as soon as you have signed in.
      </span>
      <button type="button" className="primary" onClick={onSignIn}>
        Sign in to Claude Code
      </button>
    </div>
  )
}

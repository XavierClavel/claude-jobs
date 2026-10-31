import type { Condition } from './domain/Condition'
import type { ConditionStatus } from './domain/ConditionStatus'
import type { GcloudStatus } from './domain/GcloudStatus'
import type { JobView } from './domain/JobView'

// The last checks of the conditions, as the window has them; null until the first check answers.
export type Checks = {
  claude: ConditionStatus | null
  gcloud: GcloudStatus | null
  // Google Cloud's sign-in happens in the browser, and the app waits for it.
  signingIn: boolean
}

export type CheckActions = {
  onSignIn: (condition: Condition) => void
  onCheck: () => Promise<unknown>
}

// One condition as the window shows it: met or not (null while unknown), in a word or two, and what is wrong.
export type ConditionState = { met: boolean | null; word: string; problem: string | null }

export function conditionState(condition: Condition, checks: Checks): ConditionState {
  if (condition === 'claude') {
    const status = checks.claude
    if (!status) return { met: null, word: 'checking…', problem: null }
    if (!status.problem) return { met: true, word: 'signed in', problem: null }
    const expired = /expired/i.test(status.problem)
    return { met: false, word: expired ? 'login expired' : 'signed out', problem: status.problem }
  }
  const status = checks.gcloud
  if (!status) return { met: null, word: 'checking…', problem: null }
  if (!status.installed)
    return { met: false, word: 'not installed', problem: 'The gcloud CLI is not installed.' }
  if (status.signed_in) return { met: true, word: 'signed in', problem: null }
  return {
    met: false,
    word: checks.signingIn ? 'signing in…' : 'signed out',
    problem: 'gcloud has no active account: sign in to Google Cloud again.',
  }
}

export const usedBy = (condition: Condition, jobs: JobView[]) =>
  condition === 'claude' ? jobs : jobs.filter((j) => j.conditions.includes(condition))

// Google Cloud is shown once gcloud is there, or a job needs it; Claude Code always.
export function shownConditions(checks: Checks, jobs: JobView[]): Condition[] {
  const gcloud = checks.gcloud?.installed || usedBy('gcloud', jobs).length > 0
  return gcloud ? ['claude', 'gcloud'] : ['claude']
}

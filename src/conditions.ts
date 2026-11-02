import type { CommandConditionView } from './domain/CommandConditionView'
import type { ConditionStatus } from './domain/ConditionStatus'
import type { GcloudStatus } from './domain/GcloudStatus'
import type { JobView } from './domain/JobView'

// The last checks of the conditions, as the window has them; null until the first check answers.
export type Checks = {
  claude: ConditionStatus | null
  gcloud: GcloudStatus | null
  // Google Cloud's sign-in happens in the browser, and the app waits for it.
  signingIn: boolean
  // The conditions of the user's own, each a bash command, with its last check.
  commands: CommandConditionView[]
}

export type CheckActions = {
  onSignIn: (condition: string) => void
  onCheck: () => Promise<unknown>
}

const BUILT_IN: Record<string, string> = { claude: 'Claude Code', gcloud: 'Google Cloud' }

export const isBuiltIn = (id: string) => id in BUILT_IN

export const commandOf = (id: string, checks: Checks) => checks.commands.find((c) => c.id === id)

// What a condition is called in a few words: "Google Cloud", "VPN connected".
export const shortName = (id: string, checks: Checks) =>
  BUILT_IN[id] ?? commandOf(id, checks)?.name ?? id

// What a condition is called on its own: "Google Cloud signed in", "VPN connected".
export const fullName = (id: string, checks: Checks) =>
  isBuiltIn(id) ? `${BUILT_IN[id]} signed in` : shortName(id, checks)

// One condition as the window shows it: met or not (null while unknown), in a word or two, and what is wrong.
export type ConditionState = { met: boolean | null; word: string; problem: string | null }

export function conditionState(condition: string, checks: Checks): ConditionState {
  if (condition === 'claude') {
    const status = checks.claude
    if (!status) return { met: null, word: 'checking…', problem: null }
    if (!status.problem) return { met: true, word: 'signed in', problem: null }
    const expired = /expired/i.test(status.problem)
    return { met: false, word: expired ? 'login expired' : 'signed out', problem: status.problem }
  }
  if (condition === 'gcloud') {
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
  const command = commandOf(condition, checks)
  if (!command) return { met: null, word: 'unknown', problem: null }
  if (command.problem === null) return { met: true, word: 'met', problem: null }
  return { met: false, word: 'not met', problem: command.problem }
}

// "Claude Code is signed out", "VPN connected is not met".
export const stateTitle = (id: string, checks: Checks) =>
  `${shortName(id, checks)} is ${conditionState(id, checks).word}`

// Only the built-in conditions have a sign-in, and Google Cloud's needs gcloud.
export const canSignIn = (id: string, checks: Checks) =>
  id === 'claude' || (id === 'gcloud' && !!checks.gcloud?.installed)

// Every job needs Claude Code and the command conditions marked for every job.
const forEveryJob = (id: string, checks: Checks) =>
  id === 'claude' || !!commandOf(id, checks)?.every_job

export const usedBy = (condition: string, jobs: JobView[], checks: Checks) =>
  forEveryJob(condition, checks) ? jobs : jobs.filter((j) => j.conditions.includes(condition))

// Google Cloud is shown once gcloud is there, or a job needs it; Claude Code and the command conditions always.
export function shownConditions(checks: Checks, jobs: JobView[]): string[] {
  const gcloud = checks.gcloud?.installed || usedBy('gcloud', jobs, checks).length > 0
  return ['claude', ...(gcloud ? ['gcloud'] : []), ...checks.commands.map((c) => c.id)]
}

// What a job can tick in the editor: those not already on every job.
export const optionalConditions = (checks: Checks) => [
  'gcloud',
  ...checks.commands.filter((c) => !c.every_job).map((c) => c.id),
]

// What a job waits for, as the Rust side reads it: Claude Code, the conditions for every job, then those it ticked
// that still exist.
export function required(job: Pick<JobView, 'conditions'>, checks: Checks): string[] {
  const all = [
    'claude',
    ...checks.commands.filter((c) => c.every_job).map((c) => c.id),
    ...job.conditions.filter((c) => c === 'gcloud' || commandOf(c, checks)),
  ]
  return [...new Set(all)]
}

export const waitsFor = (unmet: ConditionStatus[], checks: Checks) =>
  unmet.map((c) => shortName(c.condition, checks)).join(' and ')

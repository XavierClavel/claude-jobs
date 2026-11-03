import type { Action } from './domain/Action'
import type { Job } from './domain/Job'
import type { SlackTo } from './domain/SlackTo'

// The two lists of actions a job has, and what each is called.
export type Outcome = 'on_success' | 'on_failure' | 'on_complete'

export const OUTCOMES: [Outcome, string][] = [
  ['on_success', 'On success'],
  ['on_failure', 'On failure'],
  ['on_complete', 'On complete'],
]

// Who gets an action's message: "user U012AB3CD", "channel C012AB3CD".
export const destination = (action: Action) => `${action.target.to} ${action.target.recipient}`

// "Messages user U012AB3CD", "Posts in channel C012AB3CD"
export const destinationText = (action: Action) =>
  `${action.target.to === 'channel' ? 'Posts in' : 'Messages'} ${destination(action)}`

// The text variables every message gives the workflow, and what each holds. The recipient comes on top of them, in a
// variable named after what it is: `user` or `channel`.
export const VARIABLES: [string, string][] = [
  ['text', 'The whole message: the summary, then the result'],
  ['summary', 'How the run went, in a line: ✅ Photobook monitor finished · 2 min 3 s · $0.55'],
  ['result', "Claude's result, or the error"],
  ['job', "The job's name"],
  ['status', 'OK, Failed or Timed out'],
]

// The recipient's variable, its type in Workflow Builder, and the step that sends to it.
export const RECIPIENT: Record<SlackTo, { type: string; step: string }> = {
  user: { type: 'User ID', step: 'Send a message to a person' },
  channel: { type: 'Channel ID', step: 'Send a message to a channel' },
}

// Anyone with a webhook's URL can start the workflow: its last part, the secret, is not shown.
export const maskedUrl = (url: string) => url.replace(/[^/]+$/, '••••••••')

// What a job's action is called. An id with no action, as when actions.json does not parse, shows as it is.
export const actionName = (id: string, actions: Action[]) =>
  actions.find((a) => a.id === id)?.name ?? id

// The jobs that take an action on an outcome.
export const usersOf = <J extends Pick<Job, Outcome>>(id: string, outcome: Outcome, jobs: J[]) =>
  jobs.filter((j) => j[outcome].includes(id))

import type { Action } from './domain/Action'
import type { ActionUse } from './domain/ActionUse'
import type { Job } from './domain/Job'
import type { Recipient } from './domain/Recipient'
import type { SlackTo } from './domain/SlackTo'
import { slug } from './format'

// The lists of actions a job has, and what each is called: those Claude may take while a run goes on, then those
// taken when it ends.
export type ActionList = 'during_run' | 'on_success' | 'on_failure' | 'on_complete'

export const ACTION_LISTS: [ActionList, string][] = [
  ['during_run', 'During the run'],
  ['on_success', 'On success'],
  ['on_failure', 'On failure'],
  ['on_complete', 'On complete'],
]

// What a Slack ID is, as the Rust side tells: U… or W… for a user, C… or G… for a channel. None for a name.
export function slackKind(id: string): SlackTo | null {
  const trimmed = id.trim()
  if (!/^[A-Z0-9]{9,15}$/.test(trimmed)) return null
  if (/^[UW]/.test(trimmed)) return 'user'
  if (/^[CG]/.test(trimmed)) return 'channel'
  return null
}

// "User U012AB3CD", "Channel C012AB3CD"
export const recipientKind = (recipient: Recipient) => {
  const kind = slackKind(recipient.slack_id)
  return `${kind === 'channel' ? 'Channel' : kind === 'user' ? 'User' : 'Slack ID'} ${recipient.slack_id}`
}

// The text variables every message gives the workflow, and what each holds. The recipient comes on top of them, in a
// variable named after what it is: `user` or `channel`.
export const VARIABLES: [string, string][] = [
  ['text', 'The whole message: the summary, then the result'],
  [
    'summary',
    "How the run went, in a line: ✅ Photobook monitor finished · 2 min 3 s · $0.55, or, for a message Claude sends during the run, 💬 and its summary line, or the job's name without one",
  ],
  ['result', "Claude's result, or the error, or the message Claude sent"],
  ['job', "The job's name"],
  ['status', 'OK, Failed or Timed out; Running for a message Claude sends during the run'],
]

// The recipient's variable, its type in Workflow Builder, and the step that sends to it.
export const RECIPIENT: Record<SlackTo, { type: string; step: string }> = {
  user: { type: 'User ID', step: 'Send a message to a person' },
  channel: { type: 'Channel ID', step: 'Send a message to a channel' },
}

// Anyone with a webhook's URL can start the workflow: its last part, the secret, is not shown.
export const maskedUrl = (url: string) => url.replace(/[^/]+$/, '••••••••')

// What a job's action or recipient is called. An id with none, as when its file does not parse, shows as it is.
export const nameOf = (id: string, all: { id: string; name: string }[]) =>
  all.find((a) => a.id === id)?.name ?? id

// The recipient of a use whose recipient Claude gives during the run, by its Slack ID.
export const ANYONE = '*'

// "Private message → Xavier": an action and one of its recipients, or the action alone when it names none.
export function describeUse(used: ActionUse, actions: Action[]) {
  const action = actions.find((a) => a.id === used.action)
  const name = action?.name ?? used.action
  if (!used.recipient) return name
  if (used.recipient === ANYONE) return `${name} → anyone`
  return `${name} → ${nameOf(used.recipient, action?.recipients ?? [])}`
}

// The jobs with a use that `matches` in one of their lists.
export const usersOf = <J extends Pick<Job, ActionList>>(
  matches: (used: ActionUse) => boolean,
  list: ActionList,
  jobs: J[],
) => jobs.filter((j) => j[list].some(matches))

// An id for a recipient called `name` that none has yet: ops-channel, then ops-channel-2.
export function freeId(name: string, recipients: Recipient[]): string {
  const base = slug(name) || 'recipient'
  let id = base
  for (let n = 2; recipients.some((r) => r.id === id); n++) id = `${base.slice(0, 36)}-${n}`
  return id
}

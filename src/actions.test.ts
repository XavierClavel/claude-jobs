import { describeUse, maskedUrl, nameOf, recipientKind, slackKind, usersOf } from './actions'
import { action, job, recipient, use } from './test/fixtures'

describe('actions', () => {
  it('tells a user from a channel by its Slack ID', () => {
    expect(slackKind('U0123ABCD')).toBe('user')
    expect(slackKind(' W0123456789 ')).toBe('user')
    expect(slackKind('C0123ABCD')).toBe('channel')
    expect(slackKind('G0123ABCD')).toBe('channel')
    for (const name of ['@xavier', '#ops', 'u0123abcd', 'U012', 'T0123ABCD']) {
      expect(slackKind(name)).toBeNull()
    }
    expect(recipientKind(recipient())).toBe('Channel C0123ABCD')
    expect(recipientKind(recipient({ slack_id: 'U0123ABCD' }))).toBe('User U0123ABCD')
  })

  it('hides the secret part of a webhook', () => {
    expect(maskedUrl('https://hooks.slack.com/triggers/T0123/456/s3cr3t')).toBe(
      'https://hooks.slack.com/triggers/T0123/456/••••••••',
    )
  })

  it('names an action and its recipient, or keeps the id of one it cannot find', () => {
    expect(nameOf('alert', [action()])).toBe('Alert workflow')
    expect(nameOf('gone', [action()])).toBe('gone')
    expect(describeUse(use(), [action()])).toBe('Alert workflow → Ops channel')
    expect(describeUse(use(''), [action()])).toBe('Alert workflow')
    expect(describeUse(use('*'), [action()])).toBe('Alert workflow → anyone')
    expect(describeUse(use('gone'), [action()])).toBe('Alert workflow → gone')
  })

  it('finds the jobs that send with an action or to a recipient', () => {
    const jobs = [
      job({ id: 'a', on_failure: [use()] }),
      job({ id: 'b', on_success: [use()], on_failure: [use('me')] }),
    ]
    const toOps = (u: { recipient: string }) => u.recipient === 'ops'
    expect(usersOf(toOps, 'on_failure', jobs).map((j) => j.id)).toEqual(['a'])
    expect(usersOf(toOps, 'on_success', jobs).map((j) => j.id)).toEqual(['b'])
    const withAlert = (u: { action: string }) => u.action === 'alert'
    expect(usersOf(withAlert, 'on_failure', jobs)).toHaveLength(2)
    expect(usersOf(toOps, 'during_run', [job({ id: 'd', during_run: [use()] })])).toHaveLength(1)
  })
})

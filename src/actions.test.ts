import { actionName, destination, destinationText, maskedUrl, usersOf } from './actions'
import { action, job } from './test/fixtures'

describe('actions', () => {
  const me = action({
    id: 'me',
    name: 'Me',
    target: { ...action().target, to: 'user', recipient: 'U0123ABCD' },
  })

  it('says where an action posts', () => {
    expect(destination(action())).toBe('channel C0123ABCD')
    expect(destinationText(action())).toBe('Posts in channel C0123ABCD')
    expect(destination(me)).toBe('user U0123ABCD')
    expect(destinationText(me)).toBe('Messages user U0123ABCD')
  })

  it('hides the secret part of a webhook', () => {
    expect(maskedUrl('https://hooks.slack.com/triggers/T0123/456/s3cr3t')).toBe(
      'https://hooks.slack.com/triggers/T0123/456/••••••••',
    )
  })

  it('names an action, or keeps the id of one it cannot find', () => {
    expect(actionName('ops', [action()])).toBe('Ops channel')
    expect(actionName('gone', [action()])).toBe('gone')
  })

  it('finds the jobs that take an action on an outcome', () => {
    const jobs = [
      job({ id: 'a', on_failure: ['ops'] }),
      job({ id: 'b', on_success: ['ops'], on_failure: ['me'] }),
    ]
    expect(usersOf('ops', 'on_failure', jobs).map((j) => j.id)).toEqual(['a'])
    expect(usersOf('ops', 'on_success', jobs).map((j) => j.id)).toEqual(['b'])
    expect(usersOf('ops', 'on_complete', [job({ id: 'c', on_complete: ['ops'] })])).toHaveLength(1)
  })
})

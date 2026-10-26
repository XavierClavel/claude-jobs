import { toDraft, toJob } from './editor'

const defaults = { claude: '/bin/claude', path: '/usr/bin:/bin', cwd: '/Users/me' }

describe('editor drafts', () => {
  it('starts a new job from the defaults, every day at 09:00', () => {
    const draft = toDraft(null, defaults)
    expect(draft.time).toBe('09:00')
    expect(draft.claude).toBe('/bin/claude')
    expect(draft.enabled).toBe(true)
  })

  it('turns the typed fields into a job', () => {
    const job = toJob({
      ...toDraft(null, defaults),
      id: ' weekly ',
      name: 'Weekly',
      prompt: '/weekly',
      time: '18:30',
      timeout: '45',
      weekdays: [5, 1],
      tools: 'Read\nBash(python3:*)',
    })
    expect(job).toMatchObject({
      id: 'weekly',
      hour: 18,
      minute: 30,
      timeout_minutes: 45,
      weekdays: [1, 5],
      allowed_tools: ['Read', 'Bash(python3:*)'],
    })
  })
})

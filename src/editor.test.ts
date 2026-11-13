import { draftErrors, toDraft, toJob, withRepeat } from './editor'

const defaults = { claude: '/bin/claude', path: '/usr/bin:/bin', cwd: '/Users/me' }

describe('editor drafts', () => {
  it('starts a new job from the defaults, every day at 09:00', () => {
    const draft = toDraft(null, defaults)
    expect(draft.time).toBe('09:00')
    expect(draft.repeat).toBe('daily')
    expect(draft.claude).toBe('/bin/claude')
    expect(draft.enabled).toBe(true)
    expect(draft.lastDay).toBe(false)
  })

  it('turns the typed fields into a job', () => {
    const job = toJob({
      ...toDraft(null, defaults),
      id: ' weekly ',
      name: 'Weekly',
      prompt: '/weekly',
      time: '18:30',
      timeout: '45',
      repeat: 'weekly',
      weekdays: [5, 1],
      monthDays: [15],
      tools: 'Read\nBash(python3:*)',
    })
    expect(job).toMatchObject({
      id: 'weekly',
      hour: 18,
      minute: 30,
      timeout_minutes: 45,
      weekdays: [1, 5],
      month_days: [],
      allowed_tools: ['Read', 'Bash(python3:*)'],
    })
  })

  it('sends only the days of the picked repeat', () => {
    const draft = { ...toDraft(null, defaults), weekdays: [1], monthDays: [15, 1] }
    expect(toJob({ ...draft, repeat: 'monthly' })).toMatchObject({
      weekdays: [],
      month_days: [1, 15],
    })
    expect(toJob({ ...draft, repeat: 'daily' })).toMatchObject({ weekdays: [], month_days: [] })
  })

  it('falls back to the last day only for a day some months lack', () => {
    const draft = { ...toDraft(null, defaults), repeat: 'monthly' as const, lastDay: true }
    expect(toJob({ ...draft, monthDays: [31] }).last_day_fallback).toBe(true)
    expect(toJob({ ...draft, monthDays: [1, 15] }).last_day_fallback).toBe(false)
    expect(toJob({ ...draft, monthDays: [31], repeat: 'daily' }).last_day_fallback).toBe(false)
  })

  it('keeps the Chrome option, off for a new job', () => {
    expect(toDraft(null, defaults).chrome).toBe(false)
    const job = toJob({ ...toDraft(null, defaults), chrome: true })
    expect(job.chrome).toBe(true)
    expect(toDraft(job, defaults).chrome).toBe(true)
  })

  it('keeps the conditions a job adds, none for a new one', () => {
    expect(toDraft(null, defaults).conditions).toEqual([])
    const job = toJob({ ...toDraft(null, defaults), conditions: ['gcloud'] })
    expect(job.conditions).toEqual(['gcloud'])
    expect(toDraft(job, defaults).conditions).toEqual(['gcloud'])
  })

  it('opens a monthly job on its days of the month', () => {
    const job = toJob({ ...toDraft(null, defaults), repeat: 'monthly', monthDays: [1] })
    expect(toDraft(job, defaults)).toMatchObject({ repeat: 'monthly', monthDays: [1] })
  })

  it('starts a new week on Monday and a new month on the 1st, and keeps days already picked', () => {
    const draft = toDraft(null, defaults)
    expect(withRepeat(draft, 'weekly')).toMatchObject({ repeat: 'weekly', weekdays: [1] })
    expect(withRepeat(draft, 'monthly')).toMatchObject({ repeat: 'monthly', monthDays: [1] })
    expect(withRepeat({ ...draft, monthDays: [15] }, 'monthly').monthDays).toEqual([15])
  })

  it('wants a day once the repeat is not every day', () => {
    const draft = toDraft(null, defaults)
    expect(draftErrors(draft)).toEqual([])
    expect(draftErrors({ ...draft, repeat: 'weekly' })).toEqual([
      'Pick at least one day of the week.',
    ])
    expect(draftErrors({ ...draft, repeat: 'monthly' })).toEqual([
      'Pick at least one day of the month.',
    ])
    expect(draftErrors({ ...draft, repeat: 'monthly', monthDays: [31] })).toEqual([])
  })
})

describe('cron jobs', () => {
  const draft = toDraft(null, defaults)

  it('starts a cron expression from the schedule picked so far', () => {
    expect(withRepeat(draft, 'cron')).toMatchObject({ repeat: 'cron', cron: '0 9 * * *' })
    const weekly = { ...draft, repeat: 'weekly' as const, time: '18:30', weekdays: [7, 1, 5] }
    expect(withRepeat(weekly, 'cron').cron).toBe('30 18 * * 0,1,5')
    const monthly = { ...draft, repeat: 'monthly' as const, time: '08:05', monthDays: [15, 1] }
    expect(withRepeat(monthly, 'cron').cron).toBe('5 8 1,15 * *')
    // One already typed stays.
    expect(withRepeat({ ...draft, cron: '*/5 * * * *' }, 'cron').cron).toBe('*/5 * * * *')
  })

  it('sends the expression only while Cron is picked', () => {
    const cron = { ...draft, repeat: 'cron' as const, cron: ' */15 9-17 * * 1-5 ', weekdays: [1] }
    expect(toJob(cron)).toMatchObject({ cron: '*/15 9-17 * * 1-5', weekdays: [], month_days: [] })
    expect(toJob({ ...cron, repeat: 'daily' }).cron).toBeNull()
    expect(toDraft(toJob(cron), defaults)).toMatchObject({
      repeat: 'cron',
      cron: '*/15 9-17 * * 1-5',
    })
  })

  it('wants an expression', () => {
    expect(draftErrors({ ...draft, repeat: 'cron', cron: '  ' })).toEqual([
      'Enter a cron expression.',
    ])
    expect(draftErrors({ ...draft, repeat: 'cron', cron: '0 9 * * *' })).toEqual([])
  })
})

describe('pull request jobs', () => {
  const watching = {
    ...toDraft(null, defaults),
    starts: 'trigger' as const,
    trigger: 'pull_request_merged' as const,
    repos: ['o/a', 'o/b'],
    mine: false,
  }

  it('sends the repositories as an event', () => {
    expect(toJob(watching).event).toEqual({
      kind: 'pull_request_merged',
      repos: ['o/a', 'o/b'],
      mine: false,
    })
    expect(toJob({ ...watching, starts: 'schedule' }).event).toBeNull()
  })

  it('needs a repository', () => {
    expect(draftErrors({ ...watching, repos: [] })).toEqual(['Add a repository to watch.'])
    expect(draftErrors({ ...watching, repeat: 'weekly', weekdays: [] })).toEqual([])
  })

  it('opens a saved one on its repositories, and a new one on yours only', () => {
    const draft = toDraft(toJob(watching), defaults)
    expect([draft.starts, draft.trigger, draft.repos, draft.mine]).toEqual([
      'trigger',
      'pull_request_merged',
      ['o/a', 'o/b'],
      false,
    ])
    expect([toDraft(null, defaults).mine, toDraft(null, defaults).trigger]).toEqual([
      true,
      'pull_request_opened',
    ])
  })
})

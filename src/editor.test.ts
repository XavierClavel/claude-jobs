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

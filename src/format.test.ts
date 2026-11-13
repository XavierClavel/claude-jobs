import {
  ago,
  cost,
  day,
  duration,
  ordinal,
  prRef,
  scheduleShort,
  scheduleText,
  slug,
  stopwatch,
  tally,
  triggerShort,
  when,
} from './format'

describe('scheduleText', () => {
  it('says every day when no day is picked', () => {
    expect(
      scheduleText({ hour: 9, minute: 5, weekdays: [], month_days: [], last_day_fallback: false }),
    ).toBe('Every day at 09:05')
  })

  it('lists the picked days', () => {
    expect(
      scheduleText({
        hour: 18,
        minute: 30,
        weekdays: [1, 5, 7],
        month_days: [],
        last_day_fallback: false,
      }),
    ).toBe('Mon, Fri, Sun at 18:30')
  })

  it('lists the days of the month of a monthly job', () => {
    expect(
      scheduleText({
        hour: 8,
        minute: 0,
        weekdays: [],
        month_days: [1, 15],
        last_day_fallback: false,
      }),
    ).toBe('Monthly on the 1st, 15th at 08:00')
  })

  it('says when a shorter month runs on its last day', () => {
    expect(
      scheduleText({ hour: 8, minute: 0, weekdays: [], month_days: [31], last_day_fallback: true }),
    ).toBe('Monthly on the 31st at 08:00, or the last day of shorter months')
  })

  it('writes days of the month as ordinals', () => {
    expect([1, 2, 3, 4, 11, 12, 13, 21, 22, 23, 30, 31].map(ordinal)).toEqual([
      '1st',
      '2nd',
      '3rd',
      '4th',
      '11th',
      '12th',
      '13th',
      '21st',
      '22nd',
      '23rd',
      '30th',
      '31st',
    ])
  })
})

describe('durations, costs and ages', () => {
  it('formats a duration in seconds, then minutes', () => {
    expect(duration(45)).toBe('45 s')
    expect(duration(125)).toBe('2 min 5 s')
    expect(duration(null)).toBe('—')
  })

  it('shows a run still going as a stopwatch, with hours past an hour', () => {
    expect(stopwatch(7)).toBe('0:07')
    expect(stopwatch(754.9)).toBe('12:34')
    expect(stopwatch(3599)).toBe('59:59')
    expect(stopwatch(3725)).toBe('1:02:05')
    expect(stopwatch(-3)).toBe('0:00')
  })

  it('rounds a cost to cents', () => {
    expect(cost(0.553077)).toBe('$0.55')
    expect(cost(undefined)).toBe('—')
  })

  it('says how long ago a run started', () => {
    const now = Date.parse('2026-10-26T15:00:00Z')
    expect(ago('2026-10-26T14:59:30Z', now)).toBe('30 s ago')
    expect(ago('2026-10-26T13:00:00Z', now)).toBe('2 h ago')
  })
})

describe('slug', () => {
  it('turns a name into an id', () => {
    expect(slug('Photobook monitor')).toBe('photobook-monitor')
    expect(slug('  Weekly: Sales report! ')).toBe('weekly-sales-report')
  })
})

describe('scheduleShort', () => {
  it('says the schedule in a few words', () => {
    expect(scheduleShort({ hour: 6, minute: 0, weekdays: [], month_days: [] })).toBe('Daily 06:00')
    expect(scheduleShort({ hour: 9, minute: 0, weekdays: [1], month_days: [] })).toBe(
      'Weekly Mon 09:00',
    )
    expect(scheduleShort({ hour: 9, minute: 0, weekdays: [1, 3, 5], month_days: [] })).toBe(
      'Mon, Wed, Fri 09:00',
    )
    expect(scheduleShort({ hour: 8, minute: 0, weekdays: [], month_days: [1, 15] })).toBe(
      'Monthly 1st, 15th 08:00',
    )
  })

  it('gives a cron expression as it is, whatever the time and days kept', () => {
    const cron = { hour: 9, minute: 0, weekdays: [1], month_days: [], cron: '*/15 9-17 * * 1-5' }
    expect(scheduleShort(cron)).toBe('Cron */15 9-17 * * 1-5')
    expect(scheduleText({ ...cron, last_day_fallback: false })).toBe('Cron */15 9-17 * * 1-5')
    expect(scheduleShort({ ...cron, cron: null })).toBe('Weekly Mon 09:00')
  })
})

describe('day and when', () => {
  // Saturday 31 October 2026, 10:00 local time.
  const now = new Date(2026, 9, 31, 10, 0).getTime()
  const at = (d: number, h = 7, m = 30) => new Date(2026, 9, d, h, m).toISOString()

  it('names the days next to today', () => {
    expect(when(at(31), now)).toBe('Today 07:30')
    expect(when(at(30, 23, 5), now)).toBe('Yesterday 23:05')
    expect(day(at(32), now)).toBe('Tomorrow')
  })

  it('names the weekday within the coming week, and the date otherwise', () => {
    expect(day(at(33), now)).toBe('Mon')
    expect(when(at(29, 9, 12), now)).toBe('Thu 29 Oct 09:12')
  })

  it('has a dash for no time', () => {
    expect(when(null, now)).toBe('—')
  })
})

describe('tally', () => {
  it('counts how the runs went, leaving out those still going', () => {
    expect(tally(['ok', 'ok', 'timeout', 'running', 'failed'])).toEqual([
      '2 OK',
      '1 failed',
      '1 timed out',
    ])
  })
})

describe('pull request jobs', () => {
  const event = {
    kind: 'pull_request_opened' as const,
    repos: ['o/claude-jobs', 'o/api'],
    mine: true,
  }
  const job = { hour: 9, minute: 0, weekdays: [], month_days: [], last_day_fallback: false, event }

  it('say which pull requests start them', () => {
    expect(scheduleText(job)).toBe('Each pull request you open in o/claude-jobs or o/api')
    expect(scheduleText({ ...job, event: { ...event, mine: false, repos: ['o/a'] } })).toBe(
      'Each pull request opened in o/a',
    )
    expect(scheduleShort(job)).toBe('On PR opened')
    const merged = { ...event, kind: 'pull_request_merged' as const, repos: ['o/a'] }
    expect(scheduleText({ ...job, event: merged })).toBe('Each of your pull requests merged in o/a')
    expect(scheduleText({ ...job, event: { ...merged, mine: false } })).toBe(
      'Each pull request merged in o/a',
    )
    expect(scheduleShort({ ...job, event: { ...merged, mine: false } })).toBe('On PR merged')
  })

  it('name the pull request a run was for', () => {
    const pr = {
      repo: 'o/claude-jobs',
      number: 21,
      title: 't',
      url: 'https://github.com/o/claude-jobs/pull/21',
      author: 'me',
      head: 'f',
      base: 'main',
      draft: false,
      created_at: '2026-11-03T10:00:00Z',
      merged_at: null,
    }
    expect(prRef(pr)).toBe('o/claude-jobs#21')
    expect(triggerShort({ trigger: 'pull-request', pull_request: pr })).toBe('PR #21 claude-jobs')
    expect(triggerShort({ trigger: 'catch-up', pull_request: null })).toBe('Catch-up')
  })
})

import { ago, cost, duration, ordinal, scheduleText, slug, stopwatch } from './format'

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
